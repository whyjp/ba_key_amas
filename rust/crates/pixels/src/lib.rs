//! Pixel-only readers for the 1280x960 (4:3) Blue Archive profile / roster frame.
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
use image::{imageops, GrayImage, Luma, RgbImage};
use once_cell::sync::Lazy;

mod ink;
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
    fraction(img, DETAIL_BAND_ROI, |[r, g, b]| r < 75 && g > 50 && g < 105 && b > 85 && b < 150) > 0.6
}

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

/// Scrollbar thumb on column x=1230; `{-1,-1}` when absent.
pub fn scroll_thumb(img: &RgbImage) -> Thumb {
    let mut best = Thumb { top: -1, bottom: -1 };
    let mut start = -1;
    for y in 175..=945 {
        let [r, g, b] = px(img, SCROLL_COLUMN_X, y);
        let hit = r < 180 && g < 200 && b < 210;
        if hit && start < 0 {
            start = y;
        }
        if (!hit || y == 945) && start >= 0 {
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
/// Empty unless the frame is exactly 1280x960.
pub fn roster_cards(img: &RgbImage) -> Vec<Card> {
    if img.width() != FRAME_W || img.height() != FRAME_H {
        return Vec::new();
    }
    struct Group {
        start: i32,
        end: i32,
        count: i32,
    }
    let mut groups: Vec<Group> = Vec::new();
    for y in 170..=930 {
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
        if g.count < 60 || g.end > 920 || g.end < 290 || g.end - g.start < 90 {
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

pub fn same_cards(a: &[Card], b: &[Card]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| signature_distance(&x.signature, &y.signature) <= 2.0)
}

/// Pixel part of Go `rosterVisible`: not a profile, scrollbar thumb sized like
/// the roster's, six-card grid present, and the green-tinted All tab at (395,110).
pub fn roster_visible_pixels(img: &RgbImage) -> bool {
    if detail_visible(img) {
        return false;
    }
    let t = scroll_thumb(img);
    if t.top < 179 || t.bottom - t.top < 45 || t.bottom - t.top > 90 || t.bottom > 945 || roster_cards(img).len() < 6 {
        return false;
    }
    let [r, g, b] = px(img, 395, 110);
    g as i32 > r as i32 + 24 && b as i32 > g as i32 + 20
}

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

pub fn frame_distance(a: &FrameKey, b: &FrameKey) -> f64 {
    let sum: i32 = a.iter().zip(b).map(|(x, y)| absd(*x as i32, *y as i32)).sum();
    sum as f64 / a.len() as f64
}

// ---- deep.go: panel delta ------------------------------------------------------------------------

/// Fraction of sampled (every 3rd) panel pixels whose summed channel delta exceeds 60.
pub fn panel_delta(a: &RgbImage, b: &RgbImage) -> f64 {
    let (mut n, mut diff) = (0u32, 0u32);
    let mut y = PANEL_ROI.1;
    while y < PANEL_ROI.3 {
        let mut x = PANEL_ROI.0;
        while x < PANEL_ROI.2 {
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

#[cfg(test)]
mod tests;
