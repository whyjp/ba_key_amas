//! Student names on the cards of the 빠른 편성 grid.
//!
//! Three kinds of evidence are combined, because each fails differently:
//!
//! - **Width.** The name is drawn in a fixed face, so the ink's width grows by a fixed step
//!   per letter (a one-letter name is 51 px wide, two letters 58, three 64, four 71, and a
//!   parenthesised outfit adds a little per bracket). That settles how many letters a name
//!   has before a single stroke is read, and it removes the mix-ups the reader cannot see:
//!   "토키" against "토키(바니걸)", or the one-letter "슌" against two-letter names.
//! - **The closed-set reading**: the line is scored against every student name with the
//!   recogniser's own probabilities, like the name on a profile.
//! - **The free-text OCR** of the same label.
//!
//! `decide` is pure: it takes that evidence and names the student or nobody. A wrong name
//! would put another student's icons on a portrait, so doubt gives nobody.
use crate::{MIN_MARGIN, UNIQUE_NAMES};
use baocr_core::ocr_api::{Ocr, TextBox};
use baocr_names::{edit_distance, to_jamo};
use baocr_pixels::ink_to_line;
use baocr_pixels::{self as px, CardLabel};
use image::{GrayImage, RgbImage};
use std::collections::HashMap;

/// A student card in the grid.
#[derive(Debug, Clone, PartialEq)]
pub struct CardName {
    /// A dictionary name (a student's, exactly as written there).
    pub text: String,
    /// Horizontal centre of the card and the top of its name label, in the 1280x960 frame.
    pub x: f32,
    pub y0: f32,
    /// The grid shows pictures only (no name labels): `y0` is then the top of the card, which is the portrait
    /// alone, and the student was told by the picture.
    pub bare: bool,
}

/// Fewest ink pixels a label must show to be read at all.
const MIN_LABEL_INK: usize = 20;
/// A winner this far ahead is trusted only with the free text's agreement.
const LOW_MARGIN: f32 = 0.3;
/// A lead this big is believed even when the free text disagrees; a smaller lead against a
/// free text that names something else is doubt (a blurred capture produces such leads).
const STRONG_MARGIN: f32 = 4.0;

// Ink width in the 1280x960 frame as a function of what is written, measured on real
// frames (1 letter 51, 2 letters 57-58, 3 letters 64, 4 letters 71, 리오(무장) 76,
// 호시노(무장) 83): a base plus a step per letter, brackets and the star counting less.
const WIDTH_BASE: f32 = 44.5;
const WIDTH_PER_LETTER: f32 = 6.7;
const BRACKET_WEIGHT: f32 = 0.35;
const STAR_WEIGHT: f32 = 0.5;
/// How far a measured width may be from the expected one (a letter is 6.7).
const WIDTH_TOLERANCE: f32 = 3.5;
/// The label is only so wide: names expected beyond this look alike (all measure 82-85).
const WIDTH_SATURATED_FROM: f32 = 83.0;
const WIDTH_SATURATED_MIN: f32 = 80.0;
/// With one or two candidates left, a candidate this far below the best score of all names is
/// not believed (the label may name a student the dictionary does not have) unless the free
/// text agrees.
const THIN_FIELD_GAP: f32 = 6.0;

fn letters(s: &str) -> Vec<char> {
    s.chars().filter(|c| c.is_alphanumeric()).collect()
}

/// Does what was read stand for this name? One letter may be wrong, but only by a stroke or
/// two (the jamo): "순" is "슌" misread, "열" is another letter. Without this a one-letter
/// name would agree with every letter there is.
fn close(name: &str, read: &[char]) -> bool {
    let a = letters(name);
    if read.is_empty() || edit_distance(&a, read) > 1 {
        return false;
    }
    let (ja, jb) = (to_jamo(&a.iter().collect::<String>()), to_jamo(&read.iter().collect::<String>()));
    edit_distance(&ja, &jb) <= 2
}

/// What a name weighs in letters of width.
fn weight(name: &str) -> f32 {
    name.chars()
        .map(|c| match c {
            '(' | ')' => BRACKET_WEIGHT,
            '*' => STAR_WEIGHT,
            c if ('\u{AC00}'..='\u{D7A3}').contains(&c) => 1.0,
            c if c.is_ascii_alphanumeric() => 0.6,
            _ => 0.0,
        })
        .sum()
}

