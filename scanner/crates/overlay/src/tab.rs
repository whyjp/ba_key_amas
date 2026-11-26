//! The folded card: a small bookmark tab (a navy cap with the add-on's mark, a pale body with the
//! scene's short name written downward, a chevron that points the way the card opens), the hint
//! shown while the card is being dragged, and the choice of what a press on the card's layer
//! means. Pure pixels and plain data, checked without a window.
use crate::panel::{Hit, DRAG_HANDLE, HINT_AREA};
use crate::placement::Unfold;
use crate::render::{draw_text, fill_rrect, stroke_rrect, text_width, Fonts, BLUE, INK, NAVY, NAVY_SOFT, WHITE};
use image::{Rgba, RgbaImage};

/// Design size of the tab at the card's design scale (a card of 300 px).
pub const TAB_W: f32 = 30.0;
pub const TAB_H: f32 = 88.0;

/// The tab's size in screen pixels for a card scaled by `s` (card width over its design width).
pub fn tab_size(s: f32) -> (i32, i32) {
    (((TAB_W * s).round() as i32).max(18), ((TAB_H * s).round() as i32).max(48))
}

/// The scene's name as the tab writes it: its first word, at most three characters.
pub fn tab_label(scene: &str) -> String {
    scene.split_whitespace().next().unwrap_or("").chars().take(3).collect()
}

const CYAN: [u8; 3] = [64, 200, 236];
const SKY: [u8; 3] = [214, 232, 248];
const CAP_TOP: [u8; 3] = [48, 74, 122];
const SIGNAL: [u8; 3] = [38, 160, 96];
const YELLOW: [u8; 3] = [250, 204, 60];

fn stroke_line(img: &mut RgbaImage, a: (f32, f32), b: (f32, f32), t: f32, rgb: [u8; 3]) {
    let n = ((b.0 - a.0).hypot(b.1 - a.1) * 2.0).ceil().max(1.0) as i32;
    for i in 0..=n {
        let f = i as f32 / n as f32;
        let (x, y) = (a.0 + (b.0 - a.0) * f, a.1 + (b.1 - a.1) * f);
        fill_rrect(img, x - t / 2.0, y - t / 2.0, t, t, t / 2.0, |_, _| (rgb, 1.0));
    }
}

fn disc(img: &mut RgbaImage, cx: f32, cy: f32, r: f32, rgb: [u8; 3]) {
    fill_rrect(img, cx - r, cy - r, 2.0 * r, 2.0 * r, r, |_, _| (rgb, 1.0));
}

/// The tab body drawn for a card that opens to the right: the tab hangs on its left edge (small
/// corners), its open side is rounder, and the top corner on that side is cut at 45 degrees like
/// the card's own header. Navy outline, a navy cap on top, a white-to-sky body, a cyan stripe
/// down the open side.
fn body(img: &mut RgbaImage, cap: f32, s: f32) {
    let (w, h) = (img.width() as f32, img.height() as f32);
    let b = (1.0 * s).max(1.0);
    let (big, small) = (9.0 * s, 3.0 * s);
    let shape = |img: &mut RgbaImage, inset: f32, color: &dyn Fn(f32, f32) -> ([u8; 3], f32)| {
        let (x, y, iw, ih) = (inset, inset, w - 2.0 * inset, h - 2.0 * inset);
        fill_rrect(img, x, y, iw, ih, (big - inset).max(1.0), |px, py| color(px + inset, py + inset));
        fill_rrect(img, x, y, iw * 0.5, ih, (small - inset).max(0.5), |px, py| color(px + inset, py + inset));
    };
    shape(img, 0.0, &|_, _| (NAVY, 1.0));
    shape(img, b, &|x, y| {
        if y < cap {
            (crate::render::lerp(CAP_TOP, NAVY, y / cap), 1.0)
        } else if x > w - b - 3.0 * s && x < w - b - 1.0 * s {
            (CYAN, 1.0)
        } else {
            (crate::render::lerp(WHITE, SKY, (y - cap) / (h - cap)), 1.0)
        }
    });
    let cut = 7.0 * s;
    for y in 0..img.height() {
        for x in 0..img.width() {
            let d = (w - cut + y as f32 + 0.5) - (x as f32 + 0.5);
            let a = (0.5 + d * std::f32::consts::FRAC_1_SQRT_2).clamp(0.0, 1.0);
            if a < 1.0 {
                let p = img.get_pixel_mut(x, y);
                p[3] = (p[3] as f32 * a).round() as u8;
            }
        }
    }
}

