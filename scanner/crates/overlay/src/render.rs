//! Drawing basics and the palette: pure pixels in, pixels out, so the design can be checked
//! without a game window. Flat light cards with a navy header bar, like the game's own panels.
use ab_glyph::{point, Font, FontVec, PxScale, ScaleFont};
use image::{Rgba, RgbaImage};

/// Rectangle as (left, top, right, bottom) in screen pixels.
pub type Ltrb = (i32, i32, i32, i32);

// Palette: flat surfaces in the game's own navy and pale blue.
pub(crate) const NAVY: [u8; 3] = [31, 49, 84];
pub(crate) const NAVY_SOFT: [u8; 3] = [150, 168, 200];
pub(crate) const BODY: [u8; 3] = [245, 247, 251];
pub(crate) const WHITE: [u8; 3] = [255, 255, 255];
pub(crate) const BORDER: [u8; 3] = [188, 198, 214];
pub(crate) const RULE: [u8; 3] = [221, 227, 237];
pub(crate) const TRACK: [u8; 3] = [218, 225, 236];
pub(crate) const INK: [u8; 3] = [24, 33, 52];
pub(crate) const MUTED: [u8; 3] = [98, 112, 136];
pub(crate) const BLUE: [u8; 3] = [38, 112, 196];
pub(crate) const AMBER: [u8; 3] = [150, 96, 8];
pub(crate) const TEXT: [u8; 3] = [255, 255, 255];

/// A flat card: opaque `body` with a 1px `border`, rounded corners, and a navy header of
/// height `head` whose top-right corner is cut at 45 degrees (the one game-like detail).
pub(crate) fn card(img: &mut RgbaImage, w: f32, h: f32, s: f32, head: f32, cut: f32, body: [u8; 3]) {
    let b = (1.0 * s).max(1.0);
    fill_rrect(img, 0.0, 0.0, w, h, 5.0 * s, |x, y| {
        // inside the cut triangle: transparent, anti-aliased along the diagonal
        let d = (w - cut + y) - x;
        let a = (0.5 + d * std::f32::consts::FRAC_1_SQRT_2).clamp(0.0, 1.0);
        let rgb = if y < head {
            NAVY
        } else if x < b || x > w - b - 0.5 || y > h - b - 0.5 {
            BORDER
        } else {
            body
        };
        (rgb, a)
    });
}

#[derive(Debug, Clone, PartialEq)]
pub enum Phase {
    /// Counting the roster to learn the total.
    Counting,
    Scanning,
    Done,
    Failed,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BannerState {
    pub phase: Phase,
    pub done: usize,
    /// None while the total is not known yet (indeterminate bar).
    pub total: Option<usize>,
    /// Extra line shown for Failed (reason).
    pub note: String,
}

impl BannerState {
    /// Counting the roster; `n` students found so far.
    pub fn counting(n: usize) -> Self {
        Self { phase: Phase::Counting, done: n, total: None, note: String::new() }
    }

    pub fn scanning(done: usize, total: Option<usize>) -> Self {
        Self { phase: Phase::Scanning, done, total, note: String::new() }
    }

    pub fn finished(done: usize, total: Option<usize>) -> Self {
        Self { phase: Phase::Done, done, total, note: String::new() }
    }

    pub fn failed(done: usize, total: Option<usize>, note: &str) -> Self {
        Self { phase: Phase::Failed, done, total, note: note.to_string() }
    }
}

impl Default for BannerState {
    fn default() -> Self {
        Self { phase: Phase::Counting, done: 0, total: None, note: String::new() }
    }
}

pub struct Fonts {
    pub regular: FontVec,
    pub semibold: FontVec,
    pub bold: FontVec,
}

/// Pretendard (SIL OFL 1.1, see assets/fonts/OFL.txt), embedded so no system font is needed.
/// Falls back to the system Korean fonts only if the embedded data fails to parse.
pub fn load_fonts() -> Option<Fonts> {
    let embedded = |b: &'static [u8]| FontVec::try_from_vec(b.to_vec()).ok();
    if let (Some(regular), Some(semibold), Some(bold)) = (
        embedded(include_bytes!("../../../../assets/fonts/Pretendard-Regular.otf")),
        embedded(include_bytes!("../../../../assets/fonts/Pretendard-SemiBold.otf")),
        embedded(include_bytes!("../../../../assets/fonts/Pretendard-Bold.otf")),
    ) {
        return Some(Fonts { regular, semibold, bold });
    }
    load_system_fonts()
}

/// System Korean fonts (Malgun Gothic ships with Windows).
fn load_system_fonts() -> Option<Fonts> {
    let dir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into());
    let read = |names: &[&str]| -> Option<FontVec> {
        names.iter().find_map(|n| {
            let bytes = std::fs::read(format!("{dir}\\Fonts\\{n}")).ok()?;
            FontVec::try_from_vec(bytes).ok()
        })
    };
    let regular = read(&["malgun.ttf", "NanumGothic.ttf", "segoeui.ttf"])?;
    let bold = read(&["malgunbd.ttf", "NanumGothicBold.ttf", "segoeuib.ttf"]).unwrap_or_else(|| read(&["malgun.ttf"]).unwrap());
    let semibold = read(&["malgunbd.ttf", "malgun.ttf"])?;
    Some(Fonts { regular, semibold, bold })
}

