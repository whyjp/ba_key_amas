//! baocr: Blue Archive student profile extractor (Rust).
//!
//! Modes (choose exactly one):
//!   --image <png>        parse a saved 1280x960 profile screenshot (no game input)
//!   --reextract <dir>    re-run extraction from the frames saved in a scan directory (no game input)
//!   --scan-arrows        scan student profiles with the right arrow (drives the game via HCU)
//!   --watch              passive updater: screenshots only, stores changed values
//!   --grab <png>         save the current game region (no input)
mod assist;
use anyhow::{bail, Context, Result};
use baocr_core::model::RosterReport;
use baocr_core::ocr_api::{Ocr, ScanSink};
use baocr_export::{Index, Store};
use baocr_extract::{Extractor, Options, FRAME_MYSTIC};
use baocr_hcu::{check_aspect, detect_region, ensure_profile, prepare_scan, game_capture, new_capture, parse_frame_region, scan_arrows, Hcu, HcuSource, Watcher};
use baocr_ocr::PpOcr;
use baocr_overlay::Overlay;
use baocr_pipeline::{reextract, Pipeline};
use clap::Parser;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Parser)]
#[command(name = "baocr", about = "Read Blue Archive student profile screens into data (4:3 layout, windowed)")]
struct Args {
    /// Parse an existing 1280x960 profile screenshot (no desktop input).
    #[arg(long)]
    image: Option<PathBuf>,
    /// Re-run extraction from the frames saved in this output directory (no game input).
    #[arg(long)]
    reextract: Option<PathBuf>,
    /// With --reextract: comma-separated student ids (e.g. 003,007).
    #[arg(long)]
    ids: Option<String>,
    /// Scan sequential student profiles with the right arrow (HCU).
    #[arg(long)]
    scan_arrows: bool,
    /// --scan-arrows starts at the first student of the roster.
    #[arg(long)]
    from_first: bool,
    /// --scan-arrows: skip the menu pass (상세 popup, 레벨 업, 신비 해방 tabs).
    #[arg(long)]
    fast: bool,
    /// Passive background updater (screenshots only, no input).
    #[arg(long)]
    watch: bool,
    /// --watch frame interval in milliseconds.
    #[arg(long, default_value_t = 1000)]
    interval: u64,
    /// Save the current game region to this PNG (no input, no OCR).
    #[arg(long)]
    grab: Option<PathBuf>,
    /// Print the text boxes OCR reads on this saved 1280x960 frame (no game access).
    #[arg(long)]
    boxes: Option<PathBuf>,
    /// Passive guide: recognise the game screen and show it, and for a picked mission its terrain and enemy types, on a card beside the game window. Screenshots only, no input.
    #[arg(long)]
    assist: bool,
    /// --assist: the wiki database (build it with the `wikidb` tool).
    #[arg(long, default_value = "../wikidb/wiki.db")]
    wiki: PathBuf,
    /// Learn screens: while you play, save every new stable screen (PNG + the text read on it) into this folder. Screenshots only, no input.
    #[arg(long)]
    learn: Option<PathBuf>,
    #[arg(long, default_value_t = 300)]
    max_students: usize,
    /// Do not show the progress banner over the game while scanning.
    #[arg(long)]
    no_overlay: bool,
    /// Concurrent extraction workers.
    #[arg(long, default_value_t = 3)]
    workers: usize,
    /// Output directory.
    #[arg(long, default_value = "output-rs")]
    out: PathBuf,
    /// HCU frame rectangle of the Blue Archive client "x0,y0,x1,y1", or "auto" to find the game window.
    #[arg(long, default_value = "auto")]
    region: String,
    /// Also read fixed meta (role/type/terrain) from pixels and warn on disagreement with the dictionary.
    #[arg(long)]
    verify_fixed: bool,
    /// Directory with the OCR models (default: $BAOCR_MODELS, ./models, ../models).
    #[arg(long)]
    models: Option<PathBuf>,
}

