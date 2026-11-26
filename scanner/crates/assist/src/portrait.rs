//! Telling a student by the picture on their portrait, for the squad strip of the 빠른 편성 popup, whose slots
//! carry no name.
//!
//! The reference is the centre of each committed portrait asset (`web/public/portraits`, 200x226), shrunk to a
//! 24x24 colour template and shipped with the add-on (`data/portrait_templates.bin`, made by the `portrait_seed`
//! example). The top and bottom of a portrait are covered by the level badge and the small icons in the game, so only
//! the middle is used. The game draws the same picture a little differently in every place, so the game side tries a
//! few sizes and shifts and keeps the best normalised correlation per student.
use image::{imageops::FilterType, DynamicImage, RgbImage, RgbaImage};
use baocr_extract::scene::Layout;
use std::sync::OnceLock;

/// Right edges of the six slots of the squad strip at the bottom of the 빠른 편성 popup (4:3 frame pixels): four
/// strikers, then two specials.
pub const SLOT_RIGHT_EDGES: [i32; 6] = [111, 205, 300, 392, 490, 579];
/// The popup keeps its size and the order of its parts in 16:9 but is squeezed vertically: everything inside it rises by this.
pub const POPUP_RISE_169: i32 = 120;

/// Centre of a slot's portrait: 44 px left of its right edge, on the strip's centre line.
pub fn slot_centre(slot: usize, layout: Layout) -> (f32, f32) {
    let rise = match layout {
        Layout::Ratio43 => 0,
        Layout::Ratio169 => POPUP_RISE_169,
    };
    ((SLOT_RIGHT_EDGES[slot] - 44) as f32, (716 - rise) as f32)
}

/// Whether the slot holds no student: the game then draws a flat grey plate with the word EMPTY. Its middle is grey (little
/// colour) and nearly even (a portrait always has the contrast of a face and hair), which the correlation cannot tell:
/// a flat patch matches some template as well as a face does, so without this the empty slot was given a student.
pub fn slot_is_empty(img: &RgbImage, cx: f32, cy: f32) -> bool {
    const HALF: i64 = 22;
    let (x0, y0) = (cx.round() as i64 - HALF, cy.round() as i64 - HALF);
    if x0 < 0 || y0 < 0 || x0 + 2 * HALF > img.width() as i64 || y0 + 2 * HALF > img.height() as i64 {
        return false;
    }
    let (mut spread, mut sum, mut sum2, mut n) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
    for y in y0..y0 + 2 * HALF {
        for x in x0..x0 + 2 * HALF {
            let p = img.get_pixel(x as u32, y as u32).0;
            let (hi, lo) = (p.iter().copied().max().unwrap_or(0) as f64, p.iter().copied().min().unwrap_or(0) as f64);
            let lum = (p[0] as f64 + p[1] as f64 + p[2] as f64) / 3.0;
            spread += hi - lo;
            sum += lum;
            sum2 += lum * lum;
            n += 1.0;
        }
    }
    let mean = sum / n;
    let std = (sum2 / n - mean * mean).max(0.0).sqrt();
    // measured: the empty plate is 8 / 12 (colour spread / brightness spread), a portrait at least 12.6 / 23.6
    spread / n < 14.0 && std < 18.0
}

/// A coarse copy of the picture around (`cx`, `cy`): every fourth pixel of the middle 44x44 (11x11 colour values). Two looks at one
/// slot of one screen differ by a pixel level or two; another student, or an empty slot, by tens.
pub fn patch_sample(img: &RgbImage, cx: f32, cy: f32) -> Vec<u8> {
    let (x0, y0) = (cx.round() as i64 - 22, cy.round() as i64 - 22);
    let mut out = Vec::with_capacity(11 * 11 * 3);
    for gy in 0..11 {
        for gx in 0..11 {
            let (px, py) = ((x0 + gx * 4).clamp(0, img.width() as i64 - 1), (y0 + gy * 4).clamp(0, img.height() as i64 - 1));
            out.extend_from_slice(&img.get_pixel(px as u32, py as u32).0);
        }
    }
    out
}

/// Mean difference of two samples from [`patch_sample`] (0 to 255).
pub fn sample_distance(a: &[u8], b: &[u8]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return f32::MAX;
    }
    a.iter().zip(b).map(|(x, y)| (*x as i32 - *y as i32).unsigned_abs() as u64).sum::<u64>() as f32 / a.len() as f32
}

