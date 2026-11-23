#![cfg_attr(windows, windows_subsystem = "windows")]
//! baocr: Blue Archive student profile extractor (Rust).
//!
//! Modes (choose exactly one):
//!   --image <png>        parse a saved 1280x960 profile screenshot (no game input)
//!   --reextract <dir>    re-run extraction from the frames saved in a scan directory (no game input)
//!   --scan-arrows        scan student profiles with the right arrow (drives the game via HCU)
//!   --watch              passive updater: screenshots only, stores changed values
//!   --grab <png>         save the current game region (no input)
mod assist;
mod dump;
mod profile;
mod progress_scan;
mod scan;
use anyhow::{bail, Context, Result};
use baocr_core::ocr_api::Ocr;
use baocr_export::{Index, Store};
use baocr_extract::{Extractor, Options, FRAME_MYSTIC};
use baocr_hcu::{check_aspect, require_ratio43, detect_region, game_capture, parse_frame_region, Hcu, HcuSession, HcuSource, Watcher};
use baocr_ocr::PpOcr;
use baocr_overlay::{BannerState, Overlay};
use baocr_pipeline::reextract;
use clap::Parser;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Parser)]
#[command(name = "baocr", about = "Read Blue Archive student profile screens into data (4:3 or 16:9, windowed; the student scan needs 4:3)")]
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
    /// Read the account progress (schedule ranks, event story stamps) by walking the game's screens (HCU).
    #[arg(long)]
    scan_progress: bool,
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
    /// Check the game window, DPI, monitors and permissions that sending input depends on, then exit. Sends no input.
    #[arg(long)]
    doctor: bool,
    /// Passive guide: recognise the game screen and show it, and for a picked mission its terrain and enemy types, on a card beside the game window. Screenshots only, no input.
    #[arg(long)]
    assist: bool,
    /// --assist: the wiki database (build it with the `wikidb` tool).
    #[arg(long)]
    wiki: Option<PathBuf>,
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
    /// Output directory (default: the add-on's student folder, %APPDATA%\baocr\students).
    #[arg(long)]
    out: Option<PathBuf>,
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

impl Args {
    /// Where students.json, index.json and details/ go: `--out`, else the add-on's own folder.
    fn out_dir(&self) -> PathBuf {
        self.out.clone().unwrap_or_else(|| std::env::var_os("APPDATA").map(|d| PathBuf::from(d).join("baocr").join("students")).unwrap_or_else(|| PathBuf::from("students")))
    }
}

/// The wiki database: `--wiki`, else `wikidb/wiki.db` beside the exe (portable layout), else the
/// source tree's.
fn wiki_path(args: &Args) -> PathBuf {
    if let Some(p) = &args.wiki {
        return p.clone();
    }
    let beside = std::env::current_exe().ok().and_then(|e| e.parent().map(|d| d.join("wikidb").join("wiki.db")));
    beside.into_iter().chain(["wikidb/wiki.db", "../wikidb/wiki.db"].map(PathBuf::from)).find(|p| p.exists()).unwrap_or_else(|| PathBuf::from("wikidb/wiki.db"))
}

#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn AttachConsole(pid: u32) -> i32;
}
#[cfg(windows)]
#[link(name = "user32")]
extern "system" {
    fn MessageBoxW(hwnd: isize, text: *const u16, caption: *const u16, kind: u32) -> i32;
}

/// The exe has no console of its own (it is an add-on); started from a terminal it prints there.
fn attach_parent_console() {
    #[cfg(windows)]
    unsafe {
        AttachConsole(u32::MAX); // ATTACH_PARENT_PROCESS
    }
}

/// A failure of the windowless exe must not vanish: log it and tell the player.
fn report_fatal(e: &anyhow::Error) {
    eprintln!("error: {e:#}");
    let text = format!("{e:#}");
    if let Some(dir) = std::env::var_os("APPDATA").map(|d| PathBuf::from(d).join("baocr")) {
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(dir.join("error.log"), &text);
    }
    #[cfg(windows)]
    unsafe {
        let wide = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
        MessageBoxW(0, wide(&text).as_ptr(), wide("ba-scanner 애드온").as_ptr(), 0x10);
    }
}

fn main() {
    attach_parent_console();
    if let Err(e) = real_main() {
        report_fatal(&e);
        std::process::exit(1);
    }
}

fn load_ocr(args: &Args) -> Result<PpOcr> {
    let ocr = PpOcr::new(&args.models.clone().unwrap_or_else(PpOcr::default_models_dir))?;
    if ocr.device() != "cpu" {
        eprintln!("OCR runs on the GPU ({}); BAOCR_OCR_DEVICE=cpu turns that off", ocr.device());
    }
    Ok(ocr)
}

