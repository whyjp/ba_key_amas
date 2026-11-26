//! Where the time between "the screen changes" and "the card changes" goes. No input is ever sent.
//!   cargo run --release -p baocr --example speed_timeline
//! Offline part: fixture frames through key / OCR / classify / observe / present.
//! Card part: the closed-set card reading on quick-formation split into recogniser and CTC scoring.
//! SLEEP=1: how long `thread::sleep` really lasts (the loop waits in 30 ms slices).
//! LIVE=1: read-only desktop captures (`WinDesktop::grab`) split into layout / BitBlt / convert / resize,
//! against `game_capture` and a single-grab prototype.
use baocr_core::ocr_api::Ocr;
use baocr_extract::scene::{classify, Scene};
use baocr_hcu::{detect_region, game_capture, zoom_geometry, Desktop, HcuSession, WinDesktop};
use baocr_pixels::{frame_distance, frame_key};
use image::{imageops, imageops::FilterType, RgbImage};
use std::path::PathBuf;
use std::time::{Duration, Instant};

const RUNS: usize = 7;

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.total_cmp(b));
    v[v.len() / 2]
}

fn time<T>(runs: usize, mut f: impl FnMut() -> T) -> f64 {
    f();
    median((0..runs).map(|_| {
        let t = Instant::now();
        std::hint::black_box(f());
        t.elapsed().as_secs_f64() * 1000.0
    }).collect())
}

