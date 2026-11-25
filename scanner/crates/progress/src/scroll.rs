//! How far a scrolled list moved between two frames, measured from the pixels.
//!
//! A scan scrolls a list by a fixed number of wheel movements, but the distance one movement covers differs
//! with the list and the window, and near the end of the list it covers less. The runner therefore measures
//! the shift instead of assuming it, and numbers the rows it sees by the sum of the shifts.
use crate::vision::Rect;
use image::RgbImage;

/// Columns and rows are sampled at this step; the comparison is of a coarse picture of the list.
const STEP: usize = 3;
/// Mean absolute grey difference below which two strips can count as the same content. Animated characters and
/// speech bubbles beside a list keep it from reaching zero.
const SAME: f32 = 25.0;
/// The best shift must also be this much better than a typical one, or it is only the least bad of many.
const CLEAR: f32 = 0.5;

fn grey(img: &RgbImage, area: Rect) -> (Vec<Vec<f32>>, usize) {
    let rows = (area.y0..area.y1).map(|y| {
        (area.x0..area.x1)
            .step_by(STEP)
            .map(|x| {
                let p = img.get_pixel(x as u32, y as u32);
                0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32
            })
            .collect()
    });
    (rows.collect(), 0)
}

/// Pixels the content of `area` moved up between `prev` and `cur` (the list was scrolled down by that much):
/// `Some(0)` when it did not move, `None` when no shift up to `max_shift` explains the change (the list
/// re-laid itself out, or something drew over it).
pub fn content_shift(prev: &RgbImage, cur: &RgbImage, area: Rect, max_shift: i32) -> Option<i32> {
    let area = area.clamp(prev.width().min(cur.width()), prev.height().min(cur.height()));
    if area.h() <= 2 * STEP as i32 || area.w() <= 0 {
        return None;
    }
    let (a, _) = grey(prev, area);
    let (b, _) = grey(cur, area);
    let rows = a.len() as i32;
    let mut errs: Vec<(i32, f32)> = Vec::new();
    for dy in 0..=max_shift.min(rows - 1 - STEP as i32) {
        // the strip of `cur` that should show what `prev` showed `dy` rows lower; compare a margin only
        let overlap = rows - dy;
        let mut sum = 0.0f32;
        let mut n = 0usize;
        for y in (0..overlap).step_by(STEP) {
            let (ra, rb) = (&a[(y + dy) as usize], &b[y as usize]);
            sum += ra.iter().zip(rb).map(|(p, q)| (p - q).abs()).sum::<f32>();
            n += ra.len();
        }
        errs.push((dy, sum / n.max(1) as f32));
    }
    let (dy, err) = *errs.iter().min_by(|a, b| a.1.total_cmp(&b.1))?;
    let mut sorted: Vec<f32> = errs.iter().map(|e| e.1).collect();
    sorted.sort_by(f32::total_cmp);
    let median = sorted[sorted.len() / 2];
    (err < SAME && err < CLEAR * median).then_some(dy)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::png;
    use image::Rgb;

    fn shifted(img: &RgbImage, dy: u32) -> RgbImage {
        let mut out = RgbImage::from_pixel(img.width(), img.height(), Rgb([40, 40, 40]));
        for y in 0..img.height() - dy {
            for x in 0..img.width() {
                out.put_pixel(x, y, *img.get_pixel(x, y + dy));
            }
        }
        out
    }

    fn area() -> Rect {
        Rect { x0: 520, y0: 110, x1: 1270, y1: 700 }
    }

    #[test]
    fn a_list_moved_up_by_150_pixels_is_measured() {
        let img = png("w169-event-list-past");
        assert_eq!(content_shift(&img, &shifted(&img, 150), area(), 400), Some(150));
    }

    #[test]
    fn the_4_3_story_list_scrolled_by_a_wheel_step_is_measured() {
        let (a, b) = (png("menu-event-list-story-4x3"), png("menu-event-list-story-4x3-scrolled"));
        let shift = content_shift(&a, &b, Rect { x0: 512, y0: 144, x1: 1280, y1: 940 }, 700).expect("a shift");
        assert!((600..=606).contains(&shift), "{shift}");
    }

    #[test]
    fn an_unmoved_list_is_zero() {
        let img = png("w169-event-list-past");
        assert_eq!(content_shift(&img, &img, area(), 400), Some(0));
    }

    #[test]
    fn a_different_screen_has_no_shift() {
        assert_eq!(content_shift(&png("w169-event-list-past"), &png("w169-schedule"), area(), 400), None);
    }
}
