//! baocr: Blue Archive student profile extractor (Rust).
//!
//! Modes (choose exactly one):
//!   --image <png>        parse a saved 1280x960 profile screenshot (no game input)
//!   --reextract <dir>    re-run extraction from the frames saved in a scan directory (no game input)
//!   --scan-arrows        scan student profiles with the right arrow (drives the game via HCU)
//!   --watch              passive updater: screenshots only, stores changed values
//!   --grab <png>         save the current game region (no input)
use anyhow::{bail, Context, Result};
use baocr_core::model::RosterReport;
use baocr_core::ocr_api::Ocr;
use baocr_export::{Index, Store};
use baocr_extract::{Extractor, Options, FRAME_MYSTIC};
use baocr_hcu::{check_aspect, detect_region, game_capture, new_capture, parse_frame_region, scan_arrows, Hcu, HcuSource, Watcher};
use baocr_ocr::PpOcr;
use baocr_pipeline::{reextract, Pipeline};
use clap::Parser;
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
    #[arg(long, default_value_t = 300)]
    max_students: usize,
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
    let modes = [args.image.is_some(), args.reextract.is_some(), args.scan_arrows, args.watch, args.grab.is_some()];
    if modes.iter().filter(|m| **m).count() != 1 {
        bail!("choose exactly one of --image, --reextract, --scan-arrows, --watch, --grab");
    }
    if args.from_first && !args.scan_arrows {
        bail!("--from-first requires --scan-arrows");
    }
    let opts = Options { verify_fixed: args.verify_fixed };

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

fn run_scan(args: &Args, hcu: &Hcu, region: [f64; 4], opts: Options) -> Result<()> {
    let deep = !args.fast;
    let ocr: Arc<dyn Ocr> = Arc::new(load_ocr(args)?);
    let pipeline = Pipeline::new(&args.out, ocr, opts, deep, args.workers)?;
    let outcome = scan_arrows(hcu, region, &args.out, args.max_students, args.from_first, deep, &pipeline)?;
    let base = RosterReport { schema_version: 3, capture: Some(new_capture(region)), ..Default::default() };
    let rep = pipeline.finish(base, &outcome.stop_reason, outcome.wrapped, args.from_first)?;
    println!(
        "arrows: entries={} complete={} reason={} unresolved={}",
        rep.entries.len(),
        rep.complete,
        rep.stop_reason,
        rep.unresolved.len()
    );
    if let Some(e) = outcome.error {
        bail!(e);
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
