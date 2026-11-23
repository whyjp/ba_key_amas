use crate::capture::{game_capture, to_frame};
use crate::hcu::{Act, Hcu};
use anyhow::{anyhow, bail, Context, Result};
use baocr_extract::DeepFrames;
use baocr_pixels::panel_delta;
use image::RgbImage;

/// Click at a 1280x960 client coordinate.
pub fn click_game(h: &Hcu, region: [f64; 4], x: i32, y: i32) -> Result<()> {
    let (fx, fy) = to_frame(region, x, y);
    h.click(fx, fy)
}

/// Waits for the panel to CHANGE from `from` (the click took effect) and then
/// hold still for two consecutive captures, so neither the old screen nor a
/// half-drawn animation is ever read.
pub fn settle(h: &Hcu, region: [f64; 4], from: &RgbImage) -> Result<RgbImage> {
    let mut last: Option<RgbImage> = None;
    let mut changed = false;
    for k in 0..16 {
        if k > 0 {
            h.nap(100);
        }
        let cur = game_capture(h, region)?;
        if panel_delta(from, &cur) > 0.004 {
            changed = true;
        }
        if changed && last.as_ref().is_some_and(|l| panel_delta(l, &cur) < 0.0008) {
            return Ok(cur);
        }
        last = Some(cur);
    }
    Err(anyhow!("screen did not change and settle after the click"))
}

/// Waits for two identical consecutive captures without requiring a change
/// (used after a scroll that may already be at its limit).
pub fn settle_quiet(h: &Hcu, region: [f64; 4]) -> Result<RgbImage> {
    let mut last: Option<RgbImage> = None;
    for k in 0..12 {
        if k > 0 {
            h.nap(100);
        }
        let cur = game_capture(h, region)?;
        if last.as_ref().is_some_and(|l| panel_delta(l, &cur) < 0.0008) {
            return Ok(cur);
        }
        last = Some(cur);
    }
    Err(anyhow!("screen did not settle"))
}

/// What the scan remembers about the 상세 popup between students.
///
/// The popup reopens at the scroll offset it was closed at. Closing it scrolled to
/// the top (in the same HCU call as the close click) means the next student's popup
/// already opens at the top, so the separate scroll-up call and its wait are not
/// needed. That is checked on every student: if the popup did not open at the top
/// (the scroll down changes nothing), the explicit scroll-up path runs instead and
/// the shortcut is switched off for the rest of the scan.
#[derive(Debug, Clone)]
pub struct DeepState {
    popup_opens_at_top: bool,
    shortcut: bool,
}

impl Default for DeepState {
    fn default() -> Self {
        // the very first popup has an unknown offset
        DeepState { popup_opens_at_top: false, shortcut: true }
    }
}

/// Scroll the popup to its top and read both ends. Returns (top, bottom).
fn read_popup_ends(h: &Hcu, region: [f64; 4], opened: RgbImage, state: &mut DeepState) -> Result<(RgbImage, RgbImage)> {
    let (fx, fy) = to_frame(region, 640, 560);
    if state.popup_opens_at_top {
        // already at the top: the frame just settled is the top; scroll down for the bottom
        h.scroll(fx, fy, 6)?;
        if let Ok(bottom) = settle(h, region, &opened) {
            return Ok((opened, bottom));
        }
        // the popup was not at the top after all: do it the explicit way, and stop trusting the shortcut
        state.shortcut = false;
    }
    // The popup remembers its last scroll offset: start from the top.
    h.scroll(fx, fy, -12)?;
    let top = settle_quiet(h, region).context("stat popup top")?;
    h.scroll(fx, fy, 6)?;
    let bottom = settle(h, region, &top).context("stat popup scroll")?;
    Ok((top, bottom))
}

