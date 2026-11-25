//! The add-on: a passive guide that lives beside the game window. It only looks at the game
//! (screenshots, no input) and draws cards and icons from what it sees. This file is only the
//! loop: wait for the game, capture a frame, read its text, hand it to the assistant, and let
//! the assistant handle clicks on its buttons. Everything else lives in `baocr-assist`.
//!
//! It can be started before the game, after it, or while it restarts: the window is looked
//! for again whenever it is not known, and forgotten when it stops answering.
use anyhow::{bail, Result};
use crate::account_key;
use crate::clipboard;
use crate::dump::DumpSurface;
use crate::profile::Profile;
use crate::progress_scan;
use crate::scan::{self, ScanRequest};
use crate::upload;
use baocr_assist::observe::BattleGate;
use baocr_assist::{read_library, Assistant, Command, Observation, Observer, Settings, Surface};
use baocr_core::ocr_api::Ocr;
use baocr_extract::scene::Scene;
use baocr_extract::Options;
use baocr_hcu::{check_aspect, detect_region, game_capture, parse_frame_region, screen_kind, Hcu, HcuSession, Screen};
use baocr_overlay::{set_escape_target, BannerState, Overlay};
use baocr_pixels::{fast_frame_key, frame_distance, looks_like_battle};
use baocr_wikidb::Wiki;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::cell::RefCell;
use std::time::{Duration, Instant, SystemTime};

/// Frames this alike are the same view; two such frames in a row mean the screen has settled.
const STILL: f64 = 1.5;
/// Re-detect the game window this often (loops), so moving or resizing it is followed.
const REGION_EVERY: u32 = 25;
/// Pause between loops while a view is changing or not read yet: short, to read it soon (a capture
/// takes 85 to 125 ms already, so 20 ms more costs little and a second matching frame comes sooner).
const SETTLING_PAUSE: Duration = Duration::from_millis(20);
/// Pause once the view is read and still the same: nothing to do but notice a change.
const IDLE_PAUSE: Duration = Duration::from_millis(300);
/// Loops the view must have stayed the same before the slow pause applies.
const IDLE_AFTER: u32 = 3;
/// Pause in a battle: the add-on is quiet and only watches for the battle to end.
const BATTLE_PAUSE: Duration = Duration::from_millis(400);
/// Clicks on the card are answered this often however long the pause is.
const CLICK_SLICE: Duration = Duration::from_millis(30);
/// Pause while there is no game to look at.
const WAIT_PAUSE: Duration = Duration::from_millis(1000);
/// Capture failures in a row before the window is given up and looked for again.
const MAX_CAPTURE_FAILURES: u32 = 3;

/// How long to wait before the next look at the game.
/// `icons_up`: icons are drawn over the game, so a change must take them down at once and the loop
/// keeps looking at the short interval even when the view is read and still.
fn pause_for(in_battle: bool, moving: bool, read: bool, still: u32, icons_up: bool) -> Duration {
    if in_battle {
        BATTLE_PAUSE
    } else if !moving && read && still >= IDLE_AFTER && !icons_up {
        IDLE_PAUSE
    } else {
        SETTLING_PAUSE
    }
}

/// Wait `total`, answering clicks on the card meanwhile; a request for the main loop ends the wait.
fn wait<S: Surface>(assistant: &mut Assistant<'_, S>, total: Duration) {
    let end = Instant::now() + total;
    loop {
        let left = end.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return;
        }
        std::thread::sleep(left.min(CLICK_SLICE));
        assistant.poll();
        if assistant.has_commands() {
            return;
        }
    }
}

/// The game's client area in HCU frame pixels, if the game is there and shaped as expected.
fn find_region(hcu: &Hcu, spec: &str) -> Result<[f64; 4]> {
    let region = if spec == "auto" { detect_region(hcu)? } else { parse_frame_region(spec)? };
    check_aspect(region)?;
    Ok(region)
}

pub fn run(ocr: Arc<dyn Ocr>, session: &HcuSession, region_spec: &str, wiki_path: &Path, out: &Path, workers: usize) -> Result<()> {
    let overlay = Overlay::start_hidden();
    // BAOCR_STATE_DUMP=<file>: write what the surface shows as JSON, for the live end-to-end harness
    match std::env::var_os("BAOCR_STATE_DUMP").filter(|p| !p.is_empty()) {
        Some(path) => run_on(DumpSurface::new(overlay, path.into()), ocr, session, region_spec, wiki_path, out, workers),
        None => run_on(overlay, ocr, session, region_spec, wiki_path, out, workers),
    }
}