fn load_ocr(args: &Args) -> Result<PpOcr> {
    PpOcr::new(&args.models.clone().unwrap_or_else(PpOcr::default_models_dir))
}

fn main() -> Result<()> {
    let args = Args::parse();
    let modes = [args.boxes.is_some(), args.image.is_some(), args.reextract.is_some(), args.scan_arrows, args.watch, args.grab.is_some(), args.learn.is_some(), args.assist];
    let chosen = modes.iter().filter(|m| **m).count();
    if chosen > 1 {
        bail!("choose one of --image, --reextract, --scan-arrows, --watch, --grab, --learn, --assist (none starts the add-on)");
    }
    if args.from_first && !args.scan_arrows {
        bail!("--from-first requires --scan-arrows");
    }
    let opts = Options { verify_fixed: args.verify_fixed };
    if let Some(png) = &args.boxes {
        let img = image::open(png)?.to_rgb8();
        for b in load_ocr(&args)?.boxes(&img)? {
            println!("{:.0},{:.0},{:.0},{:.0}	{}", b.x0, b.y0, b.x1, b.y1, b.text.trim());
        }
        return Ok(());
    }

    if let Some(path) = &args.image {
        return run_image(&args, path, &opts);
    }
    if let Some(dir) = &args.reextract {
        let ocr: Arc<dyn Ocr> = Arc::new(load_ocr(&args)?);
        // Menu frames present in the index mean the scan ran the menu pass.
        let deep = Index::load(dir)?.sorted().iter().any(|e| e.frames.iter().any(|f| f == FRAME_MYSTIC));
        let ids: Vec<String> = args.ids.as_deref().map(|s| s.split(',').map(|x| x.trim().to_string()).collect()).unwrap_or_default();
        let rep = reextract(dir, &ids, ocr, opts, deep, args.workers)?;
        println!("reextract: entries={} unresolved={}", rep.entries.len(), rep.unresolved.len());
        return Ok(());
    }

    // The add-on (the default): it finds the game window itself, whenever that appears.
    if chosen == 0 || args.assist {
        let ocr = load_ocr(&args)?;
        return assist::run(&ocr, &Hcu::from_env(), &args.region, &args.wiki);
    }

    // Everything below drives or watches the game.
    let hcu = Hcu::from_env();
    let region = if args.region == "auto" {
        let r = detect_region(&hcu)?;
        println!("game window found: --region {:.2},{:.2},{:.2},{:.2}", r[0], r[1], r[2], r[3]);
        r
    } else {
        parse_frame_region(&args.region)?
    };
    check_aspect(region)?;

    if let Some(path) = &args.grab {
        hcu.activate()?;
        game_capture(&hcu, region)?.save(path)?;
        return Ok(());
    }
    if let Some(dir) = &args.learn {
        return run_learn(&args, &hcu, region, dir);
    }
    if args.watch {
        return run_watch(&args, &hcu, region, opts);
    }
    run_scan(&args, &hcu, region, opts)
}

fn run_image(args: &Args, path: &PathBuf, opts: &Options) -> Result<()> {
    let ocr = load_ocr(args)?;
    let img = image::open(path).with_context(|| format!("opening {}", path.display()))?.to_rgb8();
    if img.dimensions() != (1280, 960) {
        bail!("detail image must be 1280x960 (4:3 layout), got {:?}", img.dimensions());
    }
    let s = Extractor::new(&ocr).profile(&img, &path.display().to_string(), opts)?;
    if s.name.is_empty() {
        bail!("detail OCR did not find a student name; no export");
    }
    std::fs::create_dir_all(&args.out)?;
    img.save(args.out.join("detail.png"))?;
    baocr_export::export_student(&s, &args.out)?;
    println!("{}", serde_json::to_string_pretty(&s)?);
    Ok(())
}