fn real_main() -> Result<()> {
    let args = Args::parse();
    if args.doctor {
        let report = baocr_hcu::run_doctor();
        println!("{}", report.render());
        return Ok(());
    }
    let modes = [args.boxes.is_some(), args.image.is_some(), args.reextract.is_some(), args.scan_arrows, args.scan_progress, args.watch, args.grab.is_some(), args.learn.is_some(), args.assist];
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
        let ocr: Arc<dyn Ocr> = Arc::new(load_ocr(&args)?);
        return assist::run(ocr, &HcuSession::default_backend(), &args.region, &wiki_path(&args), &args.out_dir(), args.workers);
    }

    // Everything below drives or watches the game.
    let session = HcuSession::default_backend();
    let hcu = session.view();
    let region = if args.region == "auto" {
        let r = detect_region(hcu)?;
        println!("game window found: --region {:.2},{:.2},{:.2},{:.2}", r[0], r[1], r[2], r[3]);
        r
    } else {
        parse_frame_region(&args.region)?
    };
    let layout = check_aspect(region)?;
    println!("screen ratio {}", layout.label());

    if let Some(path) = &args.grab {
        session.exclusive().activate()?;
        game_capture(hcu, region)?.save(path)?;
        return Ok(());
    }
    if let Some(dir) = &args.learn {
        return run_learn(&args, hcu, region, dir);
    }
    if args.watch {
        if layout == baocr_hcu::Layout::Ratio169 {
            println!("16:9: profile screens are not read (the profile readers are calibrated for 4:3); the watcher stays idle");
        }
        return run_watch(&args, hcu, region, opts);
    }
    if args.scan_progress {
        return run_progress(&args, &session, region);
    }
    // the scan clicks at 4:3 frame coordinates: say so before anything is loaded or touched
    require_ratio43(region)?;
    run_scan(&args, &session, region, opts)
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
    let out = args.out_dir();
    std::fs::create_dir_all(&out)?;
    img.save(out.join("detail.png"))?;
    baocr_export::export_student(&s, &out)?;
    println!("{}", serde_json::to_string_pretty(&s)?);
    Ok(())
}

fn run_progress(args: &Args, session: &HcuSession, region: [f64; 4]) -> Result<()> {
    let overlay = (!args.no_overlay).then(Overlay::start);
    let report = |s: BannerState| {
        if let Some(o) = &overlay {
            o.set(s);
        }
    };
    let ocr: Arc<dyn Ocr> = Arc::new(load_ocr(args)?);
    let summary = progress_scan::scan_progress(ocr, &session.exclusive(), region, &progress_dir(), &report);
    if overlay.is_some() {
        std::thread::sleep(std::time::Duration::from_secs(3));
    }
    let s = summary?;
    println!("progress scan: {} sections read, file {}", s.sections_read, s.file.display());
    for w in &s.warnings {
        println!("  warning: {w}");
    }
    Ok(())
}

/// Where the progress scan writes: %APPDATA%\baocr\progress.
pub(crate) fn progress_dir() -> PathBuf {
    std::env::var_os("APPDATA").map(|d| PathBuf::from(d).join("baocr").join("progress")).unwrap_or_else(|| PathBuf::from("progress"))
}

fn run_scan(args: &Args, session: &HcuSession, region: [f64; 4], opts: Options) -> Result<()> {
    let overlay = (!args.no_overlay).then(Overlay::start);
    let report = |s: BannerState| {
        if let Some(o) = &overlay {
            o.set(s);
        }
    };
    let ocr: Arc<dyn Ocr> = Arc::new(load_ocr(args)?);
    let out = args.out_dir();
    let req = scan::ScanRequest { out: &out, deep: !args.fast, workers: args.workers, max_students: args.max_students, from_first: args.from_first };
    let summary = scan::scan_students(ocr, &session.exclusive(), region, &req, opts, &report);
    if overlay.is_some() {
        // let the final state be read
        std::thread::sleep(std::time::Duration::from_secs(if summary.is_ok() { 3 } else { 4 }));
    }
    match summary?.error {
        Some(e) => bail!(e),
        None => Ok(()),
    }
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
    let out = args.out_dir();
    let store = Store::open(&out)?;
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
    println!("watching (screenshots only, no input); Ctrl-C to stop; store: {}", out.display());
    w.run(&stop, |u| println!("update: {u}"), |e| eprintln!("watch: {e}"));
    Ok(())
}
