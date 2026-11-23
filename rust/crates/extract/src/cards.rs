//! Student names on the cards of the 빠른 편성 grid.
//!
//! The name labels are found by colour (so one that free-text OCR misses is still found).
//! Each label's text is scored against every student name with the recogniser's own
//! probabilities, like the name on a profile. A clear winner is taken. When the winner is
//! only slightly ahead, it is taken only if the free-text OCR of the same label agrees
//! with it; when it does not, the free text can still name a student if exactly one
//! dictionary name is that close to it. Otherwise the card is left out: a wrong name would
//! put another student's icons on a portrait.
use crate::{best_with_margin, MIN_MARGIN, UNIQUE_NAMES};
use baocr_core::ocr_api::{Ocr, TextBox};
use baocr_names::edit_distance;
use baocr_ocr::ink_to_line;
use baocr_pixels::{self as px, CardLabel};
use image::RgbImage;

/// A student card in the grid.
#[derive(Debug, Clone, PartialEq)]
pub struct CardName {
    /// A dictionary name (a student's, exactly as written there).
    pub text: String,
    /// Horizontal centre of the card and the top of its name label, in the 1280x960 frame.
    pub x: f32,
    pub y0: f32,
}

/// Fewest ink pixels a label must show to be read at all.
const MIN_LABEL_INK: usize = 20;
/// A winner this far ahead is trusted only with the free text's agreement.
const LOW_MARGIN: f32 = 0.3;

fn letters(s: &str) -> Vec<char> {
    s.chars().filter(|c| c.is_alphanumeric()).collect()
}

/// The free-text OCR inside a label.
fn free_text(boxes: &[TextBox], l: &CardLabel) -> Vec<char> {
    let (x0, x1) = ((l.cx - 48) as f32, (l.cx + 48) as f32);
    let inside = boxes.iter().filter(|b| {
        let (cx, cy) = ((b.x0 + b.x1) / 2.0, (b.y0 + b.y1) / 2.0);
        cx >= x0 && cx <= x1 && cy >= l.y0 as f32 && cy <= l.y1 as f32
    });
    inside.flat_map(|b| letters(&b.text)).collect()
}

/// The dictionary name whose letters are exactly `text` (two or more of them).
fn exact_name(text: &[char]) -> Option<&'static str> {
    (text.len() >= 2).then(|| UNIQUE_NAMES.iter().find(|n| letters(n) == text).map(String::as_str)).flatten()
}

/// The one dictionary name within one edit of `text`, if there is exactly one.
fn unique_near(text: &[char]) -> Option<&'static str> {
    if text.is_empty() {
        return None;
    }
    let mut near = UNIQUE_NAMES.iter().filter(|n| edit_distance(&letters(n), text) <= 1);
    let first = near.next()?;
    near.next().is_none().then_some(first.as_str())
}

/// The cards whose name label is whole in the grid, with the name read on each.
/// `free` is the frame's free-text OCR (the caller has it already for the screen).
pub fn read_card_names(ocr: &dyn Ocr, img: &RgbImage, free: &[TextBox]) -> Vec<CardName> {
    px::quick_card_labels(img)
        .into_iter()
        .filter_map(|l| {
            let ink = px::quick_label_ink(img, &l);
            if ink.pixels().filter(|p| p[0] < 128).count() < MIN_LABEL_INK {
                return None;
            }
            let (_, scores) = ocr.choose(&ink_to_line(&ink), &UNIQUE_NAMES).ok()?;
            let (best, margin) = best_with_margin(&UNIQUE_NAMES, &scores)?;
            let side = free_text(free, &l);
            if std::env::var_os("BAOCR_DEBUG_CARDS").is_some() {
                eprintln!("card x={} y={} best={} margin={margin:.2} free={}", l.cx, l.y0, UNIQUE_NAMES[best], side.iter().collect::<String>());
            }
            let closed = (margin >= MIN_MARGIN || (margin >= LOW_MARGIN && edit_distance(&letters(&UNIQUE_NAMES[best]), &side) <= 1)).then(|| UNIQUE_NAMES[best].as_str());
            let name = match (closed, exact_name(&side)) {
                (Some(c), Some(e)) if c != e => {
                    // the two readings differ: the free text spells a whole name exactly, while the
                    // closed-set reader may have stopped short at a shorter name ("시로코" for "시로코*테러")
                    let (lc, le) = (letters(c), letters(e));
                    Some(if lc.len() > le.len() && lc.starts_with(&le) { c } else { e })
                }
                (Some(c), _) => Some(c),
                (None, Some(e)) => Some(e),
                (None, None) => unique_near(&side),
            };
            name.map(|n| CardName { text: n.to_string(), x: l.cx as f32, y0: l.y0 as f32 })
        })
        .collect()
}