pub(crate) fn over(dst: &mut Rgba<u8>, rgb: [u8; 3], alpha: f32) {
    let a = alpha.clamp(0.0, 1.0);
    if a <= 0.0 {
        return;
    }
    let da = dst[3] as f32 / 255.0;
    let out_a = a + da * (1.0 - a);
    for i in 0..3 {
        let c = (rgb[i] as f32 * a + dst[i] as f32 * da * (1.0 - a)) / out_a.max(1e-6);
        dst[i] = c.round().clamp(0.0, 255.0) as u8;
    }
    dst[3] = (out_a * 255.0).round().clamp(0.0, 255.0) as u8;
}

/// Signed distance of (px,py) to a rounded rect's edge: negative inside.
fn rrect_dist(px: f32, py: f32, x: f32, y: f32, w: f32, h: f32, r: f32) -> f32 {
    let (cx, cy) = (x + w / 2.0, y + h / 2.0);
    let (qx, qy) = ((px - cx).abs() - (w / 2.0 - r), (py - cy).abs() - (h / 2.0 - r));
    (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0) - r
}

/// Coverage (0..1) of pixel centre (px,py) by a rounded rect, anti-aliased by a 1px ramp.
fn rrect_cov(px: f32, py: f32, x: f32, y: f32, w: f32, h: f32, r: f32) -> f32 {
    (0.5 - rrect_dist(px, py, x, y, w, h, r)).clamp(0.0, 1.0)
}

/// The outline of a rounded rect, `stroke` px thick, drawn inward from its edge.
#[allow(clippy::too_many_arguments)]
/// Blends the ring of a rounded rectangle, `stroke` wide, inside its edge. Only the pixels near the edge are visited:
/// a frame around a portrait is a thin ring, and visiting its whole box (four rings per frame, up to 24 frames) was most
/// of the time a full grid of marks took to draw.
pub(crate) fn stroke_rrect(img: &mut RgbaImage, x: f32, y: f32, w: f32, h: f32, r: f32, stroke: f32, rgb: [u8; 3], alpha: f32) {
    let (x0, y0) = ((x.floor() as i32 - 1).max(0), (y.floor() as i32 - 1).max(0));
    let (x1, y1) = (((x + w).ceil() as i32 + 1).min(img.width() as i32), ((y + h).ceil() as i32 + 1).min(img.height() as i32));
    // the ring is within `stroke + 0.5` inside the edge and 0.5 outside it; along a corner it can be `r` further in
    let (side, corner) = (stroke.ceil() as i32 + 3, (stroke + r).ceil() as i32 + 3);
    // limits of the box the clamp above did not cut: rows near the real top and bottom edges
    let (top, bottom) = (y.floor() as i32 - 1, (y + h).ceil() as i32 + 1);
    let ring = |img: &mut RgbaImage, py: i32, from: i32, to: i32| {
        for px in from.max(x0)..to.min(x1) {
            let d = rrect_dist(px as f32 + 0.5, py as f32 + 0.5, x, y, w, h, r);
            let cov = (0.5 - d).clamp(0.0, 1.0) * (0.5 + d + stroke).clamp(0.0, 1.0);
            if cov > 0.0 {
                over(img.get_pixel_mut(px as u32, py as u32), rgb, alpha * cov);
            }
        }
    };
    for py in y0..y1 {
        let (from_top, from_bottom) = (py - top, bottom - 1 - py);
        let reach = if from_top < side || from_bottom < side {
            None // along the top or bottom edge: the whole width
        } else if from_top < corner || from_bottom < corner {
            Some(corner)
        } else {
            Some(side)
        };
        match reach {
            None => ring(img, py, x0, x1),
            Some(n) => {
                let left_end = (x0 + n).min(x1);
                let right_start = (x1 - n).max(left_end);
                ring(img, py, x0, left_end);
                ring(img, py, right_start, x1);
            }
        }
    }
}