pub(crate) fn expected_width(name: &str) -> f32 {
    WIDTH_BASE + WIDTH_PER_LETTER * weight(name)
}

/// Can a name of this kind be drawn as ink `measured` pixels wide?
pub(crate) fn width_fits(measured: f32, name: &str) -> bool {
    let expected = expected_width(name);
    if expected >= WIDTH_SATURATED_FROM {
        measured >= WIDTH_SATURATED_MIN
    } else {
        (measured - expected).abs() <= WIDTH_TOLERANCE
    }
}

/// What was seen on one label.
pub(crate) struct Evidence<'a> {
    pub names: &'a [String],
    /// Closed-set score of every name (higher is better; NEG_INFINITY for one the model cannot spell).
    pub scores: &'a [f32],
    /// Width of the ink, in pixels.
    pub width: f32,
    /// The letters free-text OCR read inside the label.
    pub free: &'a [char],
}

/// The best and the runner-up among the allowed names that are different names.
fn best_with_margin(names: &[String], scores: &[f32], allowed: &[bool]) -> Option<(usize, f32)> {
    let best = (0..names.len()).filter(|&i| allowed[i] && scores[i].is_finite()).max_by(|&a, &b| scores[a].total_cmp(&scores[b]))?;
    let runner = (0..names.len()).filter(|&i| allowed[i] && scores[i].is_finite() && names[i] != names[best]).map(|i| scores[i]).fold(f32::NEG_INFINITY, f32::max);
    Some((best, scores[best] - runner))
}

/// The student this evidence names, as an index into `names`.
pub(crate) fn decide(e: &Evidence) -> Option<usize> {
    // only names that can be this wide are candidates at all
    let allowed: Vec<bool> = e.names.iter().map(|n| width_fits(e.width, n)).collect();
    let in_play: Vec<usize> = (0..e.names.len()).filter(|&i| allowed[i]).collect();
    if in_play.is_empty() {
        return None;
    }
    let spelled = |i: usize| edit_distance(&letters(&e.names[i]), e.free) == 0;
    let agrees = |i: usize| close(&e.names[i], e.free);
    // the free text spells one candidate exactly / within one letter of exactly one
    let exact = in_play.iter().copied().find(|&i| e.free.len() >= 2 && spelled(i));
    let near = {
        let found: Vec<usize> = in_play.iter().copied().filter(|&i| agrees(i)).collect();
        // one name, or several that are the same name written the same way
        (!found.is_empty() && found.iter().all(|&i| e.names[i] == e.names[found[0]])).then(|| found[0])
    };
    let overall = e.scores.iter().copied().filter(|s| s.is_finite()).fold(f32::NEG_INFINITY, f32::max);
    let closed = best_with_margin(e.names, e.scores, &allowed).and_then(|(best, margin)| {
        let ahead = margin >= STRONG_MARGIN || (margin >= MIN_MARGIN && (e.free.is_empty() || agrees(best))) || (margin >= LOW_MARGIN && agrees(best));
        let believable = in_play.len() > 2 || e.scores[best] >= overall - THIN_FIELD_GAP || agrees(best);
        (ahead && believable).then_some(best)
    });
    match (closed, exact) {
        (Some(c), Some(x)) if e.names[c] != e.names[x] => {
            // the reader may have stopped short at a shorter name ("시로코" for "시로코*테러")
            let (lc, lx) = (letters(&e.names[c]), letters(&e.names[x]));
            Some(if lc.len() > lx.len() && lc.starts_with(&lx) { c } else { x })
        }
        (Some(c), _) => Some(c),
        (None, Some(x)) => Some(x),
        (None, None) => near,
    }
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

/// Width of the ink (first to last column with any).
fn ink_width(ink: &GrayImage) -> f32 {
    let cols: Vec<u32> = ink.enumerate_pixels().filter(|(_, _, p)| p[0] < 128).map(|(x, _, _)| x).collect();
    match (cols.iter().min(), cols.iter().max()) {
        (Some(a), Some(b)) => (b - a + 1) as f32,
        _ => 0.0,
    }
}

/// What a label's ink (the bytes of its thresholded picture, which a given label has in every frame it is whole in) was read as:
/// a card that stays on screen through a scroll is not read again. Only names that were decided are kept.
pub type LabelCache = HashMap<u64, String>;

fn ink_print(ink: &GrayImage) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    ink.as_raw().hash(&mut h);
    h.finish()
}

