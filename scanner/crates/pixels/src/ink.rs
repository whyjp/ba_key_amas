//! Ink isolation: pixels accepted by a predicate become black (0) on white (255).
//! These are the pixel halves of Go `ocrInk`, `ocrBright`, `heartInkImage`,
//! `nameInkImage`; callers upscale (Go: 6x CatmullRom) and hand them to OCR.

use crate::{white_gray, Rect, NAME_ROI, HEART_ROI, px};
use image::{GrayImage, Luma, RgbImage};

/// Green "after unlock" numbers on the light panel.
pub fn green_ink(c: [u8; 3]) -> bool {
    let [r, g, b] = c.map(|v| v as i32);
    g > 130 && g > r + 40 && g > b + 20
}

/// Near-white glyphs (name strip, bond number text).
pub fn white_ink(c: [u8; 3]) -> bool {
    c[0] > 225 && c[1] > 225 && c[2] > 225
}

/// Go `ocrInk` image step: the region `r` with accepted pixels black on white.
pub fn ink_image(img: &RgbImage, r: Rect, ink: impl Fn([u8; 3]) -> bool) -> GrayImage {
    let (w, h) = ((r.2 - r.0).max(0) as u32, (r.3 - r.1).max(0) as u32);
    let mut bw = white_gray(w, h);
    for y in 0..h {
        for x in 0..w {
            if ink(px(img, r.0 + x as i32, r.1 + y as i32)) {
                bw.put_pixel(x, y, Luma([0]));
            }
        }
    }
    bw
}

/// Isolates the bond number drawn in dark ink inside the pink heart. Only dark
/// pixels with pink on both sides of the same row count as glyphs.
pub fn heart_ink_image(img: &RgbImage) -> GrayImage {
    let r = HEART_ROI;
    let pink = |[cr, cg, cb]: [u8; 3]| {
        cr > 225 && cg > 150 && cg < 235 && cb > 180 && cr as i32 - cg as i32 > 15
    };
    let (w, h) = (r.2 - r.0, r.3 - r.1);
    let mut bw = white_gray(w as u32, h as u32);
    for y in 0..h {
        let (mut left, mut right) = (-1, -1);
        for x in 0..w {
            if pink(px(img, r.0 + x, r.1 + y)) {
                if left < 0 {
                    left = x;
                }
                right = x;
            }
        }
        for x in 0..w {
            if left >= 0 && x > left && x < right {
                let [cr, cg, cb] = px(img, r.0 + x, r.1 + y);
                if cr < 140 && cg < 150 && cb < 180 {
                    bw.put_pixel(x as u32, y as u32, Luma([0]));
                }
            }
        }
    }
    bw
}

/// White name glyphs on the name strip (`NAME_ROI`).
pub fn name_ink_image(img: &RgbImage) -> GrayImage {
    ink_image(img, NAME_ROI, white_ink)
}

/// Go `ocrBright` image step: bright pixels (all channels > `thr`) become ink on a
/// `k`-times (nearest) enlarged grid, then strokes are dilated by `dil` pixels.
/// Go uses `k = 6`.
pub fn bright_ink_image(img: &RgbImage, r: Rect, thr: u8, dil: i32, k: u32) -> GrayImage {
    let (w, h) = (((r.2 - r.0).max(0) as u32) * k, ((r.3 - r.1).max(0) as u32) * k);
    let mut bw = white_gray(w, h);
    for y in 0..h {
        for x in 0..w {
            let c = px(img, r.0 + (x / k) as i32, r.1 + (y / k) as i32);
            if c[0] > thr && c[1] > thr && c[2] > thr {
                bw.put_pixel(x, y, Luma([0]));
            }
        }
    }
    let mut out = white_gray(w, h);
    let (wi, hi) = (w as i32, h as i32);
    for y in 0..hi {
        for x in 0..wi {
            'search: for dy in -dil..=dil {
                for dx in -dil..=dil {
                    let (xx, yy) = (x + dx, y + dy);
                    if xx >= 0 && yy >= 0 && xx < wi && yy < hi && bw.get_pixel(xx as u32, yy as u32).0[0] == 0 {
                        out.put_pixel(x as u32, y as u32, Luma([0]));
                        break 'search;
                    }
                }
            }
        }
    }
    out
}
