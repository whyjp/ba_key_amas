//! The folded card: a small navy bookmark tab with an arrow that points the way the card opens
//! and a dot while a scan runs, the hint shown while the card is being dragged, and the choice of
//! what a press on the card's layer means. Pure pixels and plain data, checked without a window.
use crate::panel::{Hit, DRAG_HANDLE, HINT_AREA};
use crate::placement::Unfold;
use crate::render::{fill_rrect, stroke_rrect, NAVY, NAVY_SOFT, WHITE};
use image::{Rgba, RgbaImage};

/// Design size of the tab at the card's design scale (a card of 300 px).
pub const TAB_W: f32 = 28.0;
pub const TAB_H: f32 = 72.0;

/// The tab's size in screen pixels for a card scaled by `s` (card width over its design width).
pub fn tab_size(s: f32) -> (i32, i32) {
    (((TAB_W * s).round() as i32).max(18), ((TAB_H * s).round() as i32).max(48))
}

fn stroke_line(img: &mut RgbaImage, a: (f32, f32), b: (f32, f32), t: f32, rgb: [u8; 3]) {
    let n = ((b.0 - a.0).hypot(b.1 - a.1) * 2.0).ceil().max(1.0) as i32;
    for i in 0..=n {
        let f = i as f32 / n as f32;
        let (x, y) = (a.0 + (b.0 - a.0) * f, a.1 + (b.1 - a.1) * f);
        fill_rrect(img, x - t / 2.0, y - t / 2.0, t, t, t / 2.0, |_, _| (rgb, 1.0));
    }
}

/// The tab: a solid navy rounded bar with a chevron pointing the way the card opens, and a
/// status dot at its top while `busy` (a scan runs). `s` is the card's scale.
pub fn render_tab(size: (i32, i32), unfold: Unfold, busy: bool, s: f32) -> RgbaImage {
    let (w, h) = (size.0 as f32, size.1 as f32);
    let mut img = RgbaImage::from_pixel(size.0 as u32, size.1 as u32, Rgba([0, 0, 0, 0]));
    let b = (1.0 * s).max(1.0);
    fill_rrect(&mut img, 0.0, 0.0, w, h, 6.0 * s, |x, y| {
        let edge = x < b || y < b || x > w - b - 0.5 || y > h - b - 0.5;
        (if edge { NAVY_SOFT } else { NAVY }, 1.0)
    });
    // chevron
    let (cx, cy, arm, t) = (w / 2.0, h * 0.56, 5.5 * s, (2.0 * s).max(1.4));
    let dir = if unfold == Unfold::Left { -1.0 } else { 1.0 };
    let tip = (cx + dir * arm * 0.5, cy);
    stroke_line(&mut img, (cx - dir * arm * 0.5, cy - arm), tip, t, WHITE);
    stroke_line(&mut img, (cx - dir * arm * 0.5, cy + arm), tip, t, WHITE);
    if busy {
        let r = 4.0 * s;
        fill_rrect(&mut img, cx - r - 1.2 * s, h * 0.2 - r - 1.2 * s, 2.0 * (r + 1.2 * s), 2.0 * (r + 1.2 * s), r + 1.2 * s, |_, _| (WHITE, 1.0));
        fill_rrect(&mut img, cx - r, h * 0.2 - r, 2.0 * r, 2.0 * r, r, |_, _| ([38, 160, 96], 1.0));
    }
    img
}

/// The mark of the place the card would snap to if released now: a faint tinted outline.
pub fn render_snap_hint(size: (i32, i32), s: f32) -> RgbaImage {
    let (w, h) = (size.0.max(2) as f32, size.1.max(2) as f32);
    let mut img = RgbaImage::from_pixel(w as u32, h as u32, Rgba([0, 0, 0, 0]));
    fill_rrect(&mut img, 0.0, 0.0, w, h, 6.0 * s, |_, _| (NAVY, 0.16));
    stroke_rrect(&mut img, 0.0, 0.0, w, h, 6.0 * s, (2.0 * s).max(1.5), NAVY_SOFT, 0.85);
    img
}

/// What a press on the card's layer means.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Press<'a> {
    /// Pick the card (or the tab) up.
    Drag,
    /// A button of the card; its id.
    Button(&'a str),
}

