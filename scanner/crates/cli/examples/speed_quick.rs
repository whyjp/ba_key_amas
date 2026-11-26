//! 편성 > 빠른 편성 reaction: what each step costs and which shortcuts keep the card reading intact.
//! No game, no input: fixture frames only.
//!   cargo run --release -p baocr --example speed_quick
//! 1. full-frame OCR + card reading (today) vs card reading with the OCR cut to the grid, and with none
//! 2. recogniser vs CTC scoring per card
//! 3. a cache of card names keyed by the label's ink (scroll steps and revisits)
//! 4. popup-open test from pixels, and scene classification from three OCR strips instead of the frame
use baocr_core::ocr_api::{Ocr, TextBox};
use baocr_extract::scene::classify;
use baocr_extract::{read_card_names, CardName};
use baocr_pixels::ink_to_line;
use baocr_pixels::{quick_card_labels, quick_label_ink, quick_tab_is_striker};
use image::{imageops, RgbImage};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

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

/// OCR of a crop, boxes put back in frame coordinates.
fn boxes_in(ocr: &dyn Ocr, img: &RgbImage, rect: [u32; 4]) -> Vec<TextBox> {
    let [x, y, w, h] = rect;
    let crop = imageops::crop_imm(img, x, y, w, h).to_image();
    ocr.boxes(&crop)
        .unwrap_or_default()
        .into_iter()
        .map(|mut b| {
            b.x0 += x as f32;
            b.x1 += x as f32;
            b.y0 += y as f32;
            b.y1 += y as f32;
            b
        })
        .collect()
}

fn brief(c: &[CardName]) -> String {
    c.iter().map(|c| format!("{}@{:.0},{:.0}", c.text, c.x, c.y0)).collect::<Vec<_>>().join(" ")
}