fn run_on<S: Surface>(surface: S, ocr: Arc<dyn Ocr>, session: &HcuSession, region_spec: &str, wiki_path: &Path, out: &Path, workers: usize) -> Result<()> {
    if !wiki_path.exists() {
        bail!("wiki database not found at {}: build it with `cargo run -p baocr-wikidb --release`", wiki_path.display());
    }
    let wiki = Wiki::open(wiki_path)?;
    let (students, types) = (wiki.students()?, wiki.type_table()?);
    let settings_path = Settings::default_path();
    let mut assistant = Assistant::new(Observer::new(&*ocr, &students), &wiki, &students, &types, Settings::load(&settings_path), Some(settings_path.clone()), surface);
    assistant.set_library(read_library(out));
    assistant.set_account_saved(upload::saved_account_id(&crate::upload_store()).is_some());
    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    ctrlc::set_handler(move || flag.store(true, Ordering::Relaxed))?;
    // ESC during a scan (or the tray's 종료) cancels every HCU call, which stops the scan at once
    let hcu = session.view();
    let cancel = hcu.cancel_token();
    set_escape_target(cancel.clone());
    println!("add-on running (it only looks at the game, never sends input); settings: {}; Ctrl-C to stop", settings_path.display());

    let (mut region, mut prev, mut handled, mut still, mut loops, mut failures) = (None::<[f64; 4]>, None, None, 0u32, 0u32, 0u32);
    let mut icons_up = false;
    let mut said_waiting = false;
    let mut prof = Profile::from_env();
    let mut gate = BattleGate::default();
    while !stop.load(Ordering::Relaxed) {
        loops += 1;
        prof.report();
        prof.time("poll", || {
            assistant.poll();
            assistant.tick(SystemTime::now());
        });
        for command in assistant.take_commands() {
            match command {
                Command::Quit => {
                    println!("add-on closed");
                    return Ok(());
                }
                Command::Upload => {
                    assistant.begin_upload();
                    let store = crate::upload_store();
                    let code = assistant.member_code().map(str::to_string);
                    let outcome = (|| -> Result<upload::Uploaded> {
                        // a code pasted on the card is kept as its hash, like one typed in a terminal
                        if let Some(code) = &code {
                            let code = account_key::parse_member_code(code)?;
                            upload::save_account_id(&store, &account_key::account_id(&code))?;
                        }
                        let req = upload::Request { server: upload::DEFAULT_SERVER.to_string(), nickname: None, students_dir: out.to_path_buf(), progress_dir: crate::progress_dir(), store: store.clone() };
                        upload::send(&req, &upload::Sources::from_env(), &mut |_| bail!("회원코드를 붙여넣어 주세요"))
                    })();
                    let saved = upload::saved_account_id(&store).is_some();
                    match outcome {
                        Ok(u) => {
                            let text = format!("업로드 완료 · 공개 ({} 명, 점수 {})\n{}", u.students, u.score, u.url);
                            println!("upload: {text}");
                            assistant.upload_finished(&text, Some(u.url));
                        }
                        Err(e) => {
                            println!("upload failed: {e:#}");
                            assistant.upload_finished(&format!("업로드 실패: {e:#}"), None);
                        }
                    }
                    if saved {
                        assistant.set_member_code("");
                    }
                    assistant.set_account_saved(saved);
                }
                Command::PasteMemberCode => match clipboard::get_text() {
                    Some(text) => assistant.set_member_code(&text),
                    None => assistant.say("클립보드에 글자가 없습니다"),
                },
                Command::OpenUrl(url) => {
                    if !(clipboard::is_openable(&url, upload::DEFAULT_SERVER) && clipboard::open_url(&url)) {
                        assistant.say("페이지를 열지 못했습니다");
                    }
                }
                Command::CopyText(text) => assistant.say(if clipboard::set_text(&text) { "주소를 복사했습니다" } else { "복사하지 못했습니다" }),
                Command::StartProgressScan => {
                    let Some(r) = region else {
                        println!("progress scan: the game window is not found");
                        continue;
                    };
                    cancel.store(false, Ordering::SeqCst);
                    assistant.begin_scan();
                    let last = RefCell::new(BannerState::counting(0));
                    match progress_scan::scan_progress(ocr.clone(), &session.exclusive(), r, &crate::progress_dir(), &|s| {
                        *last.borrow_mut() = s.clone();
                        assistant.scan_progress(s)
                    }) {
                        Ok(s) => println!("progress scan finished: {} sections, {}", s.sections_read, s.file.display()),
                        Err(e) => eprintln!("progress scan failed: {e:#}"),
                    }
                    assistant.release_input();
                    if cancel.swap(false, Ordering::SeqCst) {
                        let l = last.borrow().clone();
                        assistant.scan_progress(BannerState::failed(l.done, l.total, "ESC로 중단했습니다. 입력 제어권이 돌아왔습니다"));
                        println!("progress scan stopped by the player; input control returned");
                    }
                    std::thread::sleep(Duration::from_secs(4));
                    assistant.scan_finished(read_library(out));
                    (prev, handled, still) = (None, None, 0);
                }
                Command::StartScan { deep } => {
                    let Some(r) = region else {
                        println!("scan: the game window is not found");
                        continue;
                    };
                    // the one thing here that sends input to the game: only because the player asked
                    cancel.store(false, Ordering::SeqCst);
                    assistant.begin_scan();
                    let last = RefCell::new(BannerState::counting(0));
                    let req = ScanRequest { out, deep, workers, max_students: 300, from_first: true };
                    match scan::scan_students(ocr.clone(), &session.exclusive(), r, &req, Options::default(), &|s| {
                        *last.borrow_mut() = s.clone();
                        assistant.scan_progress(s)
                    }) {
                        Ok(s) => println!("scan finished: {} students, complete={}, reason={}", s.entries, s.complete, s.stop_reason),
                        Err(e) => eprintln!("scan failed: {e:#}"),
                    }
                    // the player's input comes back before anything else is said
                    assistant.release_input();
                    if cancel.swap(false, Ordering::SeqCst) {
                        let l = last.borrow().clone();
                        assistant.scan_progress(BannerState::failed(l.done, l.total, "ESC로 중단했습니다. 입력 제어권이 돌아왔습니다"));
                        println!("scan stopped by the player; input control returned");
                    }
                    std::thread::sleep(Duration::from_secs(4)); // let the final state be read
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
                    assistant.set_game_present(true);
                }
                Err(e) => {
                    if region.take().is_some() || !said_waiting {
                        println!("waiting for the game: {e:#}");
                        said_waiting = true;
                    }
                    // nothing known about a window that is gone: forget its views, show no icons
                    (prev, handled, still) = (None, None, 0);
                    gate.reset();
                    assistant.set_game_present(false);
                    assistant.forget();
                    std::thread::sleep(WAIT_PAUSE);
                    continue;
                }
            }
        }
        let Some(r) = region else { continue };
        let img = match prof.time("capture", || game_capture(hcu, r)) {
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
        // the combat screen is told by a few pixels, before anything else is done with the frame
        let battle_flip = prof.time("battle", || gate.feed(looks_like_battle(&img)));
        if let Some(entered) = battle_flip {
            (prev, handled, still) = (None, None, 0);
            // out of a battle nothing of the screen before it is true: the next settled frame is read afresh
            assistant.show(Observation::new(if entered { Scene::Battle } else { Scene::Unknown }));
            println!("screen: {}", if entered { "battle (the add-on stays quiet)" } else { "battle over" });
        }
        if gate.in_battle() {
            wait(&mut assistant, BATTLE_PAUSE);
            continue;
        }
        let (key, moving) = prof.time("key", || {
            let key = fast_frame_key(&img);
            let moving = prev.as_ref().map_or(true, |p| frame_distance(p, &key) >= STILL);
            (key, moving)
        });
        still = if moving { 0 } else { still + 1 };
        prev = Some(key);
        // With BAOCR_OVERLAY_CAPTURABLE (screenshots of the add-on) the icons are in the frames this reads, so showing them
        // looks like the screen moving and would take them down again; that mode keeps them up.
        if moving && std::env::var_os("BAOCR_OVERLAY_CAPTURABLE").is_none() {
            // icons placed for the old view would sit on the wrong things until the new one is read
            prof.time("transition", || assistant.transition());
        }
        let new_view = handled.as_ref().map_or(true, |h| frame_distance(h, &key) > STILL);
        if still >= 1 && new_view {
            let boxes = prof.time("ocr", || ocr.boxes(&img).unwrap_or_default());
            // the student list and profile are told apart by pixels, only when the text names no screen
            let fallback = prof.time("kind", || match screen_kind(&img) {
                Screen::Profile => Scene::StudentProfile,
                Screen::Roster => Scene::StudentList,
                Screen::Other => Scene::Unknown,
            });
            let scene = prof.time("observe+present", || assistant.on_frame(&img, &boxes, fallback));
            println!("screen: {scene:?}");
            icons_up = scene == Scene::QuickFormation;
            handled = Some(key);
        }
        let read = handled.as_ref().is_some_and(|h| frame_distance(h, &key) <= STILL);
        wait(&mut assistant, pause_for(false, moving, read, still, icons_up));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pause_follows_what_there_is_to_do() {
        assert_eq!(pause_for(true, true, false, 0, false), BATTLE_PAUSE, "a battle is watched slowly whatever moves");
        assert_eq!(pause_for(false, true, false, 0, false), SETTLING_PAUSE, "a changing view is looked at soon again");
        assert_eq!(pause_for(false, false, false, 5, false), SETTLING_PAUSE, "a view not read yet is not left waiting");
        assert_eq!(pause_for(false, false, true, 1, false), SETTLING_PAUSE, "just settled: look again soon");
        assert_eq!(pause_for(false, false, true, IDLE_AFTER, false), IDLE_PAUSE);
        assert_eq!(pause_for(false, false, true, IDLE_AFTER + 9, true), SETTLING_PAUSE, "icons over the game are watched at the short interval");
        assert_eq!(pause_for(false, true, true, 9, false), SETTLING_PAUSE, "any movement ends the slow pause");
    }
}
