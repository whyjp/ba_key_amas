//! Pixel-only readers for the Blue Archive profile / roster frame (1280x960 for 4:3, 1280x720 for 16:9).
//!
//! Every ROI constant below was measured on the 4:3 frame. The 16:9 frame keeps the same pixel sizes but
//! moves things vertically: centred panels and popups sit 120 px higher (`CENTRED_DY`), the list areas
//! end 240 px higher (the frame is that much shorter). The readers that are known in 16:9 (profile
//! recognition, student list, quick formation) apply those shifts from the frame's height; the profile
//! readers of the scan (badges, locks, stars, name strip ...) and the combat HUD are 4:3 only.
//!
//! Ports the no-OCR logic of the Go program (`meta.go`, `capture.go`,
//! `roster_detect.go`, `arrow_scan.go`, `deep.go`, `ppocr.go`, `main.go`).
//! Every threshold and ROI rectangle is identical to Go; ROIs are exposed as
//! `pub const` `(x0, y0, x1, y1)` tuples (half-open, like Go's `image.Rectangle`).
//!
//! Resampling difference: Go uses `x/image/draw.CatmullRom.Scale`; this crate
//! uses `image::imageops::resize(.., FilterType::CatmullRom)`. Both are the
//! same Catmull-Rom (B=0, C=0.5) kernel with support widened on downscale, but
//! rounding and edge handling differ slightly, so fingerprint bytes can differ
//! from Go by a few counts. Distances between identical inputs are exactly 0,
//! and thresholds (2 / 1.5) leave room for this.
//!
//! Out-of-bounds reads behave like Go's `Image.At` (black), never panic.

use baocr_core::model::{Badges, Locks, SkillLocked, Terrain};
use baocr_core::layout::Layout;
use image::{imageops, GrayImage, Luma, RgbImage};
use once_cell::sync::Lazy;

mod battle;
mod ink;
pub use battle::*;
pub use ink::*;

/// `(x0, y0, x1, y1)`, half-open.
pub type Rect = (i32, i32, i32, i32);

// ---- ROI constants ---------------------------------------------------------

pub const FRAME_W: u32 = 1280;
pub const FRAME_H: u32 = 960;
/// Detail header band (dark navy "능력치" bar).
pub const DETAIL_BAND_ROI: Rect = (700, 314, 1060, 330);
pub const ROLE_ICON_ROI: Rect = (72, 869, 97, 897);
pub const ATTACK_BADGE_ROI: Rect = (205, 872, 285, 894);
pub const ARMOR_BADGE_ROI: Rect = (205, 907, 285, 929);
pub const NAME_ROI: Rect = (64, 789, 300, 835);
pub const HEART_ROI: Rect = (34, 794, 84, 832);
/// Right-hand info panel (the left side holds an idle-animated character).
pub const PANEL_ROI: Rect = (660, 230, 1215, 780);
/// Name strip fingerprinted by `detail_key`.
pub const DETAIL_KEY_ROI: Rect = (65, 794, 304, 837);
pub const PIN_ROI: Rect = (45, 762, 120, 785);
/// Terrain face centres (x) and vertical band.
pub const TERRAIN_FACE_X: [i32; 3] = [303, 343, 383];
pub const TERRAIN_FACE_Y: (i32, i32) = (903, 931);
pub const BADGE_DOTS: [(i32, i32); 6] = [(829, 238), (1015, 238), (1195, 238), (1196, 441), (1196, 573), (1196, 678)];
pub const SKILL_CARD_X: [i32; 4] = [740, 846, 952, 1058];
pub const LOCK_ABILITY_TEAL_ROI: Rect = (1112, 345, 1196, 416);
pub const LOCK_ABILITY_WHITE_ROI: Rect = (1135, 365, 1175, 400);
pub const LOCK_SIMSANG_GREY_ROI: Rect = (588, 292, 640, 340);
pub const LOCK_SIMSANG_WHITE_ROI: Rect = (606, 308, 626, 332);
pub const STAR_X: [i32; 5] = [270, 285, 300, 315, 330];
pub const WEAPON_STAR_X: [i32; 4] = [1029, 1045, 1061, 1077];
pub const SCROLL_COLUMN_X: i32 = 1230;

// ---- pixel helpers -----------------------------------------------------------

/// How much higher centred panels and popups sit in 16:9 than in 4:3 (the frame is 240 px shorter).
const CENTRED_DY: i32 = -120;

fn layout_of(img: &RgbImage) -> Layout {
    Layout::of_frame(img.width(), img.height())
}

/// The vertical shift of centred panels (the profile's right panel, popups) in this frame.
fn centred_dy(img: &RgbImage) -> i32 {
    match layout_of(img) {
        Layout::Ratio43 => 0,
        Layout::Ratio169 => CENTRED_DY,
    }
}

fn shifted(r: Rect, dy: i32) -> Rect {
    (r.0, r.1 + dy, r.2, r.3 + dy)
}

#[inline]
fn px(img: &RgbImage, x: i32, y: i32) -> [u8; 3] {
    if x < 0 || y < 0 || x >= img.width() as i32 || y >= img.height() as i32 {
        [0, 0, 0]
    } else {
        img.get_pixel(x as u32, y as u32).0
    }
}

#[inline]
fn absd(a: i32, b: i32) -> i32 {
    (a - b).abs()
}

fn rect_dx(r: Rect) -> i32 {
    r.2 - r.0
}
fn rect_dy(r: Rect) -> i32 {
    r.3 - r.1
}