/// The part of the frame (x, y, width, height) that holds the labels that are not in `cache`, the only part whose text the
/// readers of those labels need; None when every label is known (nothing needs reading).
pub fn unread_label_band(img: &RgbImage, cache: &LabelCache) -> Option<(u32, u32, u32, u32)> {
    let unknown: Vec<CardLabel> = px::quick_card_labels(img)
        .into_iter()
        .filter(|l| {
            let ink = px::quick_label_ink(img, l);
            ink.pixels().filter(|p| p[0] < 128).count() >= MIN_LABEL_INK && !cache.contains_key(&ink_print(&ink))
        })
        .collect();
    if unknown.is_empty() {
        return None;
    }
    let (x0, x1) = (unknown.iter().map(|l| l.cx).min()? - 56, unknown.iter().map(|l| l.cx).max()? + 56);
    let (y0, y1) = (unknown.iter().map(|l| l.y0).min()? - 10, unknown.iter().map(|l| l.y1).max()? + 10);
    let (x0, y0) = (x0.max(0), y0.max(0));
    let (x1, y1) = (x1.min(img.width() as i32), y1.min(img.height() as i32));
    (x1 > x0 && y1 > y0).then(|| (x0 as u32, y0 as u32, (x1 - x0) as u32, (y1 - y0) as u32))
}

/// The cards whose name label is whole in the grid, with the name read on each.
/// `free` is the frame's free-text OCR (the caller has it already for the screen).
pub fn read_card_names(ocr: &dyn Ocr, img: &RgbImage, free: &[TextBox]) -> Vec<CardName> {
    read_card_names_cached(ocr, img, free, &mut LabelCache::new())
}

