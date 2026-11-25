//! Marks drawn over the game itself, on student portraits. The layer covers the game's
//! client area; positions are given in the 1280x960 frame the rest of the program works in
//! and scaled to the window, so any window size gets the same placement.
//!
//! Few and clear: a rated student gets one badge for how well it suits the battle (the
//! better, the bigger and brighter) and the terrain face; the best and the worst also tint
//! their whole portrait (a ring for the best, a veil for the worst) so the candidates stand
//! out across the grid at a glance. A special that is not rated by type gets a quiet "SP".
use crate::panel::{Grade, Tone};
use crate::render::{draw_text, fill_rrect, over, stroke_rrect, text_width, Fonts, TEXT};
use std::collections::HashMap;
use image::{Rgba, RgbaImage};

/// One small icon.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Icon {
    /// Terrain adaptation as the game's face, 0 (worst, red) to 4 (best, green).
    Face(u8),
    /// How well the student suits the battle.
    Fit(Grade),
    /// A special judged by its skill role, not by types: no grade.
    Special,
    /// The student's attack type, a fixed fact from the database.
    Attack(TypeTag),
    /// The student's armor type, a fixed fact from the database.
    Armor(TypeTag),
    /// Attack and armor type side by side in one row, for places too short for two stacked badges.
    Pair(TypeTag, TypeTag),
}

/// The game's colours for attack and armor types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Hue {
    Red,
    Yellow,
    Blue,
    Purple,
    Grey,
}

/// A type badge: its colour and the one syllable written on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeTag {
    pub hue: Hue,
    pub glyph: char,
}

/// A portrait's outline in the 1280x960 frame, tinted by the grade of its student.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Frame {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
    pub grade: Grade,
}

/// A column (or row) of icons centred on (`cx`, `cy`) in the 1280x960 frame, and the portrait
/// outline it belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Mark {
    pub cx: i32,
    pub cy: i32,
    pub icons: Vec<Icon>,
    /// Stacked top to bottom instead of left to right.
    pub vertical: bool,
    pub frame: Option<Frame>,
}

impl Mark {
    pub fn row(cx: i32, cy: i32, icons: Vec<Icon>) -> Mark {
        Mark { cx, cy, icons, vertical: false, frame: None }
    }

    /// A column whose first icon starts at `top` and whose middle is at `cx`.
    pub fn column_from_top(cx: i32, top: i32, icons: Vec<Icon>) -> Mark {
        let height = span(&icons, true).round() as i32;
        Mark { cx, cy: top + height / 2, icons, vertical: true, frame: None }
    }

    pub fn framed(mut self, frame: Frame) -> Mark {
        self.frame = Some(frame);
        self
    }

    /// Where the column's first icon starts (frame pixels).
    pub fn top(&self) -> i32 {
        self.cy - (span(&self.icons, true).round() as i32) / 2
    }
}

/// Icon sizes (width, height) in the 1280x960 frame: the better the grade, the bigger the badge.
const FACE: f32 = 22.0;
const TYPE_BADGE: f32 = 21.0;
const GAP: f32 = 3.0;
/// Below this share of the frame scale icons stop shrinking: a mark of a few pixels says nothing.
const MIN_SCALE: f32 = 0.5;

fn extent(icon: &Icon) -> (f32, f32) {
    match icon {
        Icon::Face(_) => (FACE, FACE),
        Icon::Fit(Grade::Excellent) => (34.0, 34.0),
        Icon::Fit(Grade::Good) => (28.0, 28.0),
        Icon::Fit(_) => (19.0, 19.0),
        Icon::Special => (30.0, 17.0),
        Icon::Attack(_) | Icon::Armor(_) => (TYPE_BADGE, TYPE_BADGE),
        Icon::Pair(..) => (TYPE_BADGE * 2.0 + GAP, TYPE_BADGE),
    }
}

/// Length of a stack of icons along its direction, in the frame.
fn span(icons: &[Icon], vertical: bool) -> f32 {
    let sizes = icons.iter().map(|i| if vertical { extent(i).1 } else { extent(i).0 });
    sizes.sum::<f32>() + icons.len().saturating_sub(1) as f32 * GAP
}