fn main() -> anyhow::Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../");
    let models = std::env::var("BAOCR_MODELS").map(PathBuf::from).unwrap_or_else(|_| baocr_ocr::PpOcr::default_models_dir());
    let ocr = baocr_ocr::PpOcr::new(&models)?;

    if std::env::var_os("SLEEP").is_some() {
        for ms in [1u64, 10, 30, 60] {
            let d = time(20, || std::thread::sleep(Duration::from_millis(ms)));
            println!("thread::sleep({ms} ms) lasts {d:.1} ms (median of 20)");
        }
    }

    println!("{:<28} {:>8} {:>10} {:>9} {:>9} {:>6}", "frame", "key ms", "ocr.boxes", "classify", "boxes", "scene");
    for name in ["lobby-idle", "work-hub-full", "unrestricted-raid-select", "quick-formation", "raid-mock-m00"] {
        let path = root.join("fixtures/screens").join(format!("{name}.png"));
        if !path.exists() {
            continue;
        }
        let img = image::open(&path)?.to_rgb8();
        let t_key = time(RUNS, || frame_key(&img));
        let t_ocr = time(3, || ocr.boxes(&img).unwrap_or_default());
        let boxes = ocr.boxes(&img)?;
        let t_cls = time(RUNS, || classify(&boxes, None));
        let scene: Scene = classify(&boxes, None);
        println!("{name:<28} {t_key:>8.2} {t_ocr:>10.0} {t_cls:>9.3} {:>9} {scene:?}", boxes.len());
    }

    // card reading on quick-formation: recogniser run vs CTC scoring of every dictionary name
    let qf = image::open(root.join("fixtures/screens/quick-formation.png"))?.to_rgb8();
    let boxes = ocr.boxes(&qf)?;
    let t_cards = time(3, || baocr_extract::read_card_names(&ocr, &qf, &boxes));
    let n = baocr_extract::read_card_names(&ocr, &qf, &boxes).len();
    // the same label images the reader makes, timed through `line` (recogniser + greedy decode only)
    let labels = baocr_pixels::quick_card_labels(&qf);
    let lines: Vec<RgbImage> = labels.iter().map(|l| baocr_pixels::ink_to_line(&baocr_pixels::quick_label_ink(&qf, l))).collect();
    let t_rec = time(3, || lines.iter().map(|l| ocr.line(l).unwrap().1).sum::<f32>());
    println!("read_card_names on quick-formation: {t_cards:.0} ms for {n} named cards ({} labels); recogniser alone over the same labels: {t_rec:.0} ms", labels.len());

    // frame_distance between a view and itself/other views (cache / settle feasibility)
    let a = image::open(root.join("fixtures/screens/quick-formation.png"))?.to_rgb8();
    let others = ["quick-formation-live-a", "quick-formation-scrolled-a", "quick-formation-scrolled-b", "raid-mock-m00", "raid-mock-m01", "lobby-idle", "work-hub-full", "title-loading-a", "title-loading-b"];
    let ka = frame_key(&a);
    print!("frame_distance from quick-formation (STILL = 1.5):");
    for o in others {
        let p = root.join("fixtures/screens").join(format!("{o}.png"));
        if let Ok(img) = image::open(&p) {
            print!(" {o}={:.2}", frame_distance(&ka, &frame_key(&img.to_rgb8())));
        }
    }
    println!();

    // a cheaper key: the mean of each 40x30 block (what a 32x32 area resize is), one pass over the pixels
    let box_key = |img: &RgbImage| -> Vec<u8> {
        let (w, h) = img.dimensions();
        let (bw, bh) = (w / 32, h / 32);
        let raw = img.as_raw();
        let mut sums = vec![0u32; 32 * 32 * 3];
        for y in 0..(bh * 32) {
            let row = &raw[(y * w * 3) as usize..((y + 1) * w * 3) as usize];
            let by = (y / bh) as usize;
            for bx in 0..32usize {
                let px = &row[bx * bw as usize * 3..(bx + 1) * bw as usize * 3];
                let s = &mut sums[(by * 32 + bx) * 3..(by * 32 + bx) * 3 + 3];
                for p in px.as_chunks::<3>().0 {
                    s[0] += p[0] as u32;
                    s[1] += p[1] as u32;
                    s[2] += p[2] as u32;
                }
            }
        }
        sums.iter().map(|s| (s / (bw * bh)) as u8).collect()
    };
    let dist = |a: &[u8], b: &[u8]| a.iter().zip(b).map(|(x, y)| (*x as i32 - *y as i32).abs()).sum::<i32>() as f64 / a.len() as f64;
    println!("frame_key {:.2} ms vs block-mean key {:.2} ms", time(RUNS, || frame_key(&a)), time(RUNS, || box_key(&a)));
    let bka = box_key(&a);
    print!("frame_distance catmull / block-mean, from quick-formation:");
    for o in others {
        let p = root.join("fixtures/screens").join(format!("{o}.png"));
        if let Ok(img) = image::open(&p) {
            let img = img.to_rgb8();
            print!(" {o}={:.2}/{:.2}", frame_distance(&ka, &frame_key(&img)), dist(&bka, &box_key(&img)));
        }
    }
    println!();

    if std::env::var_os("LIVE").is_some() {
        let session = HcuSession::native();
        let h = session.view();
        let region = detect_region(h)?;
        let desk = WinDesktop::new();
        let (ox, oy, w, hh) = desk.bounds();
        let g = zoom_geometry(region, (w, hh))?;
        println!("live: desktop {w}x{hh} at ({ox},{oy}); game region {region:.1?} -> native rect {:?}; zoom image size {:?}", g.native, g.size);
        let (gx, gy, gw, gh) = (ox + g.native.0, oy + g.native.1, g.native.2 as u32, g.native.3 as u32);
        let t_layout = time(RUNS, || desk.layout());
        let t_grab_full = time(RUNS, || desk.grab(gx, gy, gw, gh).unwrap());
        let t_grab_half = time(RUNS, || desk.grab(gx, gy, gw, gh / 2).unwrap());
        let t_grab_tiny = time(RUNS, || desk.grab(gx, gy, 64, 48).unwrap());
        println!("live: layout() {t_layout:.2} ms; grab client {gw}x{gh} {t_grab_full:.1} ms; grab half {t_grab_half:.1} ms; grab 64x48 {t_grab_tiny:.1} ms");
        let raw = desk.grab(gx, gy, gw, gh)?;
        for (name, f) in [("CatmullRom", FilterType::CatmullRom), ("Triangle", FilterType::Triangle), ("Lanczos3", FilterType::Lanczos3), ("Nearest", FilterType::Nearest)] {
            let t = time(RUNS, || imageops::resize(&raw, 1280, 960, f));
            println!("live: resize {gw}x{gh} -> 1280x960 {name}: {t:.1} ms");
        }
        let t_cap = time(10, || game_capture(h, region).unwrap());
        println!("live: game_capture {t_cap:.1} ms (median of 10)");
        // single-grab prototype: one BitBlt of the client, one resize to the 1280x960 frame
        let single = |f: FilterType| {
            let img = desk.grab(gx, gy, gw, gh).unwrap();
            if img.dimensions() == (1280, 960) { img } else { imageops::resize(&img, 1280, 960, f) }
        };
        let t_single = time(10, || single(FilterType::CatmullRom));
        let t_single_tri = time(10, || single(FilterType::Triangle));
        println!("live: single grab + CatmullRom {t_single:.1} ms; single grab + Triangle {t_single_tri:.1} ms");
        let a = game_capture(h, region)?;
        let b = single(FilterType::CatmullRom);
        let diff = |x: &RgbImage, y: &RgbImage| x.as_raw().iter().zip(y.as_raw()).map(|(p, q)| (*p as i32 - *q as i32).unsigned_abs() as u64).sum::<u64>() as f64 / x.as_raw().len() as f64;
        println!("live: mean abs pixel difference game_capture vs single(CatmullRom) {:.2}; frame_distance of their keys {:.3}", diff(&a, &b), frame_distance(&frame_key(&a), &frame_key(&b)));
        let t_ocr_a = time(3, || ocr.boxes(&a).unwrap_or_default());
        let (ba, bb) = (ocr.boxes(&a)?, ocr.boxes(&b)?);
        println!("live: ocr.boxes {t_ocr_a:.0} ms; game_capture text {} boxes, single-grab text {} boxes, identical text {}", ba.len(), bb.len(), ba.iter().map(|t| &t.text).eq(bb.iter().map(|t| &t.text)));
        let t_key = time(RUNS, || frame_key(&a));
        println!("live: frame_key {t_key:.2} ms");
        a.save(std::env::temp_dir().join("speed_live_a.png"))?;
        b.save(std::env::temp_dir().join("speed_live_b.png"))?;

        // WATCH=secs: look passively at the player's own play and log every change of the view,
        // as the add-on's loop would see it (frame_distance to the previous capture, per capture).
        if let Some(secs) = std::env::var("WATCH").ok().and_then(|s| s.parse::<u64>().ok()) {
            let start = Instant::now();
            let (mut prev, mut run_start, mut last_change): (Option<baocr_pixels::FrameKey>, Option<Instant>, Option<Instant>) = (None, None, None);
            let mut trace = Vec::new();
            while start.elapsed() < Duration::from_secs(secs) {
                let t = Instant::now();
                let img = game_capture(h, region)?;
                let key = frame_key(&img);
                let d = prev.as_ref().map_or(0.0, |p| frame_distance(p, &key));
                prev = Some(key);
                let at = start.elapsed().as_secs_f64();
                if d >= 1.5 {
                    run_start.get_or_insert(t);
                    last_change = Some(t);
                    trace.push(format!("{at:.2}s d={d:.1}"));
                } else if let (Some(s), Some(l)) = (run_start, last_change) {
                    if t.duration_since(l) > Duration::from_millis(400) {
                        println!("view changed: {} captures with motion over {:.0} ms ({})", trace.len(), l.duration_since(s).as_secs_f64() * 1000.0, trace.join(", "));
                        (run_start, last_change) = (None, None);
                        trace.clear();
                    }
                }
                std::thread::sleep(Duration::from_millis(30));
            }
            println!("watch finished after {secs} s");
        }
    }
    Ok(())
}