/// Copy of `r` clipped to the image (Go `crop`).
pub fn crop(img: &RgbImage, r: Rect) -> RgbImage {
    let x0 = r.0.clamp(0, img.width() as i32);
    let y0 = r.1.clamp(0, img.height() as i32);
    let x1 = r.2.clamp(x0, img.width() as i32);
    let y1 = r.3.clamp(y0, img.height() as i32);
    imageops::crop_imm(img, x0 as u32, y0 as u32, (x1 - x0) as u32, (y1 - y0) as u32).to_image()
}

/// Catmull-Rom resize (Go `resizeNearest`, despite its name, is CatmullRom).
pub fn resize_catmull_rom(img: &RgbImage, w: u32, h: u32) -> RgbImage {
    imageops::resize(img, w, h, imageops::FilterType::CatmullRom)
}

/// Fraction of pixels in `r` accepted by `f` (rgb).
pub fn fraction(img: &RgbImage, r: Rect, f: impl Fn([u8; 3]) -> bool) -> f64 {
    let (mut n, mut t) = (0u32, 0u32);
    for y in r.1..r.3 {
        for x in r.0..r.2 {
            t += 1;
            if f(px(img, x, y)) {
                n += 1;
            }
        }
    }
    n as f64 / t as f64
}

// ---- badges ----------------------------------------------------------------------

/// Whether a notification dot is drawn inside `r`.
pub fn orange_dot(img: &RgbImage, r: Rect) -> bool {
    let mut n = 0;
    for y in r.1..r.3 {
        for x in r.0..r.2 {
            let [cr, cg, cb] = px(img, x, y);
            if cr > 225 && cg > 140 && cg < 215 && cb < 90 {
                n += 1;
            }
        }
    }
    n >= 20
}

fn dot_at(cx: i32, cy: i32) -> Rect {
    (cx - 10, cy - 10, cx + 10, cy + 10)
}

pub fn read_badges(img: &RgbImage) -> Badges {
    let d = |i: usize| Some(orange_dot(img, dot_at(BADGE_DOTS[i].0, BADGE_DOTS[i].1)));
    Badges {
        basic_info_tab: d(0),
        level_up_tab: d(1),
        mystic_release_tab: d(2),
        skill_growth: d(3),
        weapon_growth: d(4),
        equipment_growth: d(5),
    }
}

/// `Some(true)` only when a lock glyph is proven; never `Some(false)`.
pub fn read_locks(img: &RgbImage) -> Locks {
    let mut l = Locks::default();
    let teal = fraction(img, LOCK_ABILITY_TEAL_ROI, |[r, g, b]| {
        let (r, g, b) = (r as i32, g as i32, b as i32);
        r < 80 && g > 50 && g < 120 && b > 60 && b < 135 && b > r + 15
    });
    let white = fraction(img, LOCK_ABILITY_WHITE_ROI, |[r, g, b]| r > 220 && g > 220 && b > 220);
    if teal > 0.35 && white > 0.04 {
        l.ability_unlock_locked = Some(true);
    }
    let grey = fraction(img, LOCK_SIMSANG_GREY_ROI, |[r, _, b]| {
        let d = r as i32 - b as i32;
        r > 40 && r < 130 && d > -14 && d < 14
    });
    let w2 = fraction(img, LOCK_SIMSANG_WHITE_ROI, |[r, g, b]| r > 215 && g > 215 && b > 215);
    if grey > 0.25 && w2 > 0.04 {
        l.simsang_bloom_locked = Some(true);
    }
    l
}

// ---- attack / armor type colours ---------------------------------------------------

/// Mean of strongly saturated pixels (the badge background, not text).
/// `[-1,-1,-1]` when fewer than 40 such pixels.
pub fn sat_mean(img: &RgbImage, r: Rect) -> [i32; 3] {
    let mut s = [0i32; 3];
    let mut n = 0;
    for y in r.1..r.3 {
        for x in r.0..r.2 {
            let c = px(img, x, y);
            let mx = *c.iter().max().unwrap() as i32;
            let mn = *c.iter().min().unwrap() as i32;
            if mx - mn > 60 {
                for i in 0..3 {
                    s[i] += c[i] as i32;
                }
                n += 1;
            }
        }
    }
    if n < 40 {
        return [-1, -1, -1];
    }
    [s[0] / n, s[1] / n, s[2] / n]
}

pub const ATTACK_SWATCHES: [([i32; 3], &str); 4] =
    [([147, 6, 13], "폭발"), ([191, 141, 15], "관통"), ([40, 113, 156], "신비"), ([155, 72, 168], "진동")];
pub const ARMOR_SWATCHES: [([i32; 3], &str); 5] = [
    ([148, 12, 17], "경장갑"),
    ([192, 142, 19], "중장갑"),
    ([42, 115, 157], "특수장갑"),
    ([156, 75, 169], "탄력장갑"),
    ([25, 122, 117], "복합장갑"),
];

/// Nearest palette label, or "" when unsure (distance > 60 or no colour).
pub fn nearest_swatch(c: [i32; 3], set: &[([i32; 3], &str)]) -> String {
    if c[0] < 0 {
        return String::new();
    }
    let (mut best, mut bl) = (1 << 30, "");
    for (rgb, label) in set {
        let d = absd(c[0], rgb[0]) + absd(c[1], rgb[1]) + absd(c[2], rgb[2]);
        if d < best {
            best = d;
            bl = label;
        }
    }
    if best > 60 {
        return String::new();
    }
    bl.to_string()
}

/// `(attack_type, armor_type)` from the badge colours; "" when unknown.
pub fn attack_armor_types(img: &RgbImage) -> (String, String) {
    (
        nearest_swatch(sat_mean(img, ATTACK_BADGE_ROI), &ATTACK_SWATCHES),
        nearest_swatch(sat_mean(img, ARMOR_BADGE_ROI), &ARMOR_SWATCHES),
    )
}

// ---- terrain -------------------------------------------------------------------------