/// Side of a template, in pixels.
pub const SIDE: u32 = 24;
const LEN: usize = (SIDE * SIDE * 3) as usize;
/// The part of a portrait asset that is used: (left, top, right, bottom) as shares of its width and height.
/// The face lies in the upper middle; the corners and the rim belong to other icons in the game.
pub const ASSET_CROP: (f32, f32, f32, f32) = (0.30, 0.25, 0.75, 0.65);
/// Sizes (side of the square cut from the frame) and shifts tried on the game side, in frame pixels.
const SIZES: [f32; 3] = [44.0, 52.0, 60.0];
const SHIFTS: [f32; 5] = [-6.0, -3.0, 0.0, 3.0, 6.0];

const MAGIC: &[u8; 5] = b"BAPT1";
static BUNDLED: &[u8] = include_bytes!("../data/portrait_templates.bin");

/// The shrunk centre of one portrait asset as raw pixels (transparent areas are grey), ready to be stored.
pub fn asset_template(asset: &DynamicImage) -> Vec<u8> {
    let rgba: RgbaImage = asset.to_rgba8();
    let mut flat = RgbImage::new(rgba.width(), rgba.height());
    for (x, y, p) in rgba.enumerate_pixels() {
        let a = p[3] as f32 / 255.0;
        let mix = |c: u8| (c as f32 * a + 128.0 * (1.0 - a)).round() as u8;
        flat.put_pixel(x, y, image::Rgb([mix(p[0]), mix(p[1]), mix(p[2])]));
    }
    let (w, h) = (flat.width() as f32, flat.height() as f32);
    let (l, t, r, b) = ASSET_CROP;
    let (x0, y0) = ((w * l) as u32, (h * t) as u32);
    let (cw, ch) = (((w * r) as u32).saturating_sub(x0).max(1), ((h * b) as u32).saturating_sub(y0).max(1));
    let part = image::imageops::crop_imm(&flat, x0, y0, cw, ch).to_image();
    image::imageops::resize(&part, SIDE, SIDE, FilterType::Triangle).into_raw()
}

/// Remove each colour channel's mean and scale to unit length: what is compared is the picture, not its brightness.
fn normalised(pixels: &[u8]) -> Vec<f32> {
    let n = pixels.len() / 3;
    let mut mean = [0.0f32; 3];
    for px in pixels.as_chunks::<3>().0 {
        for c in 0..3 {
            mean[c] += px[c] as f32;
        }
    }
    for m in &mut mean {
        *m /= n as f32;
    }
    let mut out: Vec<f32> = pixels.as_chunks::<3>().0.iter().flat_map(|px| (0..3).map(move |c| px[c] as f32 - mean[c])).collect::<Vec<_>>();
    let len = out.iter().map(|v| v * v).sum::<f32>().sqrt().max(1e-6);
    for v in &mut out {
        *v /= len;
    }
    out
}

/// Candidate templates of the game's picture centred on (`cx`, `cy`) in the frame: every size at every shift.
pub fn probe(img: &RgbImage, cx: f32, cy: f32) -> Vec<Vec<f32>> {
    let mut out = Vec::with_capacity(SIZES.len() * SHIFTS.len() * SHIFTS.len());
    for size in SIZES {
        for dy in SHIFTS {
            for dx in SHIFTS {
                let (x0, y0) = ((cx + dx - size / 2.0).round() as i64, (cy + dy - size / 2.0).round() as i64);
                if x0 < 0 || y0 < 0 || x0 + size as i64 > img.width() as i64 || y0 + size as i64 > img.height() as i64 {
                    continue;
                }
                let part = image::imageops::crop_imm(img, x0 as u32, y0 as u32, size as u32, size as u32).to_image();
                out.push(normalised(image::imageops::resize(&part, SIDE, SIDE, FilterType::Triangle).as_raw()));
            }
        }
    }
    out
}

/// The best student for a picture and how clearly it beats the next one.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Match {
    pub id: i64,
    /// Normalised correlation of the best size and shift, -1 to 1.
    pub score: f32,
    /// The best score minus the best score of any other allowed student.
    pub margin: f32,
    /// The student with the best score among those who are not the same student as `id` (None when there is no other).
    pub runner_up: Option<i64>,
}

/// What a match must reach to be believed (measured on the recorded screens, see `tests/portrait.rs`).
pub const MIN_SCORE: f32 = 0.55;
pub const MIN_MARGIN: f32 = 0.10;

