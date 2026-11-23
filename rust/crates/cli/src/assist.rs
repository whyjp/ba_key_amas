//! The add-on: a passive guide that lives beside the game window. It only looks at the game
//! (screenshots, no input) and draws cards and icons from what it sees. This file is only the
//! loop: wait for the game, capture a frame, read its text, hand it to the assistant, and let
//! the assistant handle clicks on its buttons. Everything else lives in `baocr-assist`.
//!
//! It can be started before the game, after it, or while it restarts: the window is looked
//! for again whenever it is not known, and forgotten when it stops answering.
use anyhow::{bail, Result};
use crate::scan::{self, ScanRequest};
use baocr_assist::{read_library, Assistant, Command, Observer, Settings};
use baocr_core::ocr_api::Ocr;
use baocr_extract::scene::Scene;
use baocr_extract::Options;
use baocr_hcu::{check_aspect, detect_region, game_capture, parse_frame_region, screen_kind, Hcu, Screen};
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
/// Pause while there is no game to look at.
const WAIT_PAUSE: Duration = Duration::from_millis(1000);
/// Capture failures in a row before the window is given up and looked for again.
const MAX_CAPTURE_FAILURES: u32 = 3;

/// The game's client area in HCU frame pixels, if the game is there and shaped as expected.
fn find_region(hcu: &Hcu, spec: &str) -> Result<[f64; 4]> {
    let region = if spec == "auto" { detect_region(hcu)? } else { parse_frame_region(spec)? };
    check_aspect(region)?;
    Ok(region)
}

pub fn run(ocr: Arc<dyn Ocr>, hcu: &Hcu, region_spec: &str, wiki_path: &Path, out: &Path, workers: usize) -> Result<()> {
    if !wiki_path.exists() {
        bail!("wiki database not found at {}: build it with `cargo run -p baocr-wikidb --release`", wiki_path.display());
    }
    let wiki = Wiki::open(wiki_path)?;
    let (students, types) = (wiki.students()?, wiki.type_table()?);
    let settings_path = Settings::default_path();
    let mut assistant = Assistant::new(Observer::new(&*ocr, &students), &wiki, &students, &types, Settings::load(&settings_path), Some(settings_path.clone()), Overlay::start_hidden());
    assistant.set_library(read_library(out));
    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    ctrlc::set_handler(move || flag.store(true, Ordering::Relaxed))?;
    println!("add-on running (it only looks at the game, never sends input); settings: {}; Ctrl-C to stop", settings_path.display());

    let (mut region, mut prev, mut handled, mut still, mut loops, mut failures) = (None::<[f64; 4]>, None, None, 0u32, 0u32, 0u32);
    let mut said_waiting = false;
    while !stop.load(Ordering::Relaxed) {
        loops += 1;
        assistant.poll();
        for command in assistant.take_commands() {
            match command {
                Command::StartScan { deep } => {
                    let Some(r) = region else {
                        println!("scan: the game window is not found");
                        continue;
                    };
                    // the one thing here that sends input to the game: only because the player asked
                    assistant.begin_scan();
                    let req = ScanRequest { out, deep, workers, max_students: 300, from_first: true };
                    match scan::scan_students(ocr.clone(), hcu, r, &req, Options::default(), &|s| assistant.scan_progress(s)) {
                        Ok(s) => println!("scan finished: {} students, complete={}, reason={}", s.entries, s.complete, s.stop_reason),
                        Err(e) => eprintln!("scan failed: {e:#}"),
                    }
                    std::thread::sleep(Duration::from_secs(4)); // let the final banner be read
                    assistant.scan_finished(read_library(out));
                    (prev, handled, still) = (None, None, 0);
                }
            }
        }
        // find the game (again) when it is not known, and follow it if it moves or is resized
        if region.is_none() || (region_spec == "auto" && loops % REGION_EVERY == 0) {
            match find_region(hcu, region_spec) {
                Ok(r) => {
                    if region.is_none() {
                        println!("game window found: --region {:.2},{:.2},{:.2},{:.2}", r[0], r[1], r[2], r[3]);
                        said_waiting = false;
                    }
                    region = Some(r);
                }
                Err(e) => {
                    if region.take().is_some() || !said_waiting {
                        println!("waiting for the game: {e:#}");
                        said_waiting = true;
                    }
                    // nothing known about a window that is gone: forget its views, show no icons
                    (prev, handled, still) = (None, None, 0);
                    assistant.forget();
                    std::thread::sleep(WAIT_PAUSE);
                    continue;
                }
            }
        }
        let Some(r) = region else { continue };
        let img = match game_capture(hcu, r) {
            Ok(i) => {
                failures = 0;
                i
            }
            Err(e) => {
                failures += 1;
                eprintln!("capture failed: {e:#}");
                if failures >= MAX_CAPTURE_FAILURES {
                    region = None; // the window may be gone: look for it again
                    failures = 0;
                }
                std::thread::sleep(WAIT_PAUSE);
                continue;
            }
        };
        let key = frame_key(&img);
        let moving = prev.as_ref().map_or(true, |p| frame_distance(p, &key) >= STILL);
        still = if moving { 0 } else { still + 1 };
        prev = Some(key);
        if moving {
            // icons placed for the old view would sit on the wrong things until the new one is read
            assistant.transition();
        }
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
            println!("screen: {scene:?}");
            handled = Some(key);
        }
        std::thread::sleep(LOOP_PAUSE);
    }
    Ok(())
}