/// 4 green+open mouth, 3 green, 2 yellow, 1 red frown, 0 red angry mouth.
pub fn face_grade(img: &RgbImage, cx: i32) -> Option<i32> {
    let (mut sum, mut n, mut dark) = ([0i32; 3], 0i32, 0i32);
    for y in TERRAIN_FACE_Y.0..TERRAIN_FACE_Y.1 {
        for x in (cx - 14)..(cx + 14) {
            let c = px(img, x, y);
            let mx = *c.iter().max().unwrap() as i32;
            let mn = *c.iter().min().unwrap() as i32;
            if mx - mn > 90 {
                for i in 0..3 {
                    sum[i] += c[i] as i32;
                }
                n += 1;
            }
            if mx < 90 {
                dark += 1;
            }
        }
    }
    if n < 150 {
        return None;
    }
    let (r, g, b) = (sum[0] / n, sum[1] / n, sum[2] / n);
    if r < 200 && g > 215 && b < 160 {
        Some(if dark >= 50 { 4 } else { 3 })
    } else if r > 235 && g > 215 && b < 130 {
        Some(2)
    } else if r > 235 && g > 120 && g < 165 && b < 150 {
        Some(if dark >= 28 { 0 } else { 1 })
    } else {
        None
    }
}

pub fn read_terrain(img: &RgbImage) -> Terrain {
    Terrain {
        urban: face_grade(img, TERRAIN_FACE_X[0]),
        outdoor: face_grade(img, TERRAIN_FACE_X[1]),
        indoor: face_grade(img, TERRAIN_FACE_X[2]),
    }
}

// ---- role icon -----------------------------------------------------------------------

/// Dark glyph pixels of the white role badge.
fn icon_mask(img: &RgbImage, r: Rect) -> Vec<bool> {
    let mut m = Vec::with_capacity((rect_dx(r) * rect_dy(r)).max(0) as usize);
    for y in r.1..r.3 {
        for x in r.0..r.2 {
            let [cr, cg, cb] = px(img, x, y);
            m.push((cr as i32 + cg as i32 + cb as i32) < 3 * 130);
        }
    }
    m
}

struct RoleIcon {
    label: String,
    mask: Vec<bool>,
}

static ROLE_ICON_FILES: [(&str, &[u8]); 4] = [
    ("딜러", include_bytes!("../data/roleicons/딜러.png")),
    ("서포터", include_bytes!("../data/roleicons/서포터.png")),
    ("탱커", include_bytes!("../data/roleicons/탱커.png")),
    ("힐러", include_bytes!("../data/roleicons/힐러.png")),
];

static ROLE_ICONS: Lazy<Vec<RoleIcon>> = Lazy::new(|| {
    ROLE_ICON_FILES
        .iter()
        .filter_map(|(label, bytes)| {
            let rgba = image::load_from_memory(bytes).ok()?.to_rgba8();
            // Go converts through premultiplied RGBA; identical for opaque icons.
            let mut rgb = RgbImage::new(rgba.width(), rgba.height());
            for (x, y, p) in rgba.enumerate_pixels() {
                let a = p[3] as u32;
                let f = |c: u8| ((c as u32 * a + 127) / 255) as u8;
                rgb.put_pixel(x, y, image::Rgb([f(p[0]), f(p[1]), f(p[2])]));
            }
            Some(RoleIcon { label: label.to_string(), mask: icon_mask(&rgb, (0, 0, rgb.width() as i32, rgb.height() as i32)) })
        })
        .collect()
});

/// Closest role icon label, or "" when no template is within 12% differing pixels.
pub fn role_from_icon(img: &RgbImage) -> String {
    let m = icon_mask(img, ROLE_ICON_ROI);
    let (mut best, mut bl) = (2.0f64, "");
    for ic in ROLE_ICONS.iter() {
        if ic.mask.len() != m.len() {
            continue;
        }
        let diff = m.iter().zip(&ic.mask).filter(|(a, b)| a != b).count();
        let f = diff as f64 / m.len() as f64;
        if f < best {
            best = f;
            bl = &ic.label;
        }
    }
    if best > 0.12 {
        return String::new();
    }
    bl.to_string()
}

// ---- skill locks -----------------------------------------------------------------------

/// Unlocked cards have a white face under the icon, locked ones a dark overlay.
pub fn skill_card_locked(img: &RgbImage, cx: i32) -> bool {
    fraction(img, (cx - 28, 522, cx + 28, 538), |[r, g, b]| r > 200 && g > 200 && b > 200) < 0.3
}

pub fn read_skill_locks(img: &RgbImage) -> SkillLocked {
    SkillLocked {
        ex: skill_card_locked(img, SKILL_CARD_X[0]),
        basic: skill_card_locked(img, SKILL_CARD_X[1]),
        passive: skill_card_locked(img, SKILL_CARD_X[2]),
        sub: skill_card_locked(img, SKILL_CARD_X[3]),
    }
}

// ---- capture.go -------------------------------------------------------------------------

/// Profile layout recognised by the dark-navy 능력치 header band.
pub fn detail_visible(img: &RgbImage) -> bool {
    // Dark-navy 능력치 header band...
    let dy = centred_dy(img);
    let band = fraction(img, shifted(DETAIL_BAND_ROI, dy), |[r, g, b]| r < 75 && g > 50 && g < 105 && b > 85 && b < 150) > 0.6;
    // ...above the panel's light body. The band colour alone also matches dark blue
    // wallpapers (the main menu), so the white stat rows under it must be there too.
    band && fraction(img, shifted(DETAIL_BODY_ROI, dy), |[r, g, b]| r > 190 && g > 190 && b > 190) > 0.5
}

/// The stat rows right under the header band: white panel background with text.
pub const DETAIL_BODY_ROI: Rect = (700, 345, 1060, 372);