/// A press at (`x`, `y`) in the layer's own pixels. A folded tab is all handle. On the open card
/// the header's icons come before the handle they sit on, and tooltip rows are not buttons.
pub fn pick<'a>(folded: bool, hits: &'a [Hit], x: i32, y: i32) -> Option<Press<'a>> {
    if folded {
        return Some(Press::Drag);
    }
    match hits.iter().find(|h| h.contains(x, y) && h.id != HINT_AREA)? {
        h if h.id == DRAG_HANDLE => Some(Press::Drag),
        h => Some(Press::Button(&h.id)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panel::{render_panel_hits, PanelState, Row, PIN_BUTTON};
    use crate::render::load_fonts;

    fn preview(name: &str, img: &RgbaImage) {
        if let Some(dir) = std::env::var_os("BANNER_PREVIEW_DIR") {
            img.save(std::path::Path::new(&dir).join(name)).unwrap();
        }
    }

    #[test]
    fn the_tab_scales_with_the_card() {
        assert_eq!(tab_size(1.0), (28, 72));
        assert_eq!(tab_size(1.2), (34, 86));
        assert!(tab_size(0.5).0 >= 18, "never too small to hit");
    }

    #[test]
    fn the_tab_points_the_way_it_opens_and_shows_a_dot_only_while_busy() {
        let size = tab_size(1.0);
        let right = render_tab(size, Unfold::Right, false, 1.0);
        let left = render_tab(size, Unfold::Left, false, 1.0);
        assert_eq!((right.width(), right.height()), (28, 72));
        assert_eq!(right.get_pixel(0, 0)[3], 0, "rounded corner");
        assert!(right.get_pixel(14, 5)[3] > 250, "solid body");
        // the chevron's tip (its middle row) is on the side it opens to, its ends are not
        let tip_vs_end = |img: &RgbaImage| -> (f32, f32) {
            let rows: Vec<u32> = (0..img.height()).filter(|y| (3..25).any(|x| img.get_pixel(x, *y).0[..3] == [255, 255, 255])).collect();
            let mean_x = |y: u32| {
                let xs: Vec<u32> = (3..25).filter(|x| img.get_pixel(*x, y).0[..3] == [255, 255, 255]).collect();
                xs.iter().sum::<u32>() as f32 / xs.len() as f32
            };
            (mean_x((rows[0] + rows[rows.len() - 1]) / 2), mean_x(rows[0]))
        };
        let (rt, re) = tip_vs_end(&right);
        let (lt, le) = tip_vs_end(&left);
        assert!(rt > re && lt < le, "right {rt} {re}, left {lt} {le}");
        let busy = render_tab(size, Unfold::Right, true, 1.0);
        let green = |img: &RgbaImage| img.pixels().any(|p| p.0[1] > 140 && p.0[0] < 60 && p.0[3] > 250);
        assert!(green(&busy) && !green(&right));
        preview("tab-right.png", &right);
        preview("tab-left.png", &left);
        preview("tab-busy.png", &busy);
        preview("snap-hint.png", &render_snap_hint((300, 200), 1.0));
    }

    #[test]
    fn a_press_on_a_folded_tab_always_picks_it_up() {
        assert_eq!(pick(true, &[], 3, 3), Some(Press::Drag));
    }

    #[test]
    fn the_headers_icons_win_over_the_handle_they_sit_on_and_rows_are_buttons() {
        let Some(f) = load_fonts() else { return };
        let state = PanelState {
            scene: "상태".into(),
            rows: vec![Row::Hint { label: "갱신".into(), value: "3일".into(), tip: "t".into() }, Row::Button { id: "scan".into(), label: "스캔".into(), value: "".into(), on: false }],
            close: Some(("quit".into(), "애드온 종료".into())),
            pin: Some(true),
            ..Default::default()
        };
        let (img, hits) = render_panel_hits(300, &state, &f);
        let at = |id: &str| hits.iter().position(|h| h.id == id).unwrap_or_else(|| panic!("no hit {id}"));
        assert!(at("quit") < at(DRAG_HANDLE) && at(PIN_BUTTON) < at(DRAG_HANDLE), "icons before the handle");
        let (quit, pin, scan) = (&hits[at("quit")], &hits[at(PIN_BUTTON)], &hits[at("scan")]);
        assert!(pin.x + pin.w <= quit.x + 0.5, "the pin is left of the power icon without overlapping it: {pin:?} {quit:?}");
        assert_eq!(pin.tip.as_deref(), Some("접기"), "pinned: pressing it folds");
        let mid = |h: &Hit| ((h.x + h.w / 2.0) as i32, (h.y + h.h / 2.0) as i32);
        let (qx, qy) = mid(quit);
        assert_eq!(pick(false, &hits, qx, qy), Some(Press::Button("quit")));
        let (px, py) = mid(pin);
        assert_eq!(pick(false, &hits, px, py), Some(Press::Button(PIN_BUTTON)));
        assert_eq!(pick(false, &hits, 20, 5), Some(Press::Drag), "the rest of the header is the handle");
        let (sx, sy) = mid(scan);
        assert_eq!(pick(false, &hits, sx, sy), Some(Press::Button("scan")));
        assert_eq!(pick(false, &hits, 30, (hits[at(HINT_AREA)].y + 3.0) as i32), None, "a tooltip row is not a button");
        let folded = render_panel_hits(300, &PanelState { pin: Some(false), ..state.clone() }, &f);
        assert_eq!(folded.1[folded.1.iter().position(|h| h.id == PIN_BUTTON).unwrap()].tip.as_deref(), Some("고정"));
        // no pin button when the overlay does not offer one
        assert!(render_panel_hits(300, &PanelState { pin: None, ..state }, &f).1.iter().all(|h| h.id != PIN_BUTTON));
        preview("pin-pinned.png", &img);
        preview("pin-folds.png", &folded.0);
    }

    /// The card and the tab composited over real screens, at two window widths and each corner.
    #[test]
    fn the_card_and_the_tab_sit_inside_a_real_screen_at_any_window_width() {
        use crate::dock::{Anchor, Corner};
        use crate::panel::{inside_card_width, PANEL_BASE_W};
        use crate::placement::{resolve, strip, Frame};
        let Some(f) = load_fonts() else { return };
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../fixtures/screens");
        let state = PanelState {
            scene: "편성".into(),
            title: "19-3 설원 지구 제2 지역".into(),
            rows: vec![
                Row::Info { label: "지형".into(), value: "야외".into() },
                Row::Info { label: "추천 Lv".into(), value: "85".into() },
                Row::Note("Hard 난이도는 적 구성이 다를 수 있습니다".into()),
                Row::Button { id: "scan".into(), label: "학생 스캔".into(), value: "시작".into(), on: false },
            ],
            close: Some(("quit".into(), "애드온 종료".into())),
            pin: Some(false),
            ..Default::default()
        };
        for (name, w) in [("quick-formation", 1280u32), ("work-hub-full", 1280), ("quick-formation", 800)] {
            let Ok(shot) = image::open(dir.join(format!("{name}.png"))) else { continue };
            let h = w * 3 / 4;
            let bg = image::imageops::resize(&shot.to_rgba8(), w, h, image::imageops::FilterType::Triangle);
            let frame = Frame { client: (0, 0, w as i32, h as i32), outer: (0, 0, w as i32, h as i32), work: (0, 0, w as i32, h as i32) };
            let width = inside_card_width(w as i32) as u32;
            let s = width as f32 / PANEL_BASE_W;
            let (card, _) = render_panel_hits(width, &state, &f);
            for (corner, tag) in [(Corner::TopRight, "tr"), (Corner::TopLeft, "tl")] {
                let r = resolve(frame, Anchor::Inside(corner), (card.width() as i32, card.height() as i32), tab_size(s));
                assert!(r.open.0 >= 0 && r.open.2 <= w as i32 && r.open.3 <= h as i32, "{name} {w} {tag}: {:?}", r.open);
                let unfolded = |p: f32| {
                    let mut canvas = bg.clone();
                    let (rc, src) = strip(&r, p);
                    let part = image::imageops::crop_imm(&card, src as u32, 0, (rc.2 - rc.0) as u32, (rc.3 - rc.1) as u32).to_image();
                    let shown = if p <= 0.0 { render_tab(tab_size(s), r.unfold, false, s) } else { part };
                    image::imageops::overlay(&mut canvas, &shown, rc.0 as i64, rc.1 as i64);
                    canvas
                };
                if tag == "tr" || w == 1280 {
                    preview(&format!("inside-{tag}-{name}-{w}-folded.png"), &unfolded(0.0));
                    preview(&format!("inside-{tag}-{name}-{w}-sliding.png"), &unfolded(0.5));
                    preview(&format!("inside-{tag}-{name}-{w}-open.png"), &unfolded(1.0));
                }
            }
        }
    }
}
