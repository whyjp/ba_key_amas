use crate::capture::{game_capture, layout_looks_calibrated, require_ratio43, to_frame};
use crate::deep::capture_deep;
use crate::hcu::{Exclusive, Hcu};
use anyhow::{anyhow, bail, Result};
use baocr_core::ocr_api::ScanSink;
use baocr_core::frames::{FRAME_DETAIL, FRAME_LEVEL_UP, FRAME_MYSTIC, FRAME_POPUP_BOTTOM, FRAME_POPUP_TOP};
use baocr_pixels::{detail_distance, detail_key, frame_distance, frame_key, roster_visible_pixels, DetailKey};
use image::RgbImage;
use std::collections::BTreeMap;
use std::path::Path;

/// How a scan ended. The caller finishes the report (drain workers, rebuild,
/// `complete = wrapped && from_first && no unresolved`, save).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ScanOutcome {
    /// running | focus_lost | max_students | repeated_profile | wrapped_to_first | next_not_confirmed |
    /// left_detail_screen | layout_mismatch | deep_failed | arrow_click_failed | <sink abort reason> | <capture error>
    pub stop_reason: String,
    pub wrapped: bool,
    pub error: Option<String>,
}

fn outcome(reason: &str, wrapped: bool, error: Option<String>) -> Result<ScanOutcome> {
    Ok(ScanOutcome { stop_reason: reason.to_string(), wrapped, error })
}

/// Waits for the profile to change AND hold still for two consecutive captures,
/// so extraction never reads a mid-transition frame. `Ok(None)` = not advanced.
/// `Err("left_detail_screen")` when the roster is on screen.
pub fn wait_stable_next(h: &Hcu, region: [f64; 4], prev: &DetailKey) -> Result<Option<RgbImage>> {
    let mut last: Option<(RgbImage, [u8; 3072])> = None;
    let mut changed = false;
    for k in 0..12 {
        if k > 0 {
            h.nap(100);
        }
        let cur = game_capture(h, region)?;
        if roster_visible_pixels(&cur) {
            return Err(anyhow!("left_detail_screen"));
        }
        let k = frame_key(&cur);
        if detail_distance(prev, &detail_key(&cur)) >= 2.0 {
            changed = true;
        }
        if changed {
            if let Some((_, lk)) = &last {
                if frame_distance(lk, &k) < 1.5 {
                    return Ok(Some(cur));
                }
            }
        }
        last = Some((cur, k));
    }
    Ok(None)
}

/// The first profile as it looks once it holds still. It is captured right after the profile opened,
/// often mid-slide, and it is the reference the wrap back to the first student is recognised by: a frame
/// caught a few pixels off would never match the settled one, and the scan would run into a repeat.
fn settled(h: &Hcu, region: [f64; 4], first: RgbImage) -> Result<RgbImage> {
    let mut last = first;
    for _ in 0..12 {
        h.nap(100);
        let cur = game_capture(h, region)?;
        let still = frame_distance(&frame_key(&last), &frame_key(&cur)) < 1.5;
        last = cur;
        if still {
            break;
        }
    }
    Ok(last)
}