fn main() -> anyhow::Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../fixtures/screens");
    let models = std::env::var("BAOCR_MODELS").map(PathBuf::from).unwrap_or_else(|_| baocr_ocr::PpOcr::default_models_dir());
    let ocr = baocr_ocr::PpOcr::new(&models)?;
    let names = baocr_names::all_names();
    let frames = ["quick-formation", "quick-formation-live-a", "quick-formation-scrolled-a", "quick-formation-scrolled-b"];
    let imgs: Vec<(String, RgbImage)> = frames.iter().map(|n| Ok((n.to_string(), image::open(root.join(format!("{n}.png")))?.to_rgb8()))).collect::<anyhow::Result<_>>()?;
    const GRID: [u32; 4] = [560, 300, 720, 360]; // x, y, w, h: the grid and its tabs

    println!("== 1. card reading with different free-text OCR (ms; cards equal to the full-frame reading?)");
    println!("{:<28} {:>9} {:>9} {:>9} {:>9} {:>9} {:>9}  same cards as full: grid-ocr / no-ocr", "frame", "full ocr", "cards", "grid ocr", "cards", "no ocr", "cards");
    for (name, img) in &imgs {
        let full = ocr.boxes(img)?;
        let t_full = time(3, || ocr.boxes(img).unwrap());
        let ref_cards = read_card_names(&ocr, img, &full);
        let t_cards = time(3, || read_card_names(&ocr, img, &full));
        let grid = boxes_in(&ocr, img, GRID);
        let t_grid = time(3, || boxes_in(&ocr, img, GRID));
        let grid_cards = read_card_names(&ocr, img, &grid);
        let t_grid_cards = time(3, || read_card_names(&ocr, img, &grid));
        let band = |img: &RgbImage| -> Vec<TextBox> {
            quick_card_labels(img)
                .iter()
                .filter_map(|l| {
                    let crop = imageops::crop_imm(img, (l.cx - 46) as u32, l.y0 as u32, 92, (l.y1 - l.y0) as u32).to_image();
                    let (text, score) = ocr.line(&crop).ok()?;
                    Some(TextBox { x0: (l.cx - 46) as f32, y0: l.y0 as f32, x1: (l.cx + 46) as f32, y1: l.y1 as f32, text, score })
                })
                .collect()
        };
        let band_boxes = band(img);
        let t_band = time(3, || band(img));
        let band_cards = read_card_names(&ocr, img, &band_boxes);
        println!("    band-rec free text: {t_band:.0} ms for {} labels, then cards identical to full: {}", band_boxes.len(), band_cards.len() == ref_cards.len() && band_cards.iter().zip(&ref_cards).all(|(x, y)| x.text == y.text));
        if band_cards.iter().zip(&ref_cards).any(|(x, y)| x.text != y.text) {
            println!("    band: {}", brief(&band_cards));
        }
        let none_cards = read_card_names(&ocr, img, &[]);
        let t_none = time(3, || read_card_names(&ocr, img, &[]));
        let same = |a: &[CardName]| a.len() == ref_cards.len() && a.iter().zip(&ref_cards).all(|(x, y)| x.text == y.text && (x.x - y.x).abs() < 2.0 && (x.y0 - y.y0).abs() < 2.0);
        println!("{name:<28} {t_full:>9.0} {t_cards:>9.0} {t_grid:>9.0} {t_grid_cards:>9.0} {:>9} {t_none:>9.0}  {} / {}  ({} cards)", "-", same(&grid_cards), same(&none_cards), ref_cards.len());
        if !same(&grid_cards) {
            println!("    full: {}\n    grid: {}", brief(&ref_cards), brief(&grid_cards));
        }
        if !same(&none_cards) {
            println!("    full: {}\n    none: {}", brief(&ref_cards), brief(&none_cards));
        }
    }

    println!("\n== 2. per card: recogniser (probs) vs closed-set choose over {} names", names.len());
    let (name, img) = &imgs[0];
    let labels = quick_card_labels(img);
    let mut line_total = 0.0;
    let mut choose_total = 0.0;
    for l in &labels {
        let line = ink_to_line(&quick_label_ink(img, l));
        let t_line = time(5, || ocr.line(&line).unwrap());
        let t_choose = time(3, || ocr.choose(&line, &names).unwrap());
        line_total += t_line;
        choose_total += t_choose;
    }
    println!("{name}: {} labels; recogniser total {line_total:.1} ms, choose total {choose_total:.1} ms (CTC scoring share {:.0}%)", labels.len(), (choose_total - line_total) / choose_total * 100.0);
    // the same choose over names pre-filtered to ones that fit the ink width (what the decision does anyway)
    let t_par = time(3, || {
        std::thread::scope(|s| {
            let hs: Vec<_> = labels.iter().map(|l| {
                let line = ink_to_line(&quick_label_ink(img, l));
                let (o, n) = (&ocr, &names);
                s.spawn(move || o.choose(&line, n).unwrap().0)
            }).collect();
            hs.into_iter().map(|h| h.join().unwrap()).sum::<usize>()
        })
    });
    println!("{name}: choose on all labels in parallel threads: {t_par:.1} ms (serial {choose_total:.1} ms)");

    println!("\n== 3. card cache keyed by the label ink");
    let key = |img: &RgbImage, l: &baocr_pixels::CardLabel| -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        quick_label_ink(img, l).as_raw().hash(&mut h);
        h.finish()
    };
    let mut seen: HashMap<u64, String> = HashMap::new();
    for (name, img) in &imgs {
        let ls = quick_card_labels(img);
        let (mut hit, mut miss) = (0, 0);
        for l in &ls {
            if seen.insert(key(img, l), name.clone()).is_some() {
                hit += 1;
            } else {
                miss += 1;
            }
        }
        let t_key = time(5, || ls.iter().map(|l| key(img, l)).sum::<u64>());
        println!("    cards: {}", brief(&read_card_names(&ocr, img, &[])));
        println!("{name}: {} labels, {hit} already seen in earlier frames, {miss} new; hashing all labels {t_key:.2} ms", ls.len());
    }

    // does the same card, seen at another scroll position, give (nearly) the same ink?
    let ink_of = |i: usize| -> Vec<(String, image::GrayImage)> {
        let img = &imgs[i].1;
        let cards = read_card_names(&ocr, img, &[]);
        quick_card_labels(img)
            .into_iter()
            .filter_map(|l| {
                let name = cards.iter().find(|c| (c.x - l.cx as f32).abs() < 2.0 && (c.y0 - l.y0 as f32).abs() < 2.0)?.text.clone();
                Some((name, quick_label_ink(img, &l)))
            })
            .collect()
    };
    for (ia_i, ib_i) in [(0usize, 1usize), (2, 3)] {
    let (a, b) = (ink_of(ia_i), ink_of(ib_i));
    println!("    nearest ink of {} in {}", imgs[ia_i].0, imgs[ib_i].0);
    for (na, ia) in &a {
        let best = b
            .iter()
            .filter(|(_, ib)| ib.dimensions() == ia.dimensions())
            .map(|(nb, ib)| {
                let (mut inter, mut uni) = (0u32, 0u32);
                for (p, q) in ia.pixels().zip(ib.pixels()) {
                    let (x, y) = (p[0] < 128, q[0] < 128);
                    inter += (x && y) as u32;
                    uni += (x || y) as u32;
                }
                (1.0 - inter as f64 / uni.max(1) as f64, nb.clone())
            })
            .min_by(|x, y| x.0.total_cmp(&y.0));
        if let Some((d, nb)) = best {
            println!("      {na:<14} -> {nb:<14} jaccard distance {d:.2} {}", if &nb == na { "(same name)" } else { "" });
        }
    }
    }

    println!("\n== 4. popup open from pixels / scene from strips");
    let all: Vec<String> = std::fs::read_dir(&root)?.filter_map(|e| e.ok()).filter_map(|e| e.path().file_stem().map(|s| s.to_string_lossy().to_string()).filter(|_| e.path().extension().is_some_and(|x| x == "png"))).collect();
    let mut sorted = all.clone();
    sorted.sort();
    for n in &sorted {
        let img = image::open(root.join(format!("{n}.png")))?.to_rgb8();
        let t = time(5, || quick_tab_is_striker(&img));
        let t_labels = time(5, || quick_card_labels(&img));
        let full = ocr.boxes(&img)?;
        let scene_full = classify(&full, None);
        let t0 = Instant::now();
        let mut strips = boxes_in(&ocr, &img, [0, 0, 1280, 70]);
        strips.extend(boxes_in(&ocr, &img, [380, 160, 520, 150]));
        strips.extend(boxes_in(&ocr, &img, [0, 870, 1280, 90]));
        let t_strips = t0.elapsed().as_secs_f64() * 1000.0;
        let scene_strips = classify(&strips, None);
        println!("{n:<34} tab={:?} ({t:.2} ms, labels {t_labels:.2} ms)  scene full={scene_full:?} strips={scene_strips:?} {} strips-ocr {t_strips:.0} ms", quick_tab_is_striker(&img), if scene_full == scene_strips { "same" } else { "DIFFERENT" });
    }
    Ok(())
}
