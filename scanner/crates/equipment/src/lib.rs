//! Port of cmd/baocr/equipment.go: equipment slots of the student detail screen.
//!
//! Pixel parts (`equip_badge`, `match_equipment`, ROI helpers) are OCR free and
//! testable on their own; `detect_equipment` adds the OCR reads through the
//! `Ocr` trait. Unknown or ambiguous items stay `None`; the catalog is never
//! used to guess.
use anyhow::{Context, Result};
use baocr_core::model::{Equipment, Int};
use baocr_core::ocr_api::Ocr;
use image::imageops::{self, FilterType};
use image::{GrayImage, ImageBuffer, Rgb, RgbImage, Rgba, RgbaImage};
use once_cell::sync::Lazy;
use regex::Regex;
use serde::Deserialize;
use std::collections::BTreeMap;

mod icons;

/// (x0, y0, x1, y1), end exclusive.
pub type Roi = (i32, i32, i32, i32);

#[derive(Debug, Clone, Deserialize)]
pub struct CatalogEntry {
    pub slot: i32,
    #[serde(rename = "type")]
    pub kind: String,
    pub tier: i32,
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Catalog {
    pub entries: Vec<CatalogEntry>,
}

static CATALOG_JSON: &str = include_str!("../data/equipment_catalog.json");
static CATALOG: Lazy<Catalog> =
    Lazy::new(|| serde_json::from_str(CATALOG_JSON).expect("embedded equipment catalog"));

/// The embedded equipment catalog.
pub fn catalog() -> &'static Catalog {
    &CATALOG
}