/// Whether white name glyphs reach the right edge of `r` (strip cut the text off).
pub fn text_touches_right_edge(img: &RgbImage, r: Rect) -> bool {
    let band = (r.2 - 4, r.1 + 4, r.2, r.3 - 4);
    fraction(img, band, |[r, g, b]| r > 200 && g > 200 && b > 200) > 0.04
}

// ---- main.go: stars / pin ----------------------------------------------------------------

fn gold(c: [u8; 3]) -> bool {
    let [r, g, b] = c;
    r > 140 && g > 100 && b < 140 && r > g
}

fn cyan(c: [u8; 3]) -> bool {
    let [r, g, b] = c;
    g > 125 && b > 140 && r < 180 && (b as i32) > r as i32 + 20
}

/// Yellow SELECT tag above the name: `Some(true)` >= 450 hits, `Some(false)` <= 150, else unknown.
pub fn pin_state(img: &RgbImage) -> Option<bool> {
    let mut hits = 0;
    for y in PIN_ROI.1..PIN_ROI.3 {
        for x in PIN_ROI.0..PIN_ROI.2 {
            let [r, g, b] = px(img, x, y);
            if r > 180 && g > 120 && b < 140 && r as i32 > g as i32 + 15 {
                hits += 1;
            }
        }
    }
    if hits >= 450 {
        Some(true)
    } else if hits <= 150 {
        Some(false)
    } else {
        None
    }
}

fn count_stars(img: &RgbImage, xs: &[i32], y0: i32, y1: i32, f: fn([u8; 3]) -> bool) -> Option<i32> {
    let mut count = 0;
    for &x in xs {
        let mut hit = 0;
        for xx in (x - 5)..=(x + 5) {
            for y in y0..y1 {
                if f(px(img, xx, y)) {
                    hit += 1;
                }
            }
        }
        if hit >= 24 {
            count += 1;
        }
    }
    if count == 0 {
        None
    } else {
        Some(count)
    }
}

/// Gold rarity stars (1..=5); `None` when none found.
pub fn stars(img: &RgbImage) -> Option<i32> {
    count_stars(img, &STAR_X, 806, 831, gold)
}

/// Cyan unique-weapon stars (1..=4); `None` when none found.
pub fn weapon_stars(img: &RgbImage) -> Option<i32> {
    count_stars(img, &WEAPON_STAR_X, 627, 650, cyan)
}

// ---- roster_detect.go ------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Thumb {
    pub top: i32,
    pub bottom: i32,
}

/// Last row of the student list's viewport (4:3: 945; 16:9: the frame is 240 px shorter).
fn list_bottom(img: &RgbImage) -> i32 {
    match layout_of(img) {
        Layout::Ratio43 => 945,
        Layout::Ratio169 => 705,
    }
}

/// Scrollbar thumb on column x=1230; `{-1,-1}` when absent.
pub fn scroll_thumb(img: &RgbImage) -> Thumb {
    let mut best = Thumb { top: -1, bottom: -1 };
    let mut start = -1;
    let bottom = list_bottom(img);
    for y in 175..=bottom {
        let [r, g, b] = px(img, SCROLL_COLUMN_X, y);
        let hit = r < 180 && g < 200 && b < 210;
        if hit && start < 0 {
            start = y;
        }
        if (!hit || y == bottom) && start >= 0 {
            let end = if hit { y } else { y - 1 };
            if end - start >= 20 && end - start > best.bottom - best.top {
                best = Thumb { top: start, bottom: end };
            }
            start = -1;
        }
    }
    best
}

pub type CardSignature = [u8; 768];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Card {
    pub x: i32,
    pub y: i32,
    pub row: i32,
    pub column: i32,
    pub signature: CardSignature,
}

fn flatten(img: &RgbImage, out: &mut [u8]) {
    for (i, p) in img.pixels().enumerate() {
        out[i * 3..i * 3 + 3].copy_from_slice(&p.0);
    }
}

pub fn card_signature(img: &RgbImage, cx: i32, end: i32) -> CardSignature {
    let area = crop(img, (cx - 55, end - 170, cx + 55, end - 55));
    let small = resize_catmull_rom(&area, 16, 16);
    let mut sig = [0u8; 768];
    flatten(&small, &mut sig);
    sig
}

pub fn signature_distance(a: &CardSignature, b: &CardSignature) -> f64 {
    let sum: i32 = a.iter().zip(b).map(|(x, y)| absd(*x as i32, *y as i32)).sum();
    sum as f64 / a.len() as f64
}

/// Card grid rows found by the left-edge colour variance scan; 6 cards per row.
/// Empty unless the frame is exactly 1280x960 or 1280x720; the 16:9 list is the same grid in a shorter viewport.
pub fn roster_cards(img: &RgbImage) -> Vec<Card> {
    if img.width() != FRAME_W || !matches!(img.height(), 960 | 720) {
        return Vec::new();
    }
    let shift = list_bottom(img) - 945; // 0, or -240 in 16:9
    let (scan_to, end_max) = (930 + shift, 920 + shift);
    struct Group {
        start: i32,
        end: i32,
        count: i32,
    }
    let mut groups: Vec<Group> = Vec::new();
    for y in 170..=scan_to {
        let a = px(img, 50, y);
        let b = px(img, 58, y);
        let c = px(img, 65, y);
        let mut diff = 0;
        for i in 0..3 {
            diff += absd(a[i] as i32, b[i] as i32) + absd(a[i] as i32, c[i] as i32);
        }
        if diff <= 120 {
            continue;
        }
        match groups.last_mut() {
            Some(g) if y - g.end <= 35 => {
                g.end = y;
                g.count += 1;
            }
            _ => groups.push(Group { start: y, end: y, count: 1 }),
        }
    }
    let mut cards = Vec::new();
    for g in &groups {
        if g.count < 60 || g.end > end_max || g.end < 290 || g.end - g.start < 90 {
            continue;
        }
        let row = (cards.len() / 6) as i32;
        for col in 0..6 {
            let x = 140 + 195 * col;
            cards.push(Card { x, y: g.end - 90, row, column: col, signature: card_signature(img, x, g.end) });
        }
    }
    cards
}