/// A clear picture (this score) is believed with a smaller lead: over another student, `LOOSE_MARGIN`; over another
/// costume of the same character, which looks alike by nature, `KIN_MARGIN`.
pub const STRONG_SCORE: f32 = 0.65;
pub const LOOSE_MARGIN: f32 = 0.0;
pub const KIN_MARGIN: f32 = 0.05;

/// The character a student name stands for, without the costume: "아루(드레스)" and "시로코*테러" are "아루" and "시로코".
pub fn base_name(name: &str) -> &str {
    name.split(['(', '*']).next().unwrap_or(name).trim()
}

impl Match {
    /// Believed on score and lead alone (the safe rule).
    pub fn trusted(&self) -> bool {
        self.score >= MIN_SCORE && self.margin >= MIN_MARGIN
    }

    /// [`Match::trusted`], or a clear picture with a small lead. `kin(a, b)` says two students are costumes of one character.
    pub fn trusted_with(&self, kin: &dyn Fn(i64, i64) -> bool) -> bool {
        if self.trusted() {
            return true;
        }
        let near_kin = self.runner_up.is_some_and(|r| kin(self.id, r));
        self.score >= STRONG_SCORE && self.margin >= if near_kin { KIN_MARGIN } else { LOOSE_MARGIN }
    }
}

pub struct Book {
    ids: Vec<i64>,
    /// `ids.len()` normalised templates, one after the other.
    rows: Vec<f32>,
}

impl Book {
    /// The templates shipped with the add-on.
    pub fn bundled() -> &'static Book {
        static BOOK: OnceLock<Book> = OnceLock::new();
        BOOK.get_or_init(|| Book::parse(BUNDLED).expect("data/portrait_templates.bin is valid"))
    }

    pub fn parse(bytes: &[u8]) -> Option<Book> {
        let head = bytes.get(..5 + 4 + 4)?;
        if &head[..5] != MAGIC || u32::from_le_bytes(head[9..13].try_into().ok()?) != SIDE {
            return None;
        }
        let count = u32::from_le_bytes(head[5..9].try_into().ok()?) as usize;
        let mut at = 13;
        let (mut ids, mut rows) = (Vec::with_capacity(count), Vec::with_capacity(count * LEN));
        for _ in 0..count {
            ids.push(i64::from_le_bytes(bytes.get(at..at + 8)?.try_into().ok()?));
            rows.extend(normalised(bytes.get(at + 8..at + 8 + LEN)?));
            at += 8 + LEN;
        }
        Some(Book { ids, rows })
    }

    pub fn len(&self) -> usize {
        self.ids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    /// Serialise templates (`id`, raw pixels) in the shipped format.
    pub fn encode(entries: &[(i64, Vec<u8>)]) -> Vec<u8> {
        let mut out = Vec::with_capacity(13 + entries.len() * (8 + LEN));
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&(entries.len() as u32).to_le_bytes());
        out.extend_from_slice(&SIDE.to_le_bytes());
        for (id, px) in entries {
            out.extend_from_slice(&id.to_le_bytes());
            out.extend_from_slice(px);
        }
        out
    }

    /// The best allowed student for the candidate templates of one picture, or None when nobody is allowed.
    /// `same(a, b)` says two ids are one student for the margin (a name with two modes has two ids and one picture):
    /// the margin is taken against the best score of anyone who is not.
    pub fn identify(&self, patches: &[Vec<f32>], allowed: &dyn Fn(i64) -> bool, same: &dyn Fn(i64, i64) -> bool) -> Option<Match> {
        let mut scores: Vec<(f32, i64)> = Vec::new();
        for (row, id) in self.rows.as_chunks::<LEN>().0.iter().zip(&self.ids) {
            if !allowed(*id) {
                continue;
            }
            let best = patches.iter().map(|p| p.iter().zip(row).map(|(a, b)| a * b).sum::<f32>()).fold(f32::NEG_INFINITY, f32::max);
            scores.push((best, *id));
        }
        scores.sort_by(|a, b| b.0.total_cmp(&a.0));
        let first = scores.first()?;
        let other = scores.iter().find(|(_, id)| !same(first.1, *id));
        Some(Match { id: first.1, score: first.0, margin: other.map_or(1.0, |o| first.0 - o.0), runner_up: other.map(|o| o.1) })
    }
}