/// Walks sequential profiles with the right arrow, handing each captured
/// student (detail frame plus, with `deep`, the menu frames) to `sink`.
///
/// `Err` only for failures before the loop (activate, first capture, not on a
/// profile). Every later stop is an `Ok(ScanOutcome)` with its reason.
/// `out` is kept for API parity with Go; the sink owns persistence.
pub fn scan_arrows(
    h: &Exclusive,
    region: [f64; 4],
    out: &Path,
    max_students: usize,
    from_first: bool,
    deep: bool,
    sink: &dyn ScanSink,
) -> Result<ScanOutcome> {
    let _ = (out, from_first);
    require_ratio43(region)?;
    h.activate()?;
    let mut img = game_capture(h, region)?;
    if roster_visible_pixels(&img) {
        bail!("open the first student profile before --scan-arrows");
    }
    img = settled(h, region, img)?;
    let first = detail_key(&img);
    let mut seen: Vec<DetailKey> = Vec::new();
    for i in 0..max_students {
        if let Err(e) = h.foreground() {
            return outcome("focus_lost", false, Some(e.to_string()));
        }
        if let Some((reason, msg)) = sink.aborted() {
            return outcome(&reason, false, Some(msg));
        }
        let key = detail_key(&img);
        // A matching name-strip fingerprint is only a repeat when the names agree too.
        for (j, prior) in seen.iter().enumerate() {
            if detail_distance(&key, prior) < 2.0 {
                let n = sink.peek_name(&img);
                if n.is_empty() || n == sink.name_of(j) {
                    return outcome("repeated_profile", false, Some("profile repeated before advancing".into()));
                }
            }
        }
        seen.push(key);
        let mut frames: BTreeMap<String, RgbImage> = BTreeMap::new();
        frames.insert(FRAME_DETAIL.to_string(), img.clone());
        if deep {
            match capture_deep(h, region, &img) {
                Ok(d) => {
                    frames.insert(FRAME_POPUP_TOP.to_string(), d.popup_top);
                    frames.insert(FRAME_POPUP_BOTTOM.to_string(), d.popup_bottom);
                    frames.insert(FRAME_LEVEL_UP.to_string(), d.level_up);
                    frames.insert(FRAME_MYSTIC.to_string(), d.mystic);
                }
                Err(e) => return outcome("deep_failed", false, Some(format!("student {} menu capture: {e:#}", i + 1))),
            }
        }
        sink.submit(i, frames);
        println!("  captured {:03}", i + 1);
        if i == 0 {
            // The first profile proves the layout before the scan goes on.
            let calibrated = sink.first_student().map(|s| layout_looks_calibrated(&s)).unwrap_or(false);
            if !calibrated {
                return outcome(
                    "layout_mismatch",
                    false,
                    Some("first profile is unreadable: the screen is not in the calibrated 4:3 layout (set the game to 4:3, windowed)".into()),
                );
            }
        }
        if i + 1 == max_students {
            return outcome("max_students", false, None);
        }
        let (x, y) = to_frame(region, 1250, 505);
        if let Err(e) = h.click(x, y) {
            return outcome("arrow_click_failed", false, Some(e.to_string()));
        }
        let next = match wait_stable_next(h, region, &key) {
            Ok(n) => n,
            Err(e) => return outcome(&e.to_string(), false, Some(e.to_string())),
        };
        let Some(next) = next else {
            return outcome("next_not_confirmed", false, None);
        };
        // Back on the first profile: the name must match student 0 as well, so a
        // look-alike strip can never mark the roster complete.
        if detail_distance(&first, &detail_key(&next)) < 2.0 {
            let n = sink.peek_name(&next);
            if !n.is_empty() && n == sink.name_of(0) {
                return outcome("wrapped_to_first", true, None);
            }
        }
        img = next;
    }
    outcome("max_students", false, None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake::{FakeSink, World};

    const R: [f64; 4] = [0.0, 0.0, 1280.0, 960.0];

    /// Arrow click advances through `shades` (grey level of each position).
    fn arrow_world(shades: Vec<u8>) -> World {
        let n = shades.len();
        World::new(
            move |p| shades[p % n],
            |p, x, y| {
                if x as i32 == 1250 && y as i32 == 505 {
                    *p += 1;
                }
            },
            |_, _| {},
        )
    }

    fn run(w: &World, max: usize, deep: bool, sink: &FakeSink) -> ScanOutcome {
        scan_arrows(&w.hcu().exclusive(), R, Path::new("out"), max, true, deep, sink).unwrap()
    }

    /// A look-alike name strip with a different (or unreadable) name is not a wrap.
    #[test]
    fn wrap_needs_a_matching_name() {
        let w = arrow_world(vec![20, 70, 120, 20, 70]);
        let sink = FakeSink::new(); // peek_name -> "" (unreadable)
        let o = run(&w, 4, false, &sink);
        assert_ne!(o.stop_reason, "wrapped_to_first");
        assert!(!o.wrapped);
    }

    #[test]
    fn wrap_to_first_is_detected() {
        let w = arrow_world(vec![20, 70, 120, 20]);
        let sink = FakeSink::new();
        *sink.peek.lock().unwrap() = Some("student0".into()); // the name on the wrapped frame matches student 0
        let o = run(&w, 10, false, &sink);
        assert_eq!((o.stop_reason.as_str(), o.wrapped, o.error), ("wrapped_to_first", true, None));
        assert_eq!(sink.keys(), vec![(0, vec!["detail".to_string()]), (1, vec!["detail".to_string()]), (2, vec!["detail".to_string()])]);
        // activate once, three arrow clicks, never anything else
        assert_eq!(w.inputs(), vec!["activate w13", "click 1250.00 505.00", "click 1250.00 505.00", "click 1250.00 505.00"]);
    }

    #[test]
    fn repeat_is_detected() {
        let w = arrow_world(vec![20, 70, 120, 70]);
        let sink = FakeSink::new();
        let o = run(&w, 10, false, &sink);
        assert_eq!(o.stop_reason, "repeated_profile");
        assert!(!o.wrapped);
        assert!(o.error.is_some());
        assert_eq!(sink.submitted.lock().unwrap().len(), 3);
    }

    #[test]
    fn stuck_arrow_is_next_not_confirmed() {
        let w = World::new(|_| 40, |_, _, _| {}, |_, _| {});
        let sink = FakeSink::new();
        let o = run(&w, 10, false, &sink);
        assert_eq!((o.stop_reason.as_str(), o.error), ("next_not_confirmed", None));
        assert_eq!(sink.submitted.lock().unwrap().len(), 1);
    }

    #[test]
    fn max_students_stops_without_extra_click() {
        let w = arrow_world(vec![20, 70, 120, 170]);
        let sink = FakeSink::new();
        let o = run(&w, 2, false, &sink);
        assert_eq!((o.stop_reason.as_str(), o.wrapped, o.error), ("max_students", false, None));
        assert_eq!(sink.submitted.lock().unwrap().len(), 2);
        assert_eq!(w.inputs().iter().filter(|s| s.starts_with("click")).count(), 1);
    }

    #[test]
    fn focus_lost_blocks_all_input() {
        // activate=1 apps call, iteration 0 foreground=2 (active), iteration 1 foreground=3 -> lost
        let w = arrow_world(vec![20, 70, 120, 170]).active_for(2);
        let sink = FakeSink::new();
        let o = run(&w, 10, false, &sink);
        assert_eq!(o.stop_reason, "focus_lost");
        assert!(o.error.unwrap().contains("not the active window"));
        assert_eq!(sink.submitted.lock().unwrap().len(), 1);
        // one arrow click happened while focused; nothing after focus was lost
        assert_eq!(w.inputs(), vec!["activate w13", "click 1250.00 505.00"]);
    }

    #[test]
    fn focus_lost_at_start_sends_no_click() {
        let w = arrow_world(vec![20, 70]).active_for(1);
        let sink = FakeSink::new();
        let o = run(&w, 10, false, &sink);
        assert_eq!(o.stop_reason, "focus_lost");
        assert_eq!(w.inputs(), vec!["activate w13"]);
        assert!(sink.submitted.lock().unwrap().is_empty());
    }

    #[test]
    fn sink_abort_stops_scan() {
        let w = arrow_world(vec![20, 70, 120, 170]);
        let sink = FakeSink::new();
        *sink.abort.lock().unwrap() = Some(("two_empty_names".into(), "two unreadable names".into()));
        *sink.abort_after.lock().unwrap() = Some(2);
        let o = run(&w, 10, false, &sink);
        assert_eq!((o.stop_reason.as_str(), o.error.as_deref()), ("two_empty_names", Some("two unreadable names")));
        assert_eq!(sink.submitted.lock().unwrap().len(), 2);
    }

    #[test]
    fn the_scan_refuses_16_9_before_any_input() {
        let w = arrow_world(vec![20, 70, 120]);
        let sink = FakeSink::new();
        let r = scan_arrows(&w.hcu().exclusive(), [0.0, 0.0, 1280.0, 720.0], Path::new("out"), 3, true, false, &sink);
        assert_eq!(r.unwrap_err().to_string(), crate::capture::SCAN_NEEDS_43);
        assert!(w.inputs().is_empty(), "{:?}", w.inputs());
    }

    #[test]
    fn layout_mismatch_after_first_profile() {
        let w = arrow_world(vec![20, 70, 120, 170]);
        let mut sink = FakeSink::new();
        sink.calibrated = false;
        let o = run(&w, 10, false, &sink);
        assert_eq!(o.stop_reason, "layout_mismatch");
        assert_eq!(sink.submitted.lock().unwrap().len(), 1);
        assert_eq!(w.inputs(), vec!["activate w13"]); // no arrow click
    }

    #[test]
    fn deep_scan_submits_menu_frames() {
        // positions: p -> grey; menu screens live at 100+ and the basic tab returns to p
        // Simplified world: menu clicks cycle a menu counter, arrow moves p. screen = p + 10*menu
        let w = World::new(
            |s| (20 + 40 * (s % 10) + 40 * (s / 10)) as u8,
            |s, x, y| match (x as i32, y as i32) {
                (1250, 505) => *s += 1,
                (1138, 320) => *s += 10,
                (1057, 267) => *s += 10,
                (937, 258) => *s += 10,
                (1120, 262) => *s += 10,
                (755, 262) => *s %= 10,
                _ => {}
            },
            |s, d| {
                // scrolling down moves the popup to its end once, however many movements are sent
                if d > 0 && *s / 10 == 1 {
                    *s += 10
                }
            },
        );
        let sink = FakeSink::new();
        let o = run(&w, 1, true, &sink);
        assert_eq!(o.stop_reason, "max_students");
        let keys = sink.keys();
        assert_eq!(
            keys,
            vec![(0, ["detail", "popup-bottom", "popup-top", "tab-levelup", "tab-mystic"].iter().map(|s| s.to_string()).collect::<Vec<_>>())]
        );
    }

    #[test]
    fn deep_failure_stops_scan() {
        // popup click never changes the screen -> settle fails
        let w = World::new(|_| 40, |_, _, _| {}, |_, _| {});
        let sink = FakeSink::new();
        let o = run(&w, 3, true, &sink);
        assert_eq!(o.stop_reason, "deep_failed");
        assert!(o.error.unwrap().contains("student 1 menu capture: stat popup"));
        assert!(sink.submitted.lock().unwrap().is_empty());
    }
}