/// The dark-blue "미보유" (not owned) banner that separates the owned students from
/// the rest of the list (4:3 only, like the scan that uses it): a full-width navy band inside the card area. Returns its
/// centre row, or None while it is not on screen.
pub fn unowned_banner(img: &RgbImage) -> Option<i32> {
    if img.width() != FRAME_W || img.height() != FRAME_H {
        return None;
    }
    let navy = |y: i32| {
        let xs: Vec<i32> = (70..1190).step_by(8).collect();
        let hits = xs.iter().filter(|&&x| matches!(px(img, x, y), [r, g, b] if r < 70 && g > 50 && g < 110 && b > 80 && b < 150)).count();
        hits as f64 / xs.len() as f64 > 0.8
    };
    let mut best: Option<(i32, i32)> = None;
    let mut start: Option<i32> = None;
    for y in 170..=946 {
        let on = y <= 945 && navy(y);
        match (on, start) {
            (true, None) => start = Some(y),
            (false, Some(s)) => {
                let end = y - 1;
                if end - s >= 20 && best.is_none_or(|(a, b)| end - s > b - a) {
                    best = Some((s, end));
                }
                start = None;
            }
            _ => {}
        }
    }
    best.map(|(a, b)| (a + b) / 2)
}

/// How far the roster moved up between two pages, in frame pixels.
///
/// Rows of sampled gray values are matched between the pages; the shift with the
/// smallest difference wins, and only if it is clearly better than any other (a
/// repeating grid must not be mistaken for a shift of one line). None if the pages
/// do not overlap enough, so a scroll that went too far cannot be counted as
/// progress. Card positions depend on where the list is scrolled to; this is how
/// they are tied to one absolute coordinate. 4:3 only, like the student scan that uses it.
pub fn list_shift(before: &RgbImage, after: &RgbImage, max_shift: i32) -> Option<i32> {
    const TOP: i32 = 185;
    const BOTTOM: i32 = 935;
    const COLS: usize = 24;
    if before.dimensions() != (FRAME_W, FRAME_H) || after.dimensions() != (FRAME_W, FRAME_H) {
        return None;
    }
    let profile = |img: &RgbImage| -> Vec<[u8; COLS]> {
        (TOP..BOTTOM)
            .map(|y| {
                let mut row = [0u8; COLS];
                for (i, v) in row.iter_mut().enumerate() {
                    let [r, g, b] = px(img, 70 + i as i32 * 47, y);
                    *v = ((r as u32 * 3 + g as u32 * 6 + b as u32) / 10) as u8;
                }
                row
            })
            .collect()
    };
    let (a, b) = (profile(before), profile(after));
    let height = (BOTTOM - TOP) as usize;
    let cost = |d: usize| -> f64 {
        let n = height - d;
        let mut sum = 0u64;
        let mut cnt = 0u64;
        for y in (0..n).step_by(2) {
            for i in 0..COLS {
                sum += (a[y + d][i] as i32 - b[y][i] as i32).unsigned_abs() as u64;
                cnt += 1;
            }
        }
        sum as f64 / cnt as f64
    };
    let max_shift = (max_shift.max(0) as usize).min(height - 120);
    let costs: Vec<(usize, f64)> = (4..=max_shift).map(|d| (d, cost(d))).collect();
    let (best_d, best) = costs.iter().copied().min_by(|x, y| x.1.partial_cmp(&y.1).unwrap())?;
    let second = costs.iter().filter(|(d, _)| d.abs_diff(best_d) > 24).map(|c| c.1).fold(f64::MAX, f64::min);
    // a good match, clearly better than every unrelated shift
    (best < 14.0 && best * 1.8 < second).then_some(best_d as i32)
}

/// Whether a card slot (centre x, y of its portrait) holds a student, or is the
/// empty panel at the end of the last row.
pub fn slot_occupied(img: &RgbImage, x: i32, y: i32) -> bool {
    let (mut n, mut ink) = (0, 0);
    for dy in (-60..=40).step_by(10) {
        for dx in (-70..=70).step_by(10) {
            // the list is clipped by its header and the screen edge; the row's own position
            // varies with the scroll, so only look at what is really inside the list
            if y + dy < 185 || y + dy > 935 {
                continue;
            }
            let [r, g, b] = px(img, x + dx, y + dy);
            n += 1;
            if !(r > 225 && g > 235 && b > 238) {
                ink += 1;
            }
        }
    }
    n > 0 && ink as f64 / n as f64 > 0.4
}

pub fn same_cards(a: &[Card], b: &[Card]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| signature_distance(&x.signature, &y.signature) <= 2.0)
}

/// The part of the frame that holds the 빠른 편성 grid's cards and their name labels, as (x, y, width, height): reading
/// the text of this alone takes a fifth of what reading the whole frame does.
pub fn quick_grid_crop(img: &RgbImage) -> (u32, u32, u32, u32) {
    let dy = centred_dy(img);
    let (x0, y0) = (LABEL_LEFT - 12, (GRID_TOP + dy - 12).max(0));
    let (x1, y1) = ((LABEL_LEFT + LABEL_COLUMNS * LABEL_PITCH + 16).min(img.width() as i32), (GRID_BOTTOM + dy + 14).min(img.height() as i32));
    (x0.max(0) as u32, y0 as u32, (x1 - x0).max(1) as u32, (y1 - y0).max(1) as u32)
}