/// Forwards frames to the pipeline and moves the banner's n/m forward.
struct ProgressSink<'a> {
    inner: &'a Pipeline,
    overlay: Option<&'a Overlay>,
    total: Option<usize>,
}

impl ScanSink for ProgressSink<'_> {
    fn submit(&self, i: usize, frames: BTreeMap<String, image::RgbImage>) {
        self.inner.submit(i, frames);
        if let Some(o) = self.overlay {
            o.scanning(i + 1, self.total);
        }
    }
    fn name_of(&self, i: usize) -> String {
        self.inner.name_of(i)
    }
    fn peek_name(&self, img: &image::RgbImage) -> String {
        self.inner.peek_name(img)
    }
    fn first_student(&self) -> Result<baocr_core::model::Student> {
        self.inner.first_student()
    }
    fn aborted(&self) -> Option<(String, String)> {
        self.inner.aborted()
    }
}

/// Plain-language reason shown on the banner when a scan stops early.
fn stop_note(reason: &str) -> String {
    match reason {
        "focus_lost" => "게임 창이 활성 상태가 아니어서 중단했습니다".into(),
        "layout_mismatch" => "4:3 창모드 화면이 아니어서 중단했습니다".into(),
        "two_empty_names" => "학생 이름을 읽지 못해 중단했습니다".into(),
        "deep_failed" => "메뉴 화면 캡처에 실패해 중단했습니다".into(),
        "repeated_profile" => "같은 학생이 반복되어 중단했습니다".into(),
        other => format!("중단됨: {other}"),
    }
}

fn run_scan(args: &Args, hcu: &Hcu, region: [f64; 4], opts: Options) -> Result<()> {
    let deep = !args.fast;
    let overlay = (!args.no_overlay).then(Overlay::start);
    let ov = overlay.as_ref();

    // Learn the total first (scroll through the roster once) so progress reads n/m,
    // and start from the first student.
    if let Some(o) = ov {
        o.counting();
    }
    let ocr: Arc<dyn Ocr> = Arc::new(load_ocr(args)?);
    // The main menu is recognised by its bottom labels, and 학생 is found there.
    let student_button = |img: &image::RgbImage| baocr_extract::lobby_student_button(&*ocr, img);
    let total = match prepare_scan(hcu, region, &student_button, |n| {
        if let Some(o) = ov {
            o.counting_n(n);
        }
    }) {
        Ok(t) => {
            println!("students in the roster: {t}");
            Some(t)
        }
        Err(e) => {
            eprintln!("could not count the roster ({e:#}); scanning from the open profile without a total");
            if let Err(e2) = ensure_profile(hcu, region, &student_button) {
                if let Some(o) = ov {
                    o.failed(0, None, "학생 목록, 학생 프로필 또는 메인 메뉴 화면을 먼저 열어 주세요");
                    std::thread::sleep(std::time::Duration::from_secs(4));
                }
                return Err(e2);
            }
            None
        }
    };
    let from_first = args.from_first || total.is_some();
    if let Some(o) = ov {
        o.scanning(0, total);
    }

    let pipeline = Pipeline::new(&args.out, ocr.clone(), opts, deep, args.workers)?;
    let sink = ProgressSink { inner: &pipeline, overlay: ov, total };
    hcu.stats_reset();
    let scan_started = std::time::Instant::now();
    let outcome = scan_arrows(hcu, region, &args.out, args.max_students, from_first, deep, &sink)?;
    if std::env::var_os("BAOCR_HCU_STATS").is_some() {
        eprintln!("scan loop {:.1}s; HCU time by command:\n{}", scan_started.elapsed().as_secs_f64(), hcu.stats_report());
    }
    let base = RosterReport { schema_version: 3, capture: Some(new_capture(region)), ..Default::default() };
    let rep = pipeline.finish(base, &outcome.stop_reason, outcome.wrapped, from_first)?;
    println!(
        "arrows: entries={} complete={} reason={} unresolved={}",
        rep.entries.len(),
        rep.complete,
        rep.stop_reason,
        rep.unresolved.len()
    );
    if let Some(o) = ov {
        let n = rep.entries.len();
        if outcome.error.is_none() || matches!(outcome.stop_reason.as_str(), "wrapped_to_first" | "max_students") {
            o.finished(n, total.or(Some(n)));
        } else {
            o.failed(n, total, &stop_note(&outcome.stop_reason));
        }
        std::thread::sleep(std::time::Duration::from_secs(3)); // let the final state be read
    }
    if let Some(e) = outcome.error {
        bail!(e);
    }
    Ok(())
}

