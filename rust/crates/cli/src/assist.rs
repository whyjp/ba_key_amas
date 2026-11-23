//! `--assist`: a passive guide. It only looks at the game (screenshots, no input) and draws
//! cards and icons from what it sees. This file is only the loop: capture a frame, read its
//! text, hand it to the assistant, and let the assistant handle clicks on its buttons.
//! Everything else lives in `baocr-assist`.
use anyhow::{bail, Result};
use baocr_assist::{Assistant, Observer, Settings};
use baocr_core::ocr_api::Ocr;
use baocr_extract::scene::Scene;
use baocr_hcu::{detect_region, game_capture, screen_kind, Hcu, Screen};
use baocr_overlay::Overlay;
use baocr_pixels::{frame_distance, frame_key};
use baocr_wikidb::Wiki;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Frames this alike are the same view; two such frames in a row mean the screen has settled.
const STILL: f64 = 1.5;
/// Re-detect the game window this often (loops), so moving or resizing it is followed.
const REGION_EVERY: u32 = 25;
/// Pause between loops: short, so a click on a button is answered quickly.
const LOOP_PAUSE: Duration = Duration::from_millis(150);

pub fn run(ocr: &dyn Ocr, hcu: &Hcu, mut region: [f64; 4], auto_region: bool, wiki_path: &Path) -> Result<()> {
    if !wiki_path.exists() {
        bail!("wiki database not found at {}: build it with `cargo run -p baocr-wikidb --release`", wiki_path.display());
    }
    let wiki = Wiki::open(wiki_path)?;
    let (students, types) = (wiki.students()?, wiki.type_table()?);
    let settings_path = Settings::default_path();
    let mut assistant = Assistant::new(Observer::new(ocr, &students), &wiki, &students, &types, Settings::load(&settings_path), Some(settings_path.clone()), Overlay::start_hidden());
    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    ctrlc::set_handler(move || flag.store(true, Ordering::Relaxed))?;
    println!("assist: watching the game (screenshots only, no input); settings: {}; Ctrl-C to stop", settings_path.display());

    let (mut prev, mut handled, mut still, mut loops) = (None, None, 0u32, 0u32);
    while !stop.load(Ordering::Relaxed) {
        loops += 1;
        assistant.poll();
        if auto_region && loops % REGION_EVERY == 0 {
            if let Ok(r) = detect_region(hcu) {
                region = r;
            }
        }
        let img = match game_capture(hcu, region) {
            Ok(i) => i,
            Err(e) => {
                eprintln!("assist: capture failed: {e:#}");
                std::thread::sleep(Duration::from_millis(1000));
                continue;
            }
        };
        let key = frame_key(&img);
        still = if prev.as_ref().is_some_and(|p| frame_distance(p, &key) < STILL) { still + 1 } else { 0 };
        prev = Some(key);
        let new_view = handled.as_ref().map_or(true, |h| frame_distance(h, &key) > STILL);
        if still >= 1 && new_view {
            let boxes = ocr.boxes(&img).unwrap_or_default();
            // the student list and profile are told apart by pixels, only when the text names no screen
            let fallback = match screen_kind(&img) {
                Screen::Profile => Scene::StudentProfile,
                Screen::Roster => Scene::StudentList,
                Screen::Other => Scene::Unknown,
            };
            let scene = assistant.on_frame(&img, &boxes, fallback);
            println!("assist: {scene:?}");
            handled = Some(key);
        }
        std::thread::sleep(LOOP_PAUSE);
    }
    Ok(())
}