/// A student card's name label in the 빠른 편성 grid: a solid dark-navy band under the
/// portrait with the name in white. All figures are in the 1280x960 frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CardLabel {
    /// Horizontal centre of the card.
    pub cx: i32,
    pub y0: i32,
    pub y1: i32,
}

const LABEL_COLOR: [i32; 3] = [67, 82, 102];
/// Left edge of the first card's label, the distance to the next card, cards per row.
const LABEL_LEFT: i32 = 583;
const LABEL_PITCH: i32 = 111;
const LABEL_COLUMNS: i32 = 6;
const LABEL_WIDTH: i32 = 92;
/// The grid's viewport: labels cut off by its top or bottom edge are not whole.
const GRID_TOP: i32 = 318;
const GRID_BOTTOM: i32 = 650;
/// The tab row (STRIKER / SPECIAL) of the grid.
const TAB_ROWS: (i32, i32) = (262, 296);
const LABEL_MIN_HEIGHT: i32 = 28;

fn is_label_color(p: [u8; 3]) -> bool {
    (0..3).all(|i| (p[i] as i32 - LABEL_COLOR[i]).abs() <= 14)
}

/// The whole name labels visible in the 빠른 편성 grid, wherever it has been scrolled to.
pub fn quick_card_labels(img: &RgbImage) -> Vec<CardLabel> {
    let mut out = Vec::new();
    // the popup is centred: in 16:9 it sits 120 px higher with the same size
    let dy = centred_dy(img);
    let (grid_top, grid_bottom) = (GRID_TOP + dy, GRID_BOTTOM + dy);
    for k in 0..LABEL_COLUMNS {
        let left = LABEL_LEFT + k * LABEL_PITCH;
        // a name can run from edge to edge: a row of the label counts when most of the
        // columns sampled across it have the label's colour (strokes cover only some)
        let xs: Vec<i32> = (2..LABEL_WIDTH - 1).step_by(4).map(|d| left + d).collect();
        if *xs.last().unwrap() as u32 >= img.width() {
            continue;
        }
        // rows that look like the label, then runs of them with the text's gaps closed
        let on: Vec<i32> = (grid_top..=grid_bottom.min(img.height() as i32 - 1))
            .filter(|y| xs.iter().filter(|x| is_label_color(img.get_pixel(**x as u32, *y as u32).0)).count() * 3 >= xs.len())
            .collect();
        let mut runs: Vec<(i32, i32)> = Vec::new();
        for y in on {
            match runs.last_mut() {
                Some(r) if y - r.1 <= 12 => r.1 = y,
                _ => runs.push((y, y)),
            }
        }
        for (s, e) in runs {
            // a label touching the viewport's edge is cut off
            let whole = s > grid_top && e < grid_bottom;
            if whole && e - s + 1 >= LABEL_MIN_HEIGHT {
                out.push(CardLabel { cx: left + LABEL_WIDTH / 2, y0: s, y1: e + 1 });
            }
        }
    }
    out.sort_by_key(|l| (l.y0 / 40, l.cx));
    out
}

/// A card of the 빠른 편성 grid in its thumbnail view (pictures only, no name labels).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThumbCard {
    /// Horizontal centre of the card.
    pub cx: i32,
    /// Top of the solid colour band (its class colour) along the portrait's bottom: the one thing on the card that
    /// is flat colour from side to side, so it fixes the row wherever the grid has been scrolled to.
    pub bar_top: i32,
}

impl ThumbCard {
    /// Centre of the portrait's picture.
    pub fn picture_y(&self) -> i32 {
        self.bar_top - 28
    }
    /// Top of the card's white tile.
    pub fn top(&self) -> i32 {
        self.bar_top - 79
    }
}

/// Does the row `y` hold the class colour band across the 28 px around `cx`: saturated and the same colour all along.
fn is_class_band(img: &RgbImage, cx: i32, y: i32) -> bool {
    let samples: Vec<[i32; 3]> = (-14..=14).step_by(4).map(|d| img.get_pixel((cx + d) as u32, y as u32).0.map(i32::from)).collect();
    let saturated = samples.iter().filter(|p| p.iter().max().unwrap() - p.iter().min().unwrap() >= 90).count();
    let mut med = [0i32; 3];
    for (c, m) in med.iter_mut().enumerate() {
        let mut v: Vec<i32> = samples.iter().map(|p| p[c]).collect();
        v.sort_unstable();
        *m = v[v.len() / 2];
    }
    let flat = samples.iter().filter(|p| (0..3).all(|c| (p[c] - med[c]).abs() <= 24)).count();
    saturated * 5 >= samples.len() * 4 && flat * 5 >= samples.len() * 4
}

/// The cards of the thumbnail view whose portrait is whole in the grid's viewport, wherever it has been scrolled to.
pub fn quick_thumb_cards(img: &RgbImage) -> Vec<ThumbCard> {
    let dy = centred_dy(img);
    let (grid_top, grid_bottom) = (GRID_TOP + dy, (GRID_BOTTOM + dy).min(img.height() as i32 - 1));
    let mut out = Vec::new();
    for k in 0..LABEL_COLUMNS {
        let cx = LABEL_LEFT + LABEL_WIDTH / 2 + k * LABEL_PITCH;
        if (cx + 15) as u32 >= img.width() {
            continue;
        }
        let mut runs: Vec<(i32, i32)> = Vec::new();
        for y in grid_top..=grid_bottom {
            if is_class_band(img, cx, y) {
                match runs.last_mut() {
                    Some(r) if y - r.1 <= 1 => r.1 = y,
                    _ => runs.push((y, y)),
                }
            }
        }
        for (s, e) in runs {
            // the band is 7 px thick; a card whose picture is cut off by the viewport's top is not whole
            if (4..=10).contains(&(e - s + 1)) && s - 56 >= grid_top {
                out.push(ThumbCard { cx, bar_top: s });
            }
        }
    }
    out.sort_by_key(|c| (c.bar_top / 40, c.cx));
    out
}