/// Walks the read-only menus (상세 popup top/bottom, 레벨 업 tab, 신비 해방 tab)
/// and returns the frames it saw. No OCR. The UI is left on the basic-info tab;
/// if that cannot be confirmed it returns an error.
pub fn capture_deep(h: &Hcu, region: [f64; 4], before: &RgbImage, state: &mut DeepState) -> Result<DeepFrames> {
    click_game(h, region, 1138, 320)?; // 상세
    let opened = settle(h, region, before).context("stat popup")?;
    let (top, bottom) = read_popup_ends(h, region, opened, state)?;
    // Close. Scrolling back to the top in the same call lets the next student's popup open there.
    let (cx, cy) = to_frame(region, 1057, 267);
    if state.shortcut {
        let (fx, fy) = to_frame(region, 640, 560);
        h.actions(&[Act::Scroll { x: fx, y: fy, delta: -12 }, Act::Click { x: cx, y: cy }])?;
        state.popup_opens_at_top = true;
    } else {
        h.click(cx, cy)?; // close
        state.popup_opens_at_top = false;
    }
    let closed = settle(h, region, &bottom).context("stat popup close")?;
    click_game(h, region, 937, 258)?; // 레벨 업
    let level_up = settle(h, region, &closed).context("level-up tab")?;
    click_game(h, region, 1120, 262)?; // 신비 해방
    let mystic = settle(h, region, &level_up).context("mystic tab")?;
    click_game(h, region, 755, 262)?; // 기본 정보
    let back = settle(h, region, &mystic).context("basic tab")?;
    if panel_delta(before, &back) > 0.02 {
        bail!("did not return to the basic-info tab");
    }
    Ok(DeepFrames { popup_top: top, popup_bottom: bottom, level_up, mystic })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake::{flat, World};

    const R: [f64; 4] = [0.0, 0.0, 1280.0, 960.0];

    #[test]
    fn settle_requires_change_then_two_stable() {
        // screen 0 = from; world flips to screen 1 on the first click.
        let w = World::new(|s| 20 + 50 * s as u8, |s, _, _| *s = 1, |_, _| {});
        let h = Hcu::with_runner(Box::new(w.clone())).without_sleep();
        let from = game_capture(&h, R).unwrap();
        // no click yet: the screen never changes
        assert!(settle(&h, R, &from).is_err());
        click_game(&h, R, 10, 10).unwrap();
        let cur = settle(&h, R, &from).unwrap();
        assert_eq!(cur.get_pixel(5, 5).0, [70, 70, 70]);
    }

    #[test]
    fn settle_quiet_needs_no_change() {
        let w = World::new(|_| 30, |_, _, _| {}, |_, _| {});
        let h = Hcu::with_runner(Box::new(w)).without_sleep();
        assert!(settle_quiet(&h, R).is_ok());
    }

    #[test]
    fn settle_waits_out_animation() {
        // screen advances on every capture until 3, so 'stable' only happens at the end
        let w = World::new(|s| 20 + 50 * s as u8, |s, _, _| *s = 1, |_, _| {}).animate_to(3);
        let h = Hcu::with_runner(Box::new(w.clone())).without_sleep();
        let from = flat(20);
        click_game(&h, R, 10, 10).unwrap();
        let cur = settle(&h, R, &from).unwrap();
        assert_eq!(cur.get_pixel(5, 5).0, [170, 170, 170]);
    }

    #[test]
    fn capture_deep_click_order_and_scroll_up_first() {
        let on_click = |s: &mut usize, x: f64, y: f64| {
            *s = match (x as i32, y as i32) {
                (1138, 320) => 1,
                (1057, 267) => 3,
                (937, 258) => 4,
                (1120, 262) => 5,
                (755, 262) => 0,
                _ => *s,
            }
        };
        let on_scroll = |s: &mut usize, d: i32| {
            if d > 0 {
                *s = 2
            }
        };
        let w = World::new(|s| 20 + 30 * s as u8, on_click, on_scroll);
        let h = Hcu::with_runner(Box::new(w.clone())).without_sleep();
        let before = flat(20);
        let f = capture_deep(&h, R, &before, &mut DeepState::default()).unwrap();
        assert_eq!(f.popup_top.get_pixel(0, 0).0, [50, 50, 50]);
        assert_eq!(f.popup_bottom.get_pixel(0, 0).0, [80, 80, 80]);
        assert_eq!(f.level_up.get_pixel(0, 0).0, [140, 140, 140]);
        assert_eq!(f.mystic.get_pixel(0, 0).0, [170, 170, 170]);
        assert_eq!(
            w.inputs(),
            vec![
                "click 1138.00 320.00",
                "scroll 640.00 560.00 -12",
                "scroll 640.00 560.00 6",
                "scroll 640.00 560.00 -12",
                "click 1057.00 267.00",
                "click 937.00 258.00",
                "click 1120.00 262.00",
                "click 755.00 262.00",
            ]
        );
    }

    #[test]
    fn second_student_skips_the_scroll_up_call() {
        let on_click = |s: &mut usize, x: f64, y: f64| {
            *s = match (x as i32, y as i32) {
                (1138, 320) => 1,
                (1057, 267) => 3,
                (937, 258) => 4,
                (1120, 262) => 5,
                (755, 262) => 0,
                _ => *s,
            }
        };
        let w = World::new(|s| 20 + 30 * s as u8, on_click, |s, d| {
            if d > 0 {
                *s = 2
            }
        });
        let h = Hcu::with_runner(Box::new(w.clone())).without_sleep();
        let mut state = DeepState::default();
        capture_deep(&h, R, &flat(20), &mut state).unwrap();
        let first = w.inputs().len();
        capture_deep(&h, R, &flat(20), &mut state).unwrap();
        let second: Vec<String> = w.inputs()[first..].to_vec();
        assert_eq!(
            second,
            vec![
                "click 1138.00 320.00",
                "scroll 640.00 560.00 6", // down for the bottom: no scroll-up first, the popup reopens at the top
                "scroll 640.00 560.00 -12", // back to the top, in the same call as the close
                "click 1057.00 267.00",
                "click 937.00 258.00",
                "click 1120.00 262.00",
                "click 755.00 262.00",
            ]
        );
    }

    #[test]
    fn capture_deep_fails_if_not_back_on_basic_tab() {
        // basic-tab click lands on a different screen than `before`
        let on_click = |s: &mut usize, x: f64, _y: f64| *s = if x as i32 == 755 { 9 } else { *s + 1 };
        let w = World::new(|s| 10 + 25 * s as u8, on_click, |s, d| {
            if d > 0 {
                *s += 1
            }
        });
        let h = Hcu::with_runner(Box::new(w)).without_sleep();
        let err = capture_deep(&h, R, &flat(10), &mut DeepState::default()).err().unwrap();
        let err = format!("{err:#}");
        assert!(err.contains("did not return to the basic-info tab"), "{err}");
    }
}