/// Passive screen learning: every stable frame that differs from the last saved one is
/// written with the text boxes read on it, so screen layouts can be studied offline.
fn run_learn(args: &Args, hcu: &Hcu, region: [f64; 4], dir: &std::path::Path) -> Result<()> {
    use baocr_pixels::{frame_distance, frame_key};
    let ocr = load_ocr(args)?;
    std::fs::create_dir_all(dir)?;
    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    ctrlc::set_handler(move || flag.store(true, Ordering::Relaxed))?;
    println!("learning screens (screenshots only, no input); play normally, Ctrl-C to stop; saving to {}", dir.display());
    let (mut prev, mut saved, mut still, mut n) = (None, None, 0u32, 0usize);
    while !stop.load(Ordering::Relaxed) {
        let img = game_capture(hcu, region)?;
        let key = frame_key(&img);
        still = match &prev {
            Some(p) if frame_distance(p, &key) < 1.5 => still + 1,
            _ => 0,
        };
        prev = Some(key);
        let new_view = saved.as_ref().map_or(true, |s| frame_distance(s, &key) > 5.0);
        if still >= 2 && new_view {
            n += 1;
            let stem = dir.join(format!("{n:03}"));
            img.save(stem.with_extension("png"))?;
            let mut boxes = ocr.boxes(&img)?;
            boxes.sort_by(|a, b| (a.y0 as i32 / 20, a.x0 as i32).cmp(&(b.y0 as i32 / 20, b.x0 as i32)));
            let lines: Vec<String> = boxes.iter().map(|b| format!("{:.0},{:.0},{:.0},{:.0}	{}", b.x0, b.y0, b.x1, b.y1, b.text.trim())).collect();
            std::fs::write(stem.with_extension("txt"), lines.join("
"))?;
            let title = boxes.iter().find(|b| b.y1 < 60.0 && b.x0 > 80.0).map(|b| b.text.trim().to_string()).unwrap_or_default();
            println!("saved {n:03}  title: {title:?}  ({} texts)", boxes.len());
            saved = Some(key);
        }
        std::thread::sleep(std::time::Duration::from_millis(args.interval.min(600)));
    }
    Ok(())
}

fn run_watch(args: &Args, hcu: &Hcu, region: [f64; 4], opts: Options) -> Result<()> {
    let ocr = load_ocr(args)?;
    let store = Store::open(&args.out)?;
    let ex = Extractor::new(&ocr);
    let mut w = Watcher::new(
        HcuSource { hcu, region },
        |img: &image::RgbImage| ex.profile(img, "watch", &opts),
        |s: &baocr_core::model::Student| {
            let (changes, is_new) = store.upsert(s, chrono::Utc::now())?;
            Ok((changes.into_iter().map(|c| format!("{} {}->{}", c.field, c.from, c.to)).collect(), is_new))
        },
    );
    w.interval = std::time::Duration::from_millis(args.interval);
    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    ctrlc::set_handler(move || flag.store(true, Ordering::Relaxed))?;
    println!("watching (screenshots only, no input); Ctrl-C to stop; store: {}", args.out.display());
    w.run(&stop, |u| println!("update: {u}"), |e| eprintln!("watch: {e}"));
    Ok(())
}