/// The label's text as ink (dark on white), ready for the closed-set reader.
pub fn quick_label_ink(img: &RgbImage, l: &CardLabel) -> GrayImage {
    let (x0, x1) = (l.cx - LABEL_WIDTH / 2 + 3, l.cx + LABEL_WIDTH / 2 - 3);
    let (y0, y1) = (l.y0 + 3, l.y1 - 3);
    let mut g = GrayImage::from_pixel((x1 - x0) as u32, (y1 - y0) as u32, image::Luma([255]));
    for y in y0..y1 {
        for x in x0..x1 {
            if x < 0 || y < 0 || x as u32 >= img.width() || y as u32 >= img.height() {
                continue;
            }
            let p = img.get_pixel(x as u32, y as u32).0;
            // the name is white on the navy label
            if (p[0] as u32 + p[1] as u32 + p[2] as u32) / 3 > 140 {
                g.put_pixel((x - x0) as u32, (y - y0) as u32, image::Luma([0]));
            }
        }
    }
    g
}

/// Which tab of the 빠른 편성 grid is active: `Some(true)` STRIKER, `Some(false)` SPECIAL (the
/// active tab is filled red), None when neither is (the popup is not showing).
pub fn quick_tab_is_striker(img: &RgbImage) -> Option<bool> {
    let dy = centred_dy(img);
    let red = |x0: u32, x1: u32| {
        let (mut n, mut t) = (0u32, 0u32);
        for y in ((TAB_ROWS.0 + dy) as u32)..((TAB_ROWS.1 + dy) as u32) {
            for x in x0..x1.min(img.width()) {
                if y >= img.height() {
                    continue;
                }
                let p = img.get_pixel(x, y).0;
                t += 1;
                n += u32::from(p[0] > 170 && p[1] < 80 && p[2] < 90);
            }
        }
        t > 0 && n as f64 / t as f64 > 0.3
    };
    match (red(574, 690), red(695, 810)) {
        (true, false) => Some(true),
        (false, true) => Some(false),
        _ => None,
    }
}

/// Is the student card whose name label (as `quick_card_labels` reports it) has its horizontal centre at `cx` and its top at
/// `y0` (1280x960 frame) outlined in the lime green the 빠른 편성 grid draws around the
/// students already in the squad? The band is just inside the card's left edge (not the
/// neighbour's right border), from the portrait's top down to the label.
pub fn quick_card_selected(img: &RgbImage, cx: i32, y0: i32) -> bool {
    let (x0, x1, ya, yb) = (cx - 56, cx - 51, y0 - 100, y0 - 20);
    let (mut lime, mut total) = (0u32, 0u32);
    for y in ya.max(0)..yb.min(img.height() as i32) {
        for x in x0.max(0)..x1.min(img.width() as i32) {
            let p = img.get_pixel(x as u32, y as u32).0;
            total += 1;
            if p[1] >= 230 && p[2] <= 80 && (150..=230).contains(&p[0]) {
                lime += 1;
            }
        }
    }
    total > 0 && lime as f64 / total as f64 >= 0.2
}

/// Pixel part of Go `rosterVisible`: not a profile, scrollbar thumb sized like
/// the roster's, six-card grid present, and the green-tinted All tab at (395,110).
pub fn roster_visible_pixels(img: &RgbImage) -> bool {
    if detail_visible(img) {
        return false;
    }
    let t = scroll_thumb(img);
    // the thumb is shorter in the shorter 16:9 viewport (33 px on a 16:9 roster, 62 px on a 4:3 one)
    let min_len = match layout_of(img) {
        Layout::Ratio43 => 45,
        Layout::Ratio169 => 25,
    };
    if t.top < 179 || t.bottom - t.top < min_len || t.bottom - t.top > 90 || t.bottom > list_bottom(img) || roster_cards(img).len() < 6 {
        return false;
    }
    roster_tab_selected(img)
}

/// Where the three tabs of the list (All, STRIKER, SPECIAL) can be probed in a 1280x960 frame, at the row of their labels.
const TAB_PROBES: [(i32, i32); 3] = [(395, 110), (560, 110), (700, 110)];

/// The list has one tab selected, drawn filled: teal for All, red for STRIKER, blue for SPECIAL. The game reopens the
/// list on the tab last used, so any of the three is the list. An idle tab is near white and not saturated.
fn roster_tab_selected(img: &RgbImage) -> bool {
    all_tab_selected(img) || TAB_PROBES[1..].iter().any(|&(x, y)| {
        let [r, g, b] = px(img, x, y);
        (r.max(g).max(b) as i32 - r.min(g).min(b) as i32) >= 70
    })
}

/// Is the All tab the selected one (the whole roster, not only the STRIKER or SPECIAL students)?
pub fn all_tab_selected(img: &RgbImage) -> bool {
    let [r, g, b] = px(img, TAB_PROBES[0].0, TAB_PROBES[0].1);
    g as i32 > r as i32 + 24 && b as i32 > g as i32 + 20
}

/// Where to click to select the All tab, in the 1280x960 frame.
pub const ALL_TAB_CLICK: (i32, i32) = (445, 115);

// ---- arrow_scan.go fingerprints -----------------------------------------------------------------

pub type DetailKey = [u8; 2304];
pub type FrameKey = [u8; 3072];

