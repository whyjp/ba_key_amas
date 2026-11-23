//! The MomoTalk student list: one row per student, the name on the left and a pink heart with the bond rank
//! number on the right.
//!
//! The OCR does not read the number inside the heart, so rows are anchored on the hearts, found by colour,
//! and each heart is paired with the name box on its line. The heart's rectangle is returned so the caller can
//! read the digit from the crop (`Ocr::line`) and pass the text to [`parse_bond`]; when the OCR did read a number
//! inside the heart it is used directly. The list scrolls, so a read returns the rows visible and the caller
//! merges reads by name.
use crate::geom::{cx, cy, digits, h, squashed, text, w};
use crate::schema::MomoStudent;
use crate::vision::{blobs, is_pink, Rect};
use crate::Read;
use baocr_core::ocr_api::TextBox;
use image::RgbImage;

#[derive(Debug, Clone, PartialEq)]
pub struct MomoRow {
    pub name: String,
    /// The heart on this row, `[x0, y0, x1, y1)`, the crop whose digit is the bond rank.
    pub heart: [i32; 4],
    /// The bond rank when the OCR boxes already carry it.
    pub bond: Option<u32>,
}

impl MomoRow {
    pub fn student(&self) -> MomoStudent {
        MomoStudent { name: self.name.clone(), bond: self.bond }
    }
}

/// A bond rank from the text of a heart crop: digits only, 1 to 100.
pub fn parse_bond(raw: &str) -> Option<u32> {
    let s: String = raw.chars().filter(|c| !c.is_whitespace()).collect();
    (!s.is_empty() && s.len() <= 3 && s.bytes().all(|c| c.is_ascii_digit())).then(|| s.parse().ok()).flatten().filter(|n| (1..=100).contains(n))
}

pub fn parse_momotalk_list(boxes: &[TextBox], img: &RgbImage) -> Read<Vec<MomoRow>> {
    let mut warnings = Vec::new();
    let Some(header) = boxes.iter().find(|b| squashed(b).contains("모든학생")) else {
        warnings.push("momotalk: no 모든 학생 header, not the student list".into());
        return Read { value: Vec::new(), warnings };
    };
    // The hearts lie in a column well right of the header text; names lie between the header's left edge and the hearts.
    let column = Rect { x0: (header.x0 + 2.0 * w(header)) as i32, y0: header.y1 as i32, x1: (header.x0 + 5.5 * w(header)) as i32, y1: img.height() as i32 };
    let mut hearts: Vec<Rect> = blobs(img, column, is_pink)
        .into_iter()
        .map(|b| b.bounds)
        .filter(|r| (r.w() as f32) >= 0.8 * h(header) && (r.w() as f32) <= 3.0 * h(header) && (r.h() as f32) >= 0.8 * h(header))
        .collect();
    hearts.sort_by_key(|r| r.y0);

    let names: Vec<&TextBox> = boxes
        .iter()
        .filter(|b| b.y0 >= header.y1 && b.x0 > header.x0 + 0.3 * w(header) && !text(b).is_empty())
        .collect();

    let mut rows = Vec::new();
    for heart in hearts {
        let (hcy, hh) = ((heart.y0 + heart.y1) as f32 / 2.0, heart.h() as f32);
        // the name sits at or just above the heart's middle; the subtitle is below it
        let name = names
            .iter()
            .filter(|b| cy(b) <= hcy + 0.15 * hh && hcy - cy(b) <= 0.6 * hh && b.x1 < heart.x0 as f32)
            .max_by(|a, b| cy(a).total_cmp(&cy(b)));
        let Some(name) = name else {
            warnings.push(format!("momotalk: heart at y={} has no name on its line (row cut off by the scroll), skipped", heart.y0));
            continue;
        };
        let inside = boxes.iter().find(|b| cx(b) > heart.x0 as f32 && cx(b) < heart.x1 as f32 && cy(b) > heart.y0 as f32 && cy(b) < heart.y1 as f32 && digits(b, 3).is_some());
        let bond = inside.and_then(|b| parse_bond(&b.text));
        if bond.is_none() {
            warnings.push(format!("momotalk: bond rank of {} is in the heart crop, not in the OCR boxes", text(name)));
        }
        rows.push(MomoRow { name: text(name).to_string(), heart: [heart.x0, heart.y0, heart.x1, heart.y1], bond });
    }
    if rows.is_empty() {
        warnings.push("momotalk: no student rows read".into());
    }
    Read { value: rows, warnings }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{boxes, png};

    #[test]
    fn five_visible_rows_with_hearts() {
        let r = parse_momotalk_list(&boxes("w169-momotalk-list"), &png("w169-momotalk-list"));
        let names: Vec<&str> = r.value.iter().map(|x| x.name.as_str()).collect();
        assert_eq!(names, vec!["나구사(수영복)", "네루(교복)", "노노미", "노도카", "노도카(온천)"]);
        // the heart crops are where the digits are; the OCR read none of them, so the rank stays null
        let first = r.value[0].heart;
        assert!((620..=624).contains(&first[0]) && (268..=274).contains(&first[1]), "{first:?}");
        assert!(r.value.iter().all(|x| x.bond.is_none()));
        assert_eq!(r.warnings.len(), 5);
    }

    #[test]
    fn a_digit_the_ocr_did_read_inside_a_heart_is_used() {
        let mut b = boxes("w169-momotalk-list");
        b.push(TextBox { x0: 633.0, y0: 276.0, x1: 650.0, y1: 298.0, text: "2".into(), score: 1.0 });
        let r = parse_momotalk_list(&b, &png("w169-momotalk-list"));
        assert_eq!(r.value[0].bond, Some(2));
        assert_eq!(r.value[0].student(), MomoStudent { name: "나구사(수영복)".into(), bond: Some(2) });
        assert_eq!(r.value[1].bond, None);
    }

    #[test]
    fn bond_text_parsing() {
        assert_eq!(parse_bond(" 5 "), Some(5));
        assert_eq!(parse_bond("12"), Some(12));
        assert_eq!(parse_bond("0"), None);
        assert_eq!(parse_bond("a5"), None);
        assert_eq!(parse_bond(""), None);
    }

    #[test]
    fn the_detail_pane_screen_is_still_the_list() {
        // with a student selected the right pane shows a copy of the name; it is not a row
        let r = parse_momotalk_list(&boxes("w169-momotalk-student"), &png("w169-momotalk-list"));
        assert_eq!(r.value.len(), 5);
    }

    #[test]
    fn other_screens_are_not_the_list() {
        let r = parse_momotalk_list(&boxes("w169-lobby"), &png("w169-lobby"));
        assert!(r.value.is_empty());
    }
}
