//! Pixel helpers: connected blobs of a colour class and long straight lines.
use image::RgbImage;

/// Half-open pixel rectangle `[x0, x1) x [y0, y1)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x0: i32,
    pub y0: i32,
    pub x1: i32,
    pub y1: i32,
}

impl Rect {
    pub fn w(&self) -> i32 {
        self.x1 - self.x0
    }
    pub fn h(&self) -> i32 {
        self.y1 - self.y0
    }
    /// The part of the rectangle inside an image of `w` x `h`.
    pub fn clamp(&self, w: u32, h: u32) -> Rect {
        Rect { x0: self.x0.clamp(0, w as i32), y0: self.y0.clamp(0, h as i32), x1: self.x1.clamp(0, w as i32), y1: self.y1.clamp(0, h as i32) }
    }
}

pub fn px(img: &RgbImage, x: i32, y: i32) -> [u8; 3] {
    img.get_pixel(x as u32, y as u32).0
}

/// Lit star yellow.
pub fn is_yellow(p: [u8; 3]) -> bool {
    p[0] > 200 && p[1] > 150 && p[2] < 130
}

/// Unlit star grey (neutral, mid light).
pub fn is_grey_star(p: [u8; 3]) -> bool {
    let [r, g, b] = p.map(|c| c as i32);
    (r - g).abs() < 10 && (g - b).abs() < 10 && (185..215).contains(&r)
}

/// Stamp red.
pub fn is_red(p: [u8; 3]) -> bool {
    let [r, g, b] = p.map(|c| c as i32);
    r > 190 && r - g > 90 && r - b > 90 && (g - b).abs() < 45
}

/// MomoTalk heart pink.
pub fn is_pink(p: [u8; 3]) -> bool {
    let [r, g, b] = p.map(|c| c as i32);
    r > 215 && (120..205).contains(&g) && (160..235).contains(&b) && r - g > 35 && b - g > 15
}

/// Grey card border: faintly bluish to neutral, mid light.
pub fn is_border_grey(p: [u8; 3]) -> bool {
    let [r, _, b] = p.map(|c| c as i32);
    b - r <= 18 && (150..215).contains(&r)
}

pub fn count(img: &RgbImage, rect: Rect, pred: fn([u8; 3]) -> bool) -> usize {
    let r = rect.clamp(img.width(), img.height());
    let mut n = 0;
    for y in r.y0..r.y1 {
        for x in r.x0..r.x1 {
            n += pred(px(img, x, y)) as usize;
        }
    }
    n
}

pub fn mean_luma(img: &RgbImage, rect: Rect) -> Option<f32> {
    let r = rect.clamp(img.width(), img.height());
    let area = (r.w() * r.h()) as f32;
    if area <= 0.0 {
        return None;
    }
    let mut sum = 0.0;
    for y in r.y0..r.y1 {
        for x in r.x0..r.x1 {
            let [r, g, b] = px(img, x, y);
            sum += 0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32;
        }
    }
    Some(sum / area)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Blob {
    pub bounds: Rect,
    pub pixels: usize,
}

/// Eight-connected blobs of pixels matching `pred` inside `rect`.
pub fn blobs(img: &RgbImage, rect: Rect, pred: fn([u8; 3]) -> bool) -> Vec<Blob> {
    let r = rect.clamp(img.width(), img.height());
    let (w, h) = (r.w().max(0) as usize, r.h().max(0) as usize);
    let mut seen = vec![false; w * h];
    let mut out = Vec::new();
    for sy in 0..h {
        for sx in 0..w {
            if seen[sy * w + sx] || !pred(px(img, r.x0 + sx as i32, r.y0 + sy as i32)) {
                continue;
            }
            seen[sy * w + sx] = true;
            let mut stack = vec![(sx as i32, sy as i32)];
            let (mut n, mut bx0, mut by0, mut bx1, mut by1) = (0, sx as i32, sy as i32, sx as i32, sy as i32);
            while let Some((x, y)) = stack.pop() {
                n += 1;
                bx0 = bx0.min(x);
                by0 = by0.min(y);
                bx1 = bx1.max(x);
                by1 = by1.max(y);
                for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (-1, -1), (1, -1), (-1, 1)] {
                    let (u, v) = (x + dx, y + dy);
                    if u < 0 || v < 0 || u >= w as i32 || v >= h as i32 {
                        continue;
                    }
                    let i = v as usize * w + u as usize;
                    if !seen[i] && pred(px(img, r.x0 + u, r.y0 + v)) {
                        seen[i] = true;
                        stack.push((u, v));
                    }
                }
            }
            out.push(Blob { bounds: Rect { x0: r.x0 + bx0, y0: r.y0 + by0, x1: r.x0 + bx1 + 1, y1: r.y0 + by1 + 1 }, pixels: n });
        }
    }
    out
}

/// Positions (along the scanned axis) of straight lines of `pred` pixels at least `min_len` long, found inside
/// `rect`. `vertical` scans for vertical lines (returns x). Lines closer than `merge` apart are one line.
pub fn long_lines(img: &RgbImage, rect: Rect, vertical: bool, min_len: i32, merge: i32, pred: fn([u8; 3]) -> bool) -> Vec<i32> {
    let r = rect.clamp(img.width(), img.height());
    let (a0, a1, b0, b1) = if vertical { (r.x0, r.x1, r.y0, r.y1) } else { (r.y0, r.y1, r.x0, r.x1) };
    let at = |a: i32, b: i32| if vertical { px(img, a, b) } else { px(img, b, a) };
    let mut hits = Vec::new();
    for a in a0..a1 {
        let (mut run, mut gap, mut best) = (0, 0, 0);
        for b in b0..b1 {
            if pred(at(a, b)) {
                run += 1 + gap;
                gap = 0;
                best = best.max(run);
            } else if run > 0 && gap < 2 {
                gap += 1;
            } else {
                run = 0;
                gap = 0;
            }
        }
        if best >= min_len {
            hits.push(a);
        }
    }
    let mut lines: Vec<(i32, i32)> = Vec::new(); // (first, last) of each cluster
    for a in hits {
        match lines.last_mut() {
            Some(l) if a - l.1 <= merge => l.1 = a,
            _ => lines.push((a, a)),
        }
    }
    lines.into_iter().map(|(f, l)| (f + l) / 2).collect()
}