/// 64x12 CatmullRom thumbnail of the name strip.
pub fn detail_key(img: &RgbImage) -> DetailKey {
    let small = resize_catmull_rom(&crop(img, DETAIL_KEY_ROI), 64, 12);
    let mut k = [0u8; 2304];
    flatten(&small, &mut k);
    k
}

pub fn detail_distance(a: &DetailKey, b: &DetailKey) -> f64 {
    let sum: i32 = a.iter().zip(b).map(|(x, y)| absd(*x as i32, *y as i32)).sum();
    sum as f64 / a.len() as f64
}

/// Coarse 32x32 whole-frame fingerprint (changes during the slide-in animation).
pub fn frame_key(img: &RgbImage) -> FrameKey {
    let small = resize_catmull_rom(img, 32, 32);
    let mut k = [0u8; 3072];
    flatten(&small, &mut k);
    k
}

/// The same 32x32 fingerprint from the mean of each block, in one pass over the pixels: about ten
/// times faster than `frame_key`'s CatmullRom resize and within a fraction of a level of it
/// (`frame_distance` between the two kinds of key is not meaningful: compare like with like).
pub fn fast_frame_key(img: &RgbImage) -> FrameKey {
    let (w, h) = (img.width() as usize, img.height() as usize);
    let mut k = [0u8; 3072];
    if w < 32 || h < 32 {
        return k;
    }
    let raw = img.as_raw();
    let mut sums = [0u32; 3072];
    for by in 0..32 {
        let (y0, y1) = (by * h / 32, (by + 1) * h / 32);
        for bx in 0..32 {
            let (x0, x1) = (bx * w / 32, (bx + 1) * w / 32);
            let s = &mut sums[(by * 32 + bx) * 3..(by * 32 + bx) * 3 + 3];
            for y in y0..y1 {
                for p in raw[(y * w + x0) * 3..(y * w + x1) * 3].as_chunks::<3>().0 {
                    s[0] += p[0] as u32;
                    s[1] += p[1] as u32;
                    s[2] += p[2] as u32;
                }
            }
            let n = ((y1 - y0) * (x1 - x0)) as u32;
            for c in 0..3 {
                k[(by * 32 + bx) * 3 + c] = ((s[c] + n / 2) / n) as u8;
            }
        }
    }
    k
}

pub fn frame_distance(a: &FrameKey, b: &FrameKey) -> f64 {
    let sum: i32 = a.iter().zip(b).map(|(x, y)| absd(*x as i32, *y as i32)).sum();
    sum as f64 / a.len() as f64
}

// ---- deep.go: panel delta ------------------------------------------------------------------------

/// Fraction of sampled (every 3rd) panel pixels whose summed channel delta exceeds 60.
pub fn panel_delta(a: &RgbImage, b: &RgbImage) -> f64 {
    let (mut n, mut diff) = (0u32, 0u32);
    let roi = shifted(PANEL_ROI, centred_dy(a));
    let mut y = roi.1;
    while y < roi.3 {
        let mut x = roi.0;
        while x < roi.2 {
            n += 1;
            let (ca, cb) = (px(a, x, y), px(b, x, y));
            if (0..3).map(|i| absd(ca[i] as i32, cb[i] as i32)).sum::<i32>() > 60 {
                diff += 1;
            }
            x += 3;
        }
        y += 3;
    }
    diff as f64 / n as f64
}

/// Gray image helper shared with `ink`.
pub(crate) fn white_gray(w: u32, h: u32) -> GrayImage {
    GrayImage::from_pixel(w, h, Luma([255]))
}

/// Turn an ink-isolated image (black glyphs on white) into a tight text line:
/// specks are removed and the page is cropped to the glyphs plus a margin, so
/// the recogniser sees one clean line and every timestep is either text or blank.
pub fn ink_to_line(g: &image::GrayImage) -> RgbImage {
    const MIN_AREA: usize = 10;
    const MARGIN: i64 = 8;
    let (w, h) = (g.width() as i64, g.height() as i64);
    let ink = |x: i64, y: i64| x >= 0 && y >= 0 && x < w && y < h && g.get_pixel(x as u32, y as u32)[0] < 128;
    let mut seen = vec![false; (w * h) as usize];
    let mut keep: Vec<(i64, i64)> = Vec::new();
    for y0 in 0..h {
        for x0 in 0..w {
            if !ink(x0, y0) || seen[(y0 * w + x0) as usize] {
                continue;
            }
            let (mut stack, mut comp) = (vec![(x0, y0)], Vec::new());
            seen[(y0 * w + x0) as usize] = true;
            while let Some((x, y)) = stack.pop() {
                comp.push((x, y));
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        let (nx, ny) = (x + dx, y + dy);
                        if ink(nx, ny) && !seen[(ny * w + nx) as usize] {
                            seen[(ny * w + nx) as usize] = true;
                            stack.push((nx, ny));
                        }
                    }
                }
            }
            if comp.len() >= MIN_AREA {
                keep.extend(comp);
            }
        }
    }
    if keep.is_empty() {
        return RgbImage::from_pixel(32, 48, image::Rgb([255, 255, 255]));
    }
    let (x0, x1) = (keep.iter().map(|p| p.0).min().unwrap(), keep.iter().map(|p| p.0).max().unwrap());
    let (y0, y1) = (keep.iter().map(|p| p.1).min().unwrap(), keep.iter().map(|p| p.1).max().unwrap());
    let mut out = RgbImage::from_pixel((x1 - x0 + 1 + 2 * MARGIN) as u32, (y1 - y0 + 1 + 2 * MARGIN) as u32, image::Rgb([255, 255, 255]));
    for (x, y) in keep {
        out.put_pixel((x - x0 + MARGIN) as u32, (y - y0 + MARGIN) as u32, image::Rgb([0, 0, 0]));
    }
    out
}

#[cfg(test)]
mod tests;