/// The same ring visiting every pixel of the box: what the faster one must give exactly.
#[cfg(test)]
#[allow(clippy::too_many_arguments)] // the same arguments as the ring it checks
pub(crate) fn stroke_rrect_all(img: &mut RgbaImage, x: f32, y: f32, w: f32, h: f32, r: f32, stroke: f32, rgb: [u8; 3], alpha: f32) {
    let (x0, y0) = ((x.floor() as i32 - 1).max(0), (y.floor() as i32 - 1).max(0));
    let (x1, y1) = (((x + w).ceil() as i32 + 1).min(img.width() as i32), ((y + h).ceil() as i32 + 1).min(img.height() as i32));
    for py in y0..y1 {
        for px in x0..x1 {
            let d = rrect_dist(px as f32 + 0.5, py as f32 + 0.5, x, y, w, h, r);
            let cov = (0.5 - d).clamp(0.0, 1.0) * (0.5 + d + stroke).clamp(0.0, 1.0);
            if cov > 0.0 {
                over(img.get_pixel_mut(px as u32, py as u32), rgb, alpha * cov);
            }
        }
    }
}

pub(crate) fn fill_rrect(img: &mut RgbaImage, x: f32, y: f32, w: f32, h: f32, r: f32, color: impl Fn(f32, f32) -> ([u8; 3], f32)) {
    let (x0, y0) = ((x.floor() as i32 - 1).max(0), (y.floor() as i32 - 1).max(0));
    let (x1, y1) = (((x + w).ceil() as i32 + 1).min(img.width() as i32), ((y + h).ceil() as i32 + 1).min(img.height() as i32));
    for py in y0..y1 {
        for px in x0..x1 {
            let cov = rrect_cov(px as f32 + 0.5, py as f32 + 0.5, x, y, w, h, r);
            if cov > 0.0 {
                let (rgb, a) = color(px as f32 - x, py as f32 - y);
                over(img.get_pixel_mut(px as u32, py as u32), rgb, a * cov);
            }
        }
    }
}

#[allow(dead_code)]
pub(crate) fn lerp(a: [u8; 3], b: [u8; 3], t: f32) -> [u8; 3] {
    let t = t.clamp(0.0, 1.0);
    [0, 1, 2].map(|i| (a[i] as f32 + (b[i] as f32 - a[i] as f32) * t).round() as u8)
}

pub fn text_width(font: &FontVec, px: f32, s: &str) -> f32 {
    let f = font.as_scaled(PxScale::from(px));
    s.chars().map(|c| f.h_advance(f.glyph_id(c))).sum()
}

/// Draw `s` with its top at `y` (so rows line up regardless of the glyphs).
#[allow(clippy::too_many_arguments)] // position, colour and text of one draw call
pub(crate) fn draw_text(img: &mut RgbaImage, font: &FontVec, px: f32, x: f32, y: f32, rgb: [u8; 3], alpha: f32, s: &str) {
    let scaled = font.as_scaled(PxScale::from(px));
    let mut caret = x;
    let baseline = y + scaled.ascent();
    for c in s.chars() {
        let id = scaled.glyph_id(c);
        let glyph = id.with_scale_and_position(PxScale::from(px), point(caret, baseline));
        if let Some(outlined) = font.outline_glyph(glyph) {
            let b = outlined.px_bounds();
            outlined.draw(|gx, gy, cov| {
                let (ix, iy) = (b.min.x as i32 + gx as i32, b.min.y as i32 + gy as i32);
                if ix >= 0 && iy >= 0 && (ix as u32) < img.width() && (iy as u32) < img.height() {
                    over(img.get_pixel_mut(ix as u32, iy as u32), rgb, cov * alpha);
                }
            });
        }
        caret += scaled.h_advance(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ring_that_visits_only_the_edge_draws_the_same_pixels_as_the_one_that_visits_every_pixel() {
        // sizes of the frames the add-on draws (a portrait frame, a slot, a very small one) at several scales and radii
        let cases = [(30.5, 20.25, 110.0, 98.0, 7.0, 3.5), (12.0, 8.0, 160.0, 130.0, 9.0, 5.0), (4.0, 4.0, 20.0, 14.0, 7.0, 2.0), (0.0, 0.0, 300.0, 40.0, 20.0, 8.0), (10.3, 10.7, 60.0, 60.0, 30.0, 6.0)];
        for (x, y, w, h, r, stroke) in cases {
            for (alpha, rgb) in [(1.0, [255, 255, 255]), (0.26, [40, 230, 150])] {
                let blank = RgbaImage::from_pixel(340, 200, image::Rgba([0, 0, 0, 0]));
                let (mut fast, mut all) = (blank.clone(), blank);
                stroke_rrect(&mut fast, x, y, w, h, r, stroke, rgb, alpha);
                stroke_rrect_all(&mut all, x, y, w, h, r, stroke, rgb, alpha);
                assert!(fast == all, "ring differs for {:?}", (x, y, w, h, r, stroke));
            }
        }
    }
}
