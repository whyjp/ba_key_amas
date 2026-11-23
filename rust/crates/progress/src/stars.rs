//! Counting the lit stars of a row (three star icons under a stage number).
//!
//! The icons sit centred under the stage number box, spaced about 0.56 of its height apart, with their
//! middle about 0.44 of that height below it. Measured on 16:9 recordings (event quest rows, mission rows);
//! 4:3 was not recorded, so only the proportions to the number box are used, never pixel constants.
use crate::geom::{cx, h};
use crate::vision::{count, is_grey_star, is_yellow, Rect};
use baocr_core::ocr_api::TextBox;
use image::RgbImage;

const SPACING: f32 = 0.56;
const BELOW: f32 = 0.44;
const HALF: f32 = 0.25;
/// Pixels of a colour a star slot needs to count as lit or unlit.
const MIN_PIXELS: usize = 20;

/// Lit stars (0..=3) of the row whose number box is `number`, or `None` when no star icons are there.
pub fn count_stars(img: &RgbImage, number: &TextBox) -> Option<u8> {
    let unit = h(number);
    let (mid_y, half) = (number.y1 + BELOW * unit, HALF * unit);
    let (mut lit, mut seen) = (0u8, 0u8);
    for slot in [-1.0f32, 0.0, 1.0] {
        let x = cx(number) + slot * SPACING * unit;
        let rect = Rect { x0: (x - half) as i32, y0: (mid_y - half) as i32, x1: (x + half) as i32 + 1, y1: (mid_y + half) as i32 + 1 };
        if count(img, rect, is_yellow) >= MIN_PIXELS {
            lit += 1;
            seen += 1;
        } else if count(img, rect, is_grey_star) >= MIN_PIXELS {
            seen += 1;
        }
    }
    (seen == 3).then_some(lit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{boxes, png};

    fn number(b: &[TextBox], t: &str) -> TextBox {
        b.iter().find(|x| x.text.trim() == t).unwrap_or_else(|| panic!("no box {t}")).clone()
    }

    #[test]
    fn event_quest_rows_are_all_three_stars() {
        let (b, img) = (boxes("w169-event-open"), png("w169-event-open"));
        for n in ["08", "09", "10", "11", "12"] {
            assert_eq!(count_stars(&img, &number(&b, n)), Some(3), "stage {n}");
        }
    }

    #[test]
    fn mission_rows_count_lit_and_unlit() {
        let (b, img) = (boxes("w169-mission-list"), png("w169-mission-list"));
        assert_eq!(count_stars(&img, &number(&b, "19-2")), Some(2));
        assert_eq!(count_stars(&img, &number(&b, "19-3")), Some(3));
        assert_eq!(count_stars(&img, &number(&b, "19-4")), Some(3));
        assert_eq!(count_stars(&img, &number(&b, "19-5")), Some(0));
    }

    #[test]
    fn no_icons_is_not_zero_stars() {
        // a number box over open sky and sea: nothing to count
        let img = png("w169-event-open");
        let nowhere = TextBox { x0: 420.0, y0: 150.0, x1: 460.0, y1: 183.0, text: "01".into(), score: 1.0 };
        assert_eq!(count_stars(&img, &nowhere), None);
    }
}