/// The scale icons are drawn at for a client area `client_w` wide.
pub(crate) fn icon_scale(client_w: u32) -> f32 {
    (client_w as f32 / 1280.0).max(MIN_SCALE)
}

fn face_color(level: u8) -> [u8; 3] {
    match level {
        4 => [66, 188, 92],
        3 => [140, 206, 96],
        2 => [244, 206, 72],
        1 => [244, 154, 72],
        _ => [232, 92, 84],
    }
}

fn hue_rgb(hue: Hue) -> [u8; 3] {
    match hue {
        Hue::Red => [214, 76, 72],
        Hue::Yellow => [222, 168, 52],
        Hue::Blue => [72, 130, 220],
        Hue::Purple => [150, 96, 200],
        Hue::Grey => [130, 136, 150],
    }
}

/// A stroke from (x0, y0) to (x1, y1), round ended.
fn line(img: &mut RgbaImage, a: (f32, f32), b: (f32, f32), width: f32, rgb: [u8; 3], alpha: f32) {
    let n = ((b.0 - a.0).hypot(b.1 - a.1) / (width * 0.35).max(0.5)).ceil().max(1.0) as i32;
    for i in 0..=n {
        let t = i as f32 / n as f32;
        let (x, y) = (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
        fill_rrect(img, x - width / 2.0, y - width / 2.0, width, width, width / 2.0, |_, _| (rgb, alpha));
    }
}

const OUTLINE: [u8; 3] = [10, 18, 34];

fn badge_fill(grade: Grade) -> [u8; 3] {
    match grade {
        Grade::Poor => [150, 96, 98],
        other => other.tone().rgb(),
    }
}

/// Draw one icon with its top-left at (x, y); `k` is the frame scale.
fn draw_icon(img: &mut RgbaImage, fonts: &Fonts, icon: &Icon, x: f32, y: f32, k: f32) {
    let (w, h) = (extent(icon).0 * k, extent(icon).1 * k);
    let edge = (1.5 * k).max(1.0);
    match icon {
        Icon::Face(level) => {
            fill_rrect(img, x - edge, y - edge, w + edge * 2.0, h + edge * 2.0, (w + edge * 2.0) / 2.0, |_, _| (OUTLINE, 0.85));
            let rgb = face_color(*level);
            let size = w;
            fill_rrect(img, x, y, size, size, size / 2.0, |_, py| (rgb, 1.0 - 0.18 * py / size));
            let ink = [38, 44, 58];
            let dot = size * 0.11;
            let (ey, ex1, ex2) = (y + size * 0.36, x + size * 0.32, x + size * 0.68);
            for ex in [ex1, ex2] {
                fill_rrect(img, ex - dot, ey - dot, dot * 2.0, dot * 2.0, dot, |_, _| (ink, 1.0));
            }
            // mouth: a parabola whose curvature follows the level (4 broad smile ... 0 deep frown)
            let curve = (*level as f32 - 2.0) / 2.0; // -1 (frown) .. +1 (smile)
            let (mx, my, half) = (x + size * 0.5, y + size * 0.64, size * 0.22);
            let steps = 12;
            for i in 0..=steps {
                let t = i as f32 / steps as f32 * 2.0 - 1.0; // -1..1
                let px = mx + t * half;
                let py = my + curve * size * 0.14 * (t * t - 1.0) * -1.0 - curve * size * 0.07;
                let r = size * 0.05;
                fill_rrect(img, px - r, py - r, r * 2.0, r * 2.0, r, |_, _| (ink, 1.0));
            }
        }
        Icon::Fit(grade) => {
            let alpha = if *grade == Grade::Poor { 0.7 } else { 1.0 };
            if *grade == Grade::Excellent {
                // a soft halo under the best badge
                for (grow, a) in [(4.0, 0.16), (2.5, 0.26)] {
                    let g = grow * k;
                    fill_rrect(img, x - g, y - g, w + g * 2.0, h + g * 2.0, (w + g * 2.0) / 2.0, |_, _| ([40, 230, 150], a));
                }
            }
            fill_rrect(img, x - edge, y - edge, w + edge * 2.0, h + edge * 2.0, (w + edge * 2.0) / 2.0, |_, _| (OUTLINE, 0.85 * alpha));
            let rgb = badge_fill(*grade);
            fill_rrect(img, x, y, w, h, w / 2.0, |_, py| (rgb, alpha * (1.0 - 0.14 * py / h)));
            let at = |fx: f32, fy: f32| (x + w * fx, y + h * fy);
            let stroke = w * 0.13;
            let ink = grade.tone().ink();
            match grade {
                Grade::Excellent | Grade::Good => {
                    line(img, at(0.25, 0.54), at(0.43, 0.71), stroke, TEXT, 1.0);
                    line(img, at(0.43, 0.71), at(0.76, 0.33), stroke, TEXT, 1.0);
                    if *grade == Grade::Excellent {
                        let ring = (2.0 * k).max(1.3);
                        stroke_rrect(img, x, y, w, h, w / 2.0, ring, [255, 214, 74], 1.0);
                    }
                }
                Grade::Average => line(img, at(0.28, 0.5), at(0.72, 0.5), stroke * 1.1, ink, 1.0),
                Grade::Poor => {
                    line(img, at(0.3, 0.3), at(0.7, 0.7), stroke, TEXT, 0.9);
                    line(img, at(0.7, 0.3), at(0.3, 0.7), stroke, TEXT, 0.9);
                }
            }
        }
        Icon::Attack(tag) | Icon::Armor(tag) => {
            // attack: a round badge, armor: a square one, so the two read apart even in one colour
            let radius = if matches!(icon, Icon::Attack(_)) { w / 2.0 } else { w * 0.22 };
            fill_rrect(img, x - edge, y - edge, w + edge * 2.0, h + edge * 2.0, radius + edge, |_, _| (OUTLINE, 0.85));
            let rgb = hue_rgb(tag.hue);
            fill_rrect(img, x, y, w, h, radius, |_, py| (rgb, 1.0 - 0.16 * py / h));
            let px = h * 0.68;
            let glyph = tag.glyph.to_string();
            let tw = text_width(&fonts.bold, px, &glyph);
            draw_text(img, &fonts.bold, px, x + (w - tw) / 2.0, y + (h - px * 1.2) / 2.0, TEXT, 1.0, &glyph);
        }
        Icon::Pair(attack, armor) => {
            draw_icon(img, fonts, &Icon::Attack(*attack), x, y, k);
            draw_icon(img, fonts, &Icon::Armor(*armor), x + (TYPE_BADGE + GAP) * k, y, k);
        }
        Icon::Special => {
            fill_rrect(img, x - edge, y - edge, w + edge * 2.0, h + edge * 2.0, h * 0.3, |_, _| (OUTLINE, 0.85));
            let rgb = Tone::Neutral.rgb();
            fill_rrect(img, x, y, w, h, h * 0.25, |_, _| (rgb, 1.0));
            let px = h * 0.72;
            let tw = text_width(&fonts.bold, px, "SP");
            draw_text(img, &fonts.bold, px, x + (w - tw) / 2.0, y + (h - px * 1.2) / 2.0, TEXT, 1.0, "SP");
        }
    }
}

/// The portrait's own tint: a bright ring for the best, a thinner one for the good, a veil
/// over the poor. Average gets nothing.
fn draw_frame(img: &mut RgbaImage, f: &Frame, k: f32, origin: (f32, f32)) {
    let (x, y) = (f.left as f32 * k - origin.0, f.top as f32 * k - origin.1);
    let (w, h) = ((f.right - f.left) as f32 * k, (f.bottom - f.top) as f32 * k);
    let r = 7.0 * k;
    match f.grade {
        Grade::Excellent => {
            let (core, glow) = ((3.5 * k).max(2.0), 3.0 * k);
            for (grow, a) in [(glow * 1.6, 0.14), (glow, 0.26)] {
                stroke_rrect(img, x - grow, y - grow, w + grow * 2.0, h + grow * 2.0, r + grow, core + grow, [40, 230, 150], a);
            }
            stroke_rrect(img, x, y, w, h, r, core, [255, 255, 255], 1.0);
            stroke_rrect(img, x + core * 0.25, y + core * 0.25, w - core * 0.5, h - core * 0.5, r, core * 0.75, [24, 206, 134], 1.0);
        }
        Grade::Good => {
            let core = (2.5 * k).max(1.6);
            stroke_rrect(img, x, y, w, h, r, core, [96, 198, 120], 0.95);
        }
        Grade::Average => {}
        Grade::Poor => fill_rrect(img, x, y, w, h, r, |_, _| ([8, 14, 28], 0.42)),
    }
}

/// Margin around a frame or an icon column that the glow and the anti-aliasing can reach (in icon scale units).
const FRAME_PAD: f32 = 12.0;
/// Room around an icon in its copy for its outline (in icon scale units).
const ICON_MARGIN: f32 = 4.0;

/// Lays `src` over `dst` with its top left corner at (`dx`, `dy`), pixel by pixel with the usual alpha blend.
fn blend_over(dst: &mut RgbaImage, src: &RgbaImage, dx: i64, dy: i64) {
    for (sx, sy, p) in src.enumerate_pixels() {
        if p[3] == 0 {
            continue;
        }
        let (x, y) = (sx as i64 + dx, sy as i64 + dy);
        if x >= 0 && y >= 0 && (x as u32) < dst.width() && (y as u32) < dst.height() {
            over(dst.get_pixel_mut(x as u32, y as u32), [p[0], p[1], p[2]], p[3] as f32 / 255.0);
        }
    }
}

/// The part of the client area the marks can touch, as (left, top, right, bottom) in client pixels (a margin is
/// included for the glow around a frame and for anti-aliasing), or None when there is nothing to draw.
pub fn marks_bounds(client_w: u32, client_h: u32, marks: &[Mark]) -> Option<(i32, i32, i32, i32)> {
    let frame_k = client_w as f32 / 1280.0;
    let k = icon_scale(client_w);
    let pad = FRAME_PAD * k.max(1.0);
    let mut b: Option<(f32, f32, f32, f32)> = None;
    let mut add = |l: f32, t: f32, r: f32, bt: f32| {
        b = Some(match b {
            Some((a, c, d, e)) => (a.min(l), c.min(t), d.max(r), e.max(bt)),
            None => (l, t, r, bt),
        });
    };
    for m in marks {
        if let Some(f) = &m.frame {
            add(f.left as f32 * frame_k - pad, f.top as f32 * frame_k - pad, f.right as f32 * frame_k + pad, f.bottom as f32 * frame_k + pad);
        }
        let total = span(&m.icons, m.vertical) * k;
        let wide = m.icons.iter().map(|i| extent(i).0).fold(0.0f32, f32::max) * k;
        let high = m.icons.iter().map(|i| extent(i).1).fold(0.0f32, f32::max) * k;
        if m.vertical {
            let (x, y) = (m.cx as f32 * frame_k, m.top() as f32 * frame_k);
            add(x - wide / 2.0 - pad, y - pad, x + wide / 2.0 + pad, y + total + pad);
        } else {
            let (x, y) = (m.cx as f32 * frame_k - total / 2.0, m.cy as f32 * frame_k);
            add(x - pad, y - high / 2.0 - pad, x + total + pad, y + high / 2.0 + pad);
        }
    }
    let (l, t, r, bt) = b?;
    let (l, t) = ((l.floor() as i32).max(0), (t.floor() as i32).max(0));
    let (r, bt) = ((r.ceil() as i32).min(client_w as i32), (bt.ceil() as i32).min(client_h as i32));
    (r > l && bt > t).then_some((l, t, r, bt))
}

/// Render the marks onto a transparent image that covers only where they are drawn. Returns it and the position of
/// its top left corner in the client area. A layer the size of the whole game cost 12 ms (at 1280x960) to 49 ms (at
/// 2560x1920) to clear, draw and hand to the window for 24 marks, and the compositor blends every pixel of it.
pub fn render_marks_clipped(client_w: u32, client_h: u32, marks: &[Mark], fonts: &Fonts) -> (RgbaImage, (i32, i32)) {
    let Some((x0, y0, x1, y1)) = marks_bounds(client_w, client_h, marks) else { return (RgbaImage::from_pixel(1, 1, Rgba([0, 0, 0, 0])), (0, 0)) };
    let mut img = RgbaImage::from_pixel((x1 - x0) as u32, (y1 - y0) as u32, Rgba([0, 0, 0, 0]));
    let origin = (x0 as f32, y0 as f32);
    let frame_k = client_w as f32 / 1280.0;
    let k = icon_scale(client_w);
    // portrait tints first, so no icon is ever under one. The frames of a grid are all one size, so each size and grade
    // is drawn once and copied to the other places (to the pixel: the copy is placed on the nearest whole pixel).
    let pad = FRAME_PAD * k.max(1.0);
    let mut sprites: HashMap<(i32, i32, Grade), RgbaImage> = HashMap::new();
    for m in marks {
        if let Some(f) = &m.frame {
            let sprite = sprites.entry((f.right - f.left, f.bottom - f.top, f.grade)).or_insert_with(|| {
                let (w, h) = (((f.right - f.left) as f32 * frame_k + 2.0 * pad).ceil() as u32, ((f.bottom - f.top) as f32 * frame_k + 2.0 * pad).ceil() as u32);
                let mut s = RgbaImage::from_pixel(w, h, Rgba([0, 0, 0, 0]));
                draw_frame(&mut s, &Frame { left: 0, top: 0, right: f.right - f.left, bottom: f.bottom - f.top, grade: f.grade }, frame_k, (-pad, -pad));
                s
            });
            let (dx, dy) = ((f.left as f32 * frame_k - origin.0 - pad).round() as i64, (f.top as f32 * frame_k - origin.1 - pad).round() as i64);
            blend_over(&mut img, sprite, dx, dy);
        }
    }
    // the same icon (a grade, a type) stands on many portraits: drawn once, copied to each place
    let mut icon_sprites: HashMap<Icon, RgbaImage> = HashMap::new();
    let margin = ICON_MARGIN * k;
    let mut put = |img: &mut RgbaImage, icon: &Icon, x: f32, y: f32| {
        let sprite = icon_sprites.entry(icon.clone()).or_insert_with(|| {
            let (w, h) = ((extent(icon).0 * k + 2.0 * margin).ceil() as u32, (extent(icon).1 * k + 2.0 * margin).ceil() as u32);
            let mut s = RgbaImage::from_pixel(w, h, Rgba([0, 0, 0, 0]));
            draw_icon(&mut s, fonts, icon, margin, margin, k);
            s
        });
        blend_over(img, sprite, (x - margin).round() as i64, (y - margin).round() as i64);
    };
    for m in marks {
        let total = span(&m.icons, m.vertical) * k;
        let (mut x, mut y) = if m.vertical { (m.cx as f32 * frame_k, m.top() as f32 * frame_k) } else { (m.cx as f32 * frame_k - total / 2.0, m.cy as f32 * frame_k) };
        for icon in &m.icons {
            let (w, h) = (extent(icon).0 * k, extent(icon).1 * k);
            if m.vertical {
                put(&mut img, icon, x - w / 2.0 - origin.0, y - origin.1);
                y += h + GAP * k;
            } else {
                put(&mut img, icon, x - origin.0, y - h / 2.0 - origin.1);
                x += w + GAP * k;
            }
        }
    }
    (img, (x0, y0))
}

/// Render the marks onto a transparent layer of `client_w` x `client_h` pixels (the whole client area).
pub fn render_marks(client_w: u32, client_h: u32, marks: &[Mark], fonts: &Fonts) -> RgbaImage {
    let mut full = RgbaImage::from_pixel(client_w, client_h, Rgba([0, 0, 0, 0]));
    let (part, (x, y)) = render_marks_clipped(client_w, client_h, marks, fonts);
    image::imageops::replace(&mut full, &part, x as i64, y as i64);
    full
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::load_fonts;

    fn sample() -> Vec<Mark> {
        let grades = [Grade::Excellent, Grade::Good, Grade::Average, Grade::Poor];
        (0..4)
            .map(|i| {
                let cx = 200 + 240 * i;
                Mark::column_from_top(cx + 52, 90, vec![Icon::Fit(grades[i as usize]), Icon::Face(i as u8 + 1)]).framed(Frame { left: cx - 54, top: 40, right: cx + 54, bottom: 190, grade: grades[i as usize] })
            })
            .chain([Mark::column_from_top(1180, 90, vec![Icon::Special])])
            .chain([Mark::column_from_top(1100, 300, vec![
                Icon::Attack(TypeTag { hue: Hue::Red, glyph: '폭' }),
                Icon::Armor(TypeTag { hue: Hue::Blue, glyph: '특' }),
                Icon::Face(3),
            ])])
            .collect()
    }

    #[test]
    fn a_grid_of_equal_frames_is_drawn_as_it_would_be_frame_by_frame() {
        let Some(f) = load_fonts() else { return };
        let marks: Vec<Mark> = (0..12)
            .map(|i| {
                let (x, y) = (600 + 110 * (i % 4), 322 + 104 * (i / 4));
                let grade = if i % 3 == 0 { Grade::Excellent } else { Grade::Good };
                Mark::column_from_top(x + 55, y + 38, vec![]).framed(Frame { left: x, top: y, right: x + 110, bottom: y + 98, grade })
            })
            .collect();
        // the scale is one at 1280 wide, so every frame lands on a whole pixel and the copy must be exact
        let (fast, at) = render_marks_clipped(1280, 960, &marks, &f);
        let mut direct = RgbaImage::from_pixel(fast.width(), fast.height(), Rgba([0, 0, 0, 0]));
        for m in marks.iter().filter_map(|m| m.frame.as_ref()) {
            draw_frame(&mut direct, m, 1.0, (at.0 as f32, at.1 as f32));
        }
        // a glow where two frames meet is blended in another order, which rounds differently by a step or two
        let worst = fast.pixels().zip(direct.pixels()).map(|(a, b)| (0..4).map(|i| (a[i] as i32 - b[i] as i32).abs()).max().unwrap_or(0)).max().unwrap_or(0);
        assert!(worst <= 3, "the copied frames differ from the drawn ones by {worst}");
    }

    #[test]
    fn copied_icons_cover_the_same_pixels_as_drawn_ones_to_within_half_a_pixel() {
        let Some(f) = load_fonts() else { return };
        let icons = vec![Icon::Fit(Grade::Excellent), Icon::Attack(TypeTag { hue: Hue::Red, glyph: '폭' }), Icon::Armor(TypeTag { hue: Hue::Yellow, glyph: '중' }), Icon::Face(3)];
        let (client_w, client_h) = (1280u32, 960u32);
        let marks: Vec<Mark> = (0..6).map(|i| Mark::column_from_top(660 + 110 * (i % 3), 360 + 120 * (i / 3), icons.clone())).collect();
        let (fast, at) = render_marks_clipped(client_w, client_h, &marks, &f);
        let mut direct = RgbaImage::from_pixel(fast.width(), fast.height(), Rgba([0, 0, 0, 0]));
        let k = icon_scale(client_w);
        for m in &marks {
            let (x, mut y) = (m.cx as f32, m.top() as f32);
            for icon in &m.icons {
                let (w, h) = (extent(icon).0 * k, extent(icon).1 * k);
                draw_icon(&mut direct, &f, icon, x - w / 2.0 - at.0 as f32, y - at.1 as f32, k);
                y += h + GAP * k;
            }
        }
        let covered = |img: &RgbaImage| img.pixels().filter(|p| p[3] > 128).count() as f64;
        let (a, b) = (covered(&fast), covered(&direct));
        assert!((a - b).abs() / b < 0.03, "{a} pixels covered by the copies, {b} by the drawn icons");
    }

    #[test]
    fn marks_are_drawn_where_asked_and_nowhere_else() {
        let Some(f) = load_fonts() else { return };
        for (w, h) in [(458u32, 344u32), (1280, 960)] {
            let img = render_marks(w, h, &sample(), &f);
            assert_eq!((img.width(), img.height()), (w, h));
            let k = w as f32 / 1280.0;
            let drawn = |x: f32, y: f32| img.get_pixel((x * k) as u32, (y * k) as u32)[3] > 0;
            assert!(drawn(252.0, 100.0), "the best student's badge, on its portrait's right edge");
            assert!(!drawn(640.0, 700.0), "empty space stays transparent");
            assert!(!drawn(5.0, 5.0));
            if let Some(dir) = std::env::var_os("BANNER_PREVIEW_DIR") {
                let mut out = RgbaImage::from_pixel(w, h, Rgba([150, 170, 200, 255]));
                image::imageops::overlay(&mut out, &img, 0, 0);
                out.save(std::path::Path::new(&dir).join(format!("marks-{w}.png"))).unwrap();
            }
        }
    }

    #[test]
    fn a_column_stacks_downward_from_its_top_and_a_row_does_not() {
        let Some(f) = load_fonts() else { return };
        let icons = vec![Icon::Face(4), Icon::Face(3), Icon::Face(2)];
        let col = Mark::column_from_top(300, 200, icons.clone());
        assert_eq!(col.top(), 200);
        let img = render_marks(1280, 960, &[col], &f);
        let drawn = |x: u32, y: u32| img.get_pixel(x, y)[3] > 0;
        // 22px icons with a 3px gap: centres at 211, 236, 261
        assert!(drawn(300, 211) && drawn(300, 236) && drawn(300, 261), "three icons one below the other");
        assert!(!drawn(300, 190) && !drawn(300, 290), "nothing above the top or below the stack");
        assert!(!drawn(270, 236) && !drawn(330, 236), "one icon wide");
    }

    #[test]
    fn badges_grow_with_the_grade_and_the_frame_tints_only_the_extremes() {
        let Some(f) = load_fonts() else { return };
        let one = |g: Grade| render_marks(1280, 960, &[Mark::column_from_top(300, 200, vec![Icon::Fit(g)]).framed(Frame { left: 100, top: 100, right: 220, bottom: 300, grade: g })], &f);
        let size = |img: &RgbaImage| (200..300u32).filter(|&y| img.get_pixel(300, y)[3] > 200).count();
        assert!(size(&one(Grade::Excellent)) > size(&one(Grade::Good)) && size(&one(Grade::Good)) > size(&one(Grade::Average)));
        let inside = |g: Grade| one(g).get_pixel(160, 200)[3];
        assert!(inside(Grade::Poor) > 80, "a poor student's portrait is veiled");
        assert_eq!(inside(Grade::Average), 0, "an average one is left alone");
        assert_eq!(inside(Grade::Good), 0, "a ring leaves the portrait itself clear");
        assert!(one(Grade::Excellent).get_pixel(101, 200)[3] > 200, "the best get a bright ring");
    }

    #[test]
    fn icons_keep_a_legible_size_in_a_small_window() {
        assert_eq!(icon_scale(1280), 1.0);
        assert_eq!(icon_scale(640), 0.5);
        assert_eq!(icon_scale(458), MIN_SCALE, "never shrunk below the floor");
        assert!(icon_scale(2560) > 1.9);
    }

    #[test]
    fn faces_differ_by_level_and_grades_by_kind() {
        let Some(f) = load_fonts() else { return };
        let one = |icon: Icon| render_marks(100, 100, &[Mark::row(640, 480, vec![icon])], &f);
        let at = |img: &RgbaImage| *img.get_pixel(50, 38);
        assert_ne!(at(&one(Icon::Face(4))), at(&one(Icon::Face(0))));
        assert_ne!(at(&one(Icon::Fit(Grade::Good))), at(&one(Icon::Fit(Grade::Average))));
    }
}