static TIER_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)[T7I1\[]\s*(10|[1-9])").unwrap());
static DIGITS_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\d+").unwrap());

// ---- ROI geometry (matches the Go constants) ----

/// Tier text ROI for standard slot index `i` (0..3).
pub fn tier_roi(i: i32) -> Roi {
    let x = 685 + 95 * i;
    (x, 737, x + 32, 757)
}
/// Level text ROIs for slot index `i`; Go tries three x shifts.
pub fn level_rois(i: i32) -> [Roi; 3] {
    let mk = |dx: i32| {
        let x = 683 + 95 * i + dx;
        (x, 675, x + 53, 693)
    };
    [mk(3), mk(6), mk(9)]
}
/// Unique-gear (slot 4) state text ROI.
pub const UNIQUE_STATE_ROI: Roi = (969, 706, 1046, 740);
/// Icon origins for slots 1..3.
pub const ICON_ORIGINS: [(i32, i32); 3] = [(685, 696), (780, 696), (875, 692)];

// ---- pixel helpers ----

fn px(img: &RgbImage, x: i32, y: i32) -> [i32; 3] {
    if x < 0 || y < 0 || x >= img.width() as i32 || y >= img.height() as i32 {
        return [0, 0, 0];
    }
    let p = img.get_pixel(x as u32, y as u32).0;
    [p[0] as i32, p[1] as i32, p[2] as i32]
}

/// Gold level badge of an equipped standard slot `i` (0..3).
pub fn equip_badge(img: &RgbImage, i: i32) -> bool {
    let x = 760 + 95 * i;
    let mut hits = 0;
    for y in 680..699 {
        for xx in x..x + 18 {
            let [r, g, b] = px(img, xx, y);
            if r > 145 && g > 105 && b < 125 && r > g {
                hits += 1;
            }
        }
    }
    hits >= 8
}

/// Slot 4 reads "EMPTY" when unequipped.
pub fn unique_state(text: &str) -> Option<bool> {
    text.to_uppercase().contains("EMPTY").then_some(false)
}

/// Parse a tier from OCR text ("T4", "[4", ...).
pub fn equipment_tier(text: &str) -> Option<Int> {
    let m = TIER_RE.captures(text.trim())?;
    m.get(1)?.as_str().parse().ok()
}

/// Parse a level from one OCR observation, as Go does: needs a l/u/v glyph and
/// a standalone two-digit token in 1..=100 (O read as 0, spaces dropped).
pub fn parse_level(text: &str) -> Option<Int> {
    let normalized = text.to_uppercase().replace('O', "0").replace(' ', "");
    if !text.to_lowercase().contains(['l', 'u', 'v']) {
        return None;
    }
    for m in DIGITS_RE.find_iter(&normalized) {
        let tok = m.as_str();
        if tok.len() != 2 {
            continue;
        }
        if let Ok(n) = tok.parse::<Int>() {
            if (1..=100).contains(&n) {
                return Some(n);
            }
        }
    }
    None
}

fn crop_clamped(img: &RgbImage, r: Roi) -> RgbImage {
    let x0 = r.0.max(0) as u32;
    let y0 = r.1.max(0) as u32;
    let x1 = (r.2.max(0) as u32).min(img.width());
    let y1 = (r.3.max(0) as u32).min(img.height());
    if x1 <= x0 || y1 <= y0 {
        return RgbImage::new(0, 0);
    }
    imageops::crop_imm(img, x0, y0, x1 - x0, y1 - y0).to_image()
}

fn upscale(img: &RgbImage, k: u32) -> RgbImage {
    imageops::resize(img, img.width() * k, img.height() * k, FilterType::CatmullRom)
}

fn load_icon(kind: &str, tier: i32) -> Result<RgbaImage> {
    let bytes = icons::bytes(kind, tier).with_context(|| format!("missing icon {kind}-T{tier}"))?;
    Ok(image::load_from_memory_with_format(bytes, image::ImageFormat::Png)?.to_rgba8())
}

/// Scale to `width` into premultiplied RGBA (Go scales into image.RGBA).
fn scale_icon(icon: &RgbaImage, width: u32) -> RgbaImage {
    let height = ((width as f64) * icon.height() as f64 / icon.width() as f64).round() as u32;
    let mut pre: RgbaImage = ImageBuffer::new(icon.width(), icon.height());
    for (x, y, p) in icon.enumerate_pixels() {
        let a = p[3] as u32;
        let m = |c: u8| ((c as u32 * a + 127) / 255) as u8;
        pre.put_pixel(x, y, Rgba([m(p[0]), m(p[1]), m(p[2]), p[3]]));
    }
    imageops::resize(&pre, width, height, FilterType::CatmullRom)
}

/// Best mean absolute RGB error of `icon` against the game at origin (x0,y0).
pub fn icon_error(game: &RgbImage, icon: &RgbaImage, x0: i32, y0: i32) -> f64 {
    let mut best = f64::INFINITY;
    for width in [65u32, 72, 80, 88] {
        let scaled = scale_icon(icon, width);
        let (w, h) = (scaled.width() as i32, scaled.height() as i32);
        let mut dx = -4;
        while dx <= 12 {
            let mut dy = -10;
            while dy <= 14 {
                let (mut sum, mut count) = (0i64, 0i64);
                let mut y = 0;
                while y < h {
                    let mut x = 0;
                    while x < w {
                        let c = scaled.get_pixel(x as u32, y as u32).0;
                        if c[3] >= 175 {
                            let g = px(game, x0 + dx + x, y0 + dy + y);
                            sum += (c[0] as i64 - g[0] as i64).abs()
                                + (c[1] as i64 - g[1] as i64).abs()
                                + (c[2] as i64 - g[2] as i64).abs();
                            count += 3;
                        }
                        x += 2;
                    }
                    y += 2;
                }
                if count > 100 {
                    let score = sum as f64 / count as f64;
                    if score < best {
                        best = score;
                    }
                }
                dy += 2;
            }
            dx += 2;
        }
    }
    best
}

#[derive(Debug, Clone, PartialEq)]
pub struct ItemMatch {
    pub kind: String,
    pub name: String,
    pub tier: Int,
    pub error: f64,
}

/// Match slot `slot` (1..=3) against the catalog templates; `None` unless the
/// best score is <= 40 and beats the runner-up by >= 12.
pub fn match_equipment(img: &RgbImage, slot: i32, tier: Option<Int>) -> Result<Option<ItemMatch>> {
    if !(1..=3).contains(&slot) {
        return Ok(None);
    }
    let (mut best, mut second) = (f64::INFINITY, f64::INFINITY);
    let mut winner: Option<&CatalogEntry> = None;
    let at = ICON_ORIGINS[(slot - 1) as usize];
    for e in &catalog().entries {
        if e.slot != slot || tier.is_some_and(|t| e.tier != t) {
            continue;
        }
        let icon = load_icon(&e.kind, e.tier)?;
        let score = icon_error(img, &icon, at.0, at.1);
        if score < best {
            second = best;
            best = score;
            winner = Some(e);
        } else if score < second {
            second = score;
        }
    }
    match winner {
        Some(w) if !(best > 40.0 || second - best < 12.0) => Ok(Some(ItemMatch {
            kind: w.kind.clone(),
            name: w.name.clone(),
            tier: w.tier,
            error: best,
        })),
        _ => Ok(None),
    }
}

fn threshold_rgb(img: &RgbImage, t: u8) -> RgbImage {
    let gray: GrayImage = imageops::grayscale(img);
    let mut out = RgbImage::new(gray.width(), gray.height());
    for (x, y, p) in gray.enumerate_pixels() {
        let v = if p[0] > t { 255 } else { 0 };
        out.put_pixel(x, y, Rgb([v, v, v]));
    }
    out
}

fn equipment_level(img: &RgbImage, ocr: &dyn Ocr, i: i32) -> Result<(Option<Int>, String)> {
    let mut obs: Vec<String> = Vec::new();
    for roi in level_rois(i) {
        let big = upscale(&crop_clamped(img, roi), 4);
        if big.width() == 0 {
            continue;
        }
        let variants = [big.clone(), threshold_rgb(&big, 210), threshold_rgb(&big, 225)];
        for v in &variants {
            let (text, _) = ocr.line(v)?;
            obs.push(text.clone());
            if let Some(n) = parse_level(&text) {
                return Ok((Some(n), obs.join(" | ")));
            }
        }
    }
    Ok((None, obs.join(" | ")))
}

fn read_line(img: &RgbImage, ocr: &dyn Ocr, roi: Roi) -> Result<String> {
    let c = crop_clamped(img, roi);
    if c.width() == 0 {
        return Ok(String::new());
    }
    Ok(ocr.line(&upscale(&c, 4))?.0)
}

fn blank(slot: i32, kind: &str) -> Equipment {
    Equipment {
        slot,
        slot_kind: kind.into(),
        equipped: None,
        kind: None,
        item_name: None,
        tier: None,
        level: None,
        icon_match_error: None,
    }
}

/// The four equipment slots (slot 4 = unique gear) plus the raw OCR strings.
pub fn detect_equipment(
    img: &RgbImage,
    ocr: &dyn Ocr,
) -> Result<(Vec<Equipment>, BTreeMap<String, String>)> {
    let mut raw = BTreeMap::new();
    let mut result = Vec::with_capacity(4);
    for i in 0..3 {
        let mut e = blank(i + 1, "standard");
        if equip_badge(img, i) {
            e.equipped = Some(true);
        }
        let tier_text = read_line(img, ocr, tier_roi(i))?;
        e.tier = equipment_tier(&tier_text);
        raw.insert(format!("equipment_{}_tier", i + 1), tier_text);
        if e.equipped.is_some() {
            let (lvl, obs) = equipment_level(img, ocr, i)?;
            e.level = lvl;
            raw.insert(format!("equipment_{}_level", i + 1), obs);
            if let Some(m) = match_equipment(img, i + 1, e.tier)? {
                e.kind = Some(m.kind);
                e.item_name = Some(m.name);
                e.icon_match_error = Some(m.error);
                if e.tier.is_none() {
                    e.tier = Some(m.tier);
                }
            }
        }
        result.push(e);
    }
    let text = read_line(img, ocr, UNIQUE_STATE_ROI)?;
    let mut extra = blank(4, "unique_gear");
    extra.equipped = unique_state(&text);
    raw.insert("equipment_4_state".into(), text);
    result.push(extra);
    Ok((result, raw))
}

#[cfg(test)]
mod tests;