/// [`read_card_names`] that does not read a label again whose ink is in `cache`, and adds the ones it reads. `free` then only
/// needs to cover the labels that are not cached (see [`unread_label_band`]).
pub fn read_card_names_cached(ocr: &dyn Ocr, img: &RgbImage, free: &[TextBox], cache: &mut LabelCache) -> Vec<CardName> {
    px::quick_card_labels(img)
        .into_iter()
        .filter_map(|l| {
            let ink = px::quick_label_ink(img, &l);
            if ink.pixels().filter(|p| p[0] < 128).count() < MIN_LABEL_INK {
                return None;
            }
            let print = ink_print(&ink);
            if let Some(name) = cache.get(&print) {
                return Some(CardName { text: name.clone(), x: l.cx as f32, y0: l.y0 as f32, bare: false });
            }
            let (_, scores) = ocr.choose(&ink_to_line(&ink), &UNIQUE_NAMES).ok()?;
            let side = free_text(free, &l);
            let evidence = Evidence { names: &UNIQUE_NAMES, scores: &scores, width: ink_width(&ink), free: &side };
            if std::env::var_os("BAOCR_DEBUG_CARDS").is_some() {
                let allowed: Vec<bool> = UNIQUE_NAMES.iter().map(|n| width_fits(evidence.width, n)).collect();
                let mut top: Vec<usize> = (0..scores.len()).filter(|&i| scores[i].is_finite()).collect();
                top.sort_by(|&a, &b| scores[b].total_cmp(&scores[a]));
                let show = |i: usize| format!("{}{}:{:.1}", UNIQUE_NAMES[i], if allowed[i] { "" } else { "(x)" }, scores[i]);
                eprintln!("card x={} y={} width={} fit={} free={} top={:?}", l.cx, l.y0, evidence.width, allowed.iter().filter(|a| **a).count(), side.iter().collect::<String>(), top.iter().take(4).map(|&i| show(i)).collect::<Vec<_>>());
            }
            let name = decide(&evidence).map(|i| UNIQUE_NAMES[i].clone())?;
            cache.insert(print, name.clone());
            Some(CardName { text: name, x: l.cx as f32, y0: l.y0 as f32, bare: false })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    fn pick<'a>(names: &'a [String], scores: &[f32], width: f32, free: &str) -> Option<&'a str> {
        let free: Vec<char> = free.chars().collect();
        decide(&Evidence { names, scores, width, free: &free }).map(|i| names[i].as_str())
    }

    #[test]
    fn width_follows_the_letter_count_and_brackets_weigh_little() {
        // measured on real frames
        for (name, measured) in [("슌", 51.0), ("케이", 57.0), ("히나", 58.0), ("아리스", 64.0), ("사쿠라코", 71.0), ("리오(무장)", 76.0), ("호시노(무장)", 83.0), ("시로코*테러", 81.0), ("토키(바니걸)", 82.0)] {
            assert!(width_fits(measured, name), "{name} at {measured}: expected {:.1}", expected_width(name));
        }
        assert!(!width_fits(82.0, "토키") && !width_fits(58.0, "토키(바니걸)"), "a short name is not a long one");
        assert!(!width_fits(51.0, "케이") && !width_fits(58.0, "아리스"), "one letter is a step of 6.7 px");
        // very long names all fill the label
        assert!(width_fits(84.0, "하나코(수영복)") && width_fits(85.0, "마리나(치파오)") && !width_fits(76.0, "마리나(치파오)"));
    }

    #[test]
    fn every_dictionary_name_has_a_distinct_enough_width_class() {
        // one-letter names are told from two-letter ones, and two from three, by width alone
        let one: Vec<&String> = UNIQUE_NAMES.iter().filter(|n| letters(n).len() == 1).collect();
        assert_eq!(one.iter().map(|n| n.as_str()).collect::<Vec<_>>(), vec!["슌"], "the only one-letter student");
        for n in UNIQUE_NAMES.iter().filter(|n| letters(n).len() == 2 && !n.contains('(')) {
            assert!(!width_fits(51.0, n), "{n} is not as narrow as 슌");
            assert!(width_fits(57.5, n), "{n} measures about 57-58");
        }
    }

    // Scores below are the closed-set scores read off real frames (BAOCR_DEBUG_CARDS=1).
    #[test]
    fn a_one_letter_name_is_found_though_the_reader_prefers_a_two_letter_one() {
        let n = names(&["우이", "슌", "안나", "주리"]);
        // reader: 우이 -14.1, 슌 -14.7; free text read "순"; ink 51 px wide
        assert_eq!(pick(&n, &[-14.1, -14.7, -14.8, -15.4], 51.0, "순"), Some("슌"));
        // and without any free text at all
        assert_eq!(pick(&n, &[-14.1, -14.7, -14.8, -15.4], 51.0, ""), Some("슌"));
    }

    #[test]
    fn a_name_the_dictionary_lacks_is_not_forced_into_the_only_candidate() {
        let n = names(&["우이", "슌", "안나"]);
        // a one-letter label whose best reading is far better than "슌", and free text that disagrees
        assert_eq!(pick(&n, &[-3.0, -25.0, -15.0], 51.0, "열"), None);
        // the same label with free text that agrees is taken
        assert_eq!(pick(&n, &[-3.0, -25.0, -15.0], 51.0, "순"), Some("슌"));
    }

    #[test]
    fn a_short_name_is_not_mistaken_for_its_outfit_and_the_reverse() {
        let n = names(&["토키", "토키(바니걸)", "토키(무장)", "마키"]);
        // reader preferred "토키" (-15.0) over "토키(바니걸)" (-15.6); the label is 82 px wide
        assert_eq!(pick(&n, &[-15.0, -15.6, -21.9, -24.3], 82.0, "토키바니걸"), Some("토키(바니걸)"));
        // a 58 px label with the same scores is the short name
        assert_eq!(pick(&n, &[-3.0, -9.0, -9.5, -12.0], 58.0, "토키"), Some("토키"));
    }

    #[test]
    fn a_name_with_a_star_is_not_cut_short() {
        let n = names(&["시로코", "시로코*테러", "에리", "토키"]);
        // reader: "시로코" -9.2 ahead of "시로코*테러" -11.9; free text "시로코테러"; label 81 px
        assert_eq!(pick(&n, &[-9.2, -11.9, -11.6, -12.3], 81.0, "시로코테러"), Some("시로코*테러"));
        // the plain name is 64 px
        assert_eq!(pick(&n, &[-2.0, -11.9, -11.6, -12.3], 64.0, "시로코"), Some("시로코"));
    }

    #[test]
    fn doubt_gives_nobody() {
        let n = names(&["케이", "레이", "카호"]);
        // two-letter label, reader unsure between two names, free text unreadable
        assert_eq!(pick(&n, &[-5.0, -5.2, -5.9], 57.5, ""), None);
        // nothing is as wide as the ink
        assert_eq!(pick(&n, &[-1.0, -2.0, -3.0], 85.0, "케이"), None);
    }
}