/// The folded card as a bookmark tab. The mark of the add-on (a cyan lens with a yellow spark)
/// sits in the navy cap, the scene's short name `label` is written downward on the pale body,
/// and a chevron at the foot points the way the card opens. While `busy` (a scan runs) a green
/// status dot with a white ring sits at the mark. `s` is the card's scale. With a layer too small
/// to read (`s` below 0.7) only the mark and the chevron are drawn.
pub fn render_tab(size: (i32, i32), unfold: Unfold, busy: bool, label: &str, fonts: &Fonts, s: f32) -> RgbaImage {
    let (w, h) = (size.0 as f32, size.1 as f32);
    let mut img = RgbaImage::from_pixel(size.0 as u32, size.1 as u32, Rgba([0, 0, 0, 0]));
    let cap = 26.0 * s;
    body(&mut img, cap, s);
    if unfold == Unfold::Left {
        img = image::imageops::flip_horizontal(&img);
    }
    let cx = w / 2.0;
    // the mark
    let cy = cap * 0.5;
    disc(&mut img, cx, cy, 6.8 * s, WHITE);
    disc(&mut img, cx, cy, 5.2 * s, CYAN);
    disc(&mut img, cx, cy, 1.9 * s, WHITE);
    if busy {
        disc(&mut img, cx + 6.5 * s, cy + 6.5 * s, 4.4 * s, WHITE);
        disc(&mut img, cx + 6.5 * s, cy + 6.5 * s, 3.2 * s, SIGNAL);
    } else {
        disc(&mut img, cx + 5.8 * s, cy - 5.8 * s, 2.2 * s, YELLOW);
    }
    // the scene, one character per row
    let px = 12.0 * s;
    if s >= 0.7 {
        let chars: Vec<char> = label.chars().take(3).collect();
        let step = 13.5 * s;
        let top = 29.0 * s + (3 - chars.len()) as f32 * step / 2.0;
        for (i, c) in chars.iter().enumerate() {
            let t = c.to_string();
            let x = cx - text_width(&fonts.semibold, px, &t) / 2.0;
            draw_text(&mut img, &fonts.semibold, px, x, top + i as f32 * step, INK, 1.0, &t);
        }
    }
    // chevron
    let (cy, arm, t) = (h - 11.0 * s, 4.2 * s, (1.8 * s).max(1.3));
    let dir = if unfold == Unfold::Left { -1.0 } else { 1.0 };
    let tip = (cx + dir * arm * 0.5, cy);
    stroke_line(&mut img, (cx - dir * arm * 0.5, cy - arm), tip, t, BLUE);
    stroke_line(&mut img, (cx - dir * arm * 0.5, cy + arm), tip, t, BLUE);
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
        assert_eq!(tab_size(1.0), (30, 88));
        assert_eq!(tab_size(1.2), (36, 106));
        assert!(tab_size(0.5).0 >= 18, "never too small to hit");
    }

    #[test]
    fn the_label_is_the_first_word_cut_to_three_characters() {
        assert_eq!(tab_label("편성"), "편성");
        assert_eq!(tab_label("이벤트 일람"), "이벤트");
        assert_eq!(tab_label("스케줄러"), "스케줄");
        assert_eq!(tab_label(""), "");
    }

    fn count(img: &RgbaImage, f: impl Fn(&Rgba<u8>) -> bool) -> usize {
        img.pixels().filter(|p| f(p)).count()
    }

    #[test]
    fn the_tab_is_a_bookmark_with_a_mark_a_label_and_a_chevron_that_mirror() {
        let Some(f) = load_fonts() else { return };
        let size = tab_size(1.0);
        let right = render_tab(size, Unfold::Right, false, "편성", &f, 1.0);
        let left = render_tab(size, Unfold::Left, false, "편성", &f, 1.0);
        assert_eq!((right.width(), right.height()), (30, 88));
        // the open side is rounder and its top corner is cut: transparent where the anchored side is solid
        assert_eq!(right.get_pixel(29, 0)[3], 0, "cut corner on the open side");
        assert!(right.get_pixel(2, 2)[3] > 200, "anchored side keeps its corner");
        assert_eq!(left.get_pixel(0, 0)[3], 0, "mirrored");
        assert!(left.get_pixel(27, 2)[3] > 200);
        assert!(right.get_pixel(15, 60)[3] > 250, "solid body");
        // the mark: a cyan lens in the cap with the label's ink below it
        let cyan = |p: &Rgba<u8>| p.0[0] < 90 && p.0[1] > 180 && p.0[2] > 220 && p[3] > 250;
        let lens = |img: &RgbaImage| (0..26).any(|y| (8..22).any(|x| cyan(img.get_pixel(x, y))));
        assert!(lens(&right) && lens(&left));
        let ink = |p: &Rgba<u8>| p.0[0] < 140 && p.0[1] < 140 && p[3] > 200;
        let inked = |img: &RgbaImage| (28..72).flat_map(|y| (4..24).map(move |x| (x, y))).filter(|(x, y)| ink(img.get_pixel(*x, *y))).count();
        assert!(inked(&right) > 20, "the scene's name is written on the body");
        let blank = render_tab(size, Unfold::Right, false, "", &f, 1.0);
        assert_eq!(inked(&blank), 0, "no scene, no text");
        assert!(inked(&render_tab(size, Unfold::Right, false, "편성", &f, 1.0)) == inked(&right));
        // the chevron's tip (its middle row) is on the side it opens to, its ends are not
        let blue = |p: &Rgba<u8>| p.0[2] > 170 && p.0[0] < 70 && p.0[1] > 90 && p.0[1] < 140;
        let tip_vs_end = |img: &RgbaImage| -> (f32, f32) {
            let rows: Vec<u32> = (72..88).filter(|y| (0..30).any(|x| blue(img.get_pixel(x, *y)))).collect();
            let mean_x = |y: u32| {
                let xs: Vec<u32> = (0..30).filter(|x| blue(img.get_pixel(*x, y))).collect();
                xs.iter().sum::<u32>() as f32 / xs.len() as f32
            };
            (mean_x((rows[0] + rows[rows.len() - 1]) / 2), mean_x(rows[0]))
        };
        let (rt, re) = tip_vs_end(&right);
        let (lt, le) = tip_vs_end(&left);
        assert!(rt > re && lt < le, "right {rt} {re}, left {lt} {le}");
        // a status dot only while busy, and the spark only while idle
        let busy = render_tab(size, Unfold::Right, true, "편성", &f, 1.0);
        let green = |img: &RgbaImage| count(img, |p| p.0[1] > 140 && p.0[0] < 60 && p.0[2] < 120 && p[3] > 250);
        let yellow = |img: &RgbaImage| count(img, |p| p.0[0] > 230 && p.0[1] > 190 && p.0[2] < 100 && p[3] > 250);
        assert!(green(&busy) > 10 && green(&right) == 0);
        assert!(yellow(&right) > 5 && yellow(&busy) == 0);
        preview("tab-right.png", &right);
        preview("tab-left.png", &left);
        preview("tab-busy.png", &busy);
        preview("snap-hint.png", &render_snap_hint((300, 200), 1.0));
    }

    #[test]
    fn a_small_window_keeps_the_mark_and_drops_the_text() {
        let Some(f) = load_fonts() else { return };
        let s = 0.5;
        let size = tab_size(s);
        assert_eq!(size, (18, 48));
        let img = render_tab(size, Unfold::Right, false, "편성", &f, s);
        assert_eq!((img.width(), img.height()), (18, 48));
        let ink = count(&image::imageops::crop_imm(&img, 2, 16, 12, 18).to_image(), |p| p.0[0] < 90 && p.0[1] < 90 && p[3] > 200);
        assert_eq!(ink, 0, "no text on the body");
        assert!(img.get_pixel(9, 20)[3] > 250);
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
                    let shown = if p <= 0.0 { render_tab(tab_size(s), r.unfold, false, &tab_label(&state.scene), &f, s) } else { part };
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

    /// All four anchors, folded and open, over a real screen: the previews of the tab's look.
    #[test]
    fn the_tab_at_every_anchor_folded_and_open() {
        use crate::dock::{Anchor, Corner, Side};
        use crate::panel::{inside_card_width, panel_width, PANEL_BASE_W};
        use crate::placement::{resolve, strip, Frame};
        let Some(f) = load_fonts() else { return };
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../fixtures/screens");
        let Ok(shot) = image::open(dir.join("quick-formation.png")) else { return };
        let (cw, ch, margin) = (1280u32, 960u32, 420u32);
        let (tw, th) = (cw + 2 * margin, ch);
        let mut stage = RgbaImage::from_pixel(tw, th, Rgba([58, 66, 82, 255]));
        image::imageops::overlay(&mut stage, &image::imageops::resize(&shot.to_rgba8(), cw, ch, image::imageops::FilterType::Triangle), margin as i64, 0);
        let frame = Frame { client: (margin as i32, 0, (margin + cw) as i32, ch as i32), outer: (margin as i32, 0, (margin + cw) as i32, ch as i32), work: (0, 0, tw as i32, th as i32) };
        let state = PanelState {
            scene: "편성".into(),
            title: "19-3 설원 지구 제2 지역".into(),
            rows: vec![Row::Info { label: "지형".into(), value: "야외".into() }, Row::Button { id: "scan".into(), label: "학생 스캔".into(), value: "시작".into(), on: false }],
            close: Some(("quit".into(), "애드온 종료".into())),
            pin: Some(false),
            ..Default::default()
        };
        for (anchor, tag) in [
            (Anchor::Outside(Side::Right), "outside-r"),
            (Anchor::Outside(Side::Left), "outside-l"),
            (Anchor::Inside(Corner::TopRight), "inside-tr"),
            (Anchor::Inside(Corner::TopLeft), "inside-tl"),
        ] {
            let width = if anchor.is_inside() { inside_card_width(cw as i32) } else { panel_width(cw as i32) } as u32;
            let s = width as f32 / PANEL_BASE_W;
            let (card, _) = render_panel_hits(width, &state, &f);
            let r = resolve(frame, anchor, (card.width() as i32, card.height() as i32), tab_size(s));
            for (p, name) in [(0.0, "folded"), (1.0, "open")] {
                let mut canvas = stage.clone();
                let (rc, src) = strip(&r, p);
                let shown = if p <= 0.0 { render_tab(tab_size(s), r.unfold, false, &tab_label(&state.scene), &f, s) } else { image::imageops::crop_imm(&card, src as u32, 0, (rc.2 - rc.0) as u32, (rc.3 - rc.1) as u32).to_image() };
                image::imageops::overlay(&mut canvas, &shown, rc.0 as i64, rc.1 as i64);
                // a crop around the card's edge keeps the previews readable
                let x0 = (r.tab.0 - 260).clamp(0, tw as i32 - 520) as u32;
                let view = image::imageops::crop_imm(&canvas, x0, 0, 520, 260).to_image();
                preview(&format!("anchor-{tag}-{name}.png"), &image::imageops::resize(&view, 1040, 520, image::imageops::FilterType::Nearest));
            }
            let busy = render_tab(tab_size(s), r.unfold, true, &tab_label(&state.scene), &f, s);
            preview(&format!("anchor-{tag}-busy.png"), &image::imageops::resize(&busy, busy.width() * 6, busy.height() * 6, image::imageops::FilterType::Nearest));
        }
    }
}
