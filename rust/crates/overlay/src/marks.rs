//! Icons drawn over the game itself (on student portraits). The layer covers the game's
//! client area; positions are given in the 1280x960 frame the rest of the program works in
//! and scaled to the window, so any window size gets the same placement.
use crate::panel::Tone;
use crate::render::{draw_text, fill_rrect, text_width, Fonts, TEXT};
use image::{Rgba, RgbaImage};

/// One small icon.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Icon {
    /// Terrain adaptation as the game's face, 0 (worst, red) to 4 (best, green).
    Face(u8),
    /// Attack type by SchaleDB code (Explosion, Pierce, Mystic, Sonic).
    Attack(String),
    /// Armor type by SchaleDB code.
    Armor(String),
}

/// A row of icons centred on (`cx`, `cy`) in the 1280x960 frame.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Mark {
    pub cx: i32,
    pub cy: i32,
    pub icons: Vec<Icon>,
}

/// Icon size in the 1280x960 frame.
const ICON: f32 = 20.0;
const GAP: f32 = 2.0;

fn face_color(level: u8) -> [u8; 3] {
    match level {
        4 => [66, 188, 92],
        3 => [140, 206, 96],
        2 => [244, 206, 72],
        1 => [244, 154, 72],
        _ => [232, 92, 84],
    }
}

fn glyph(icon: &Icon) -> Option<&'static str> {
    match icon {
        Icon::Attack(c) => Some(match c.as_str() {
            "Explosion" => "폭",
            "Pierce" => "관",
            "Mystic" => "신",
            "Sonic" => "진",
            _ => "일",
        }),
        Icon::Armor(c) => Some(match c.as_str() {
            "LightArmor" => "경",
            "HeavyArmor" => "중",
            "Unarmed" => "특",
            "ElasticArmor" => "탄",
            "CompositeArmor" => "복",
            _ => "일",
        }),
        Icon::Face(_) => None,
    }
}

fn tone(icon: &Icon) -> Tone {
    match icon {
        Icon::Attack(c) | Icon::Armor(c) => Tone::from_code(c),
        Icon::Face(_) => Tone::Neutral,
    }
}

/// Draw one icon with its top-left at (x, y), `size` px square.
fn draw_icon(img: &mut RgbaImage, fonts: &Fonts, icon: &Icon, x: f32, y: f32, size: f32) {
    // dark outline so the icon reads on any portrait
    fill_rrect(img, x - 1.5, y - 1.5, size + 3.0, size + 3.0, (size + 3.0) / 2.0 * if matches!(icon, Icon::Face(_)) { 1.0 } else { 0.35 }, |_, _| ([10, 18, 34], 0.85));
    match icon {
        Icon::Face(level) => {
            let rgb = face_color(*level);
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
        other => {
            let rgb = tone(other).rgb();
            fill_rrect(img, x, y, size, size, size * 0.22, |_, py| (rgb, 1.0 - 0.2 * py / size));
            if let Some(g) = glyph(other) {
                let px = size * 0.72;
                let w = text_width(&fonts.bold, px, g);
                draw_text(img, &fonts.bold, px, x + (size - w) / 2.0, y + size * 0.1, TEXT, 1.0, g);
            }
        }
    }
}

/// Width of a row of `n` icons in the 1280x960 frame.
fn row_width(n: usize) -> f32 {
    n as f32 * ICON + n.saturating_sub(1) as f32 * GAP
}

/// Render the marks onto a transparent layer of `client_w` x `client_h` pixels.
pub fn render_marks(client_w: u32, client_h: u32, marks: &[Mark], fonts: &Fonts) -> RgbaImage {
    let mut img = RgbaImage::from_pixel(client_w, client_h, Rgba([0, 0, 0, 0]));
    let k = client_w as f32 / 1280.0;
    for m in marks {
        let size = ICON * k;
        let total = row_width(m.icons.len()) * k;
        let mut x = m.cx as f32 * k - total / 2.0;
        let y = m.cy as f32 * k - size / 2.0;
        for icon in &m.icons {
            draw_icon(&mut img, fonts, icon, x, y, size);
            x += (ICON + GAP) * k;
        }
    }
    img
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::load_fonts;

    fn sample() -> Vec<Mark> {
        let strip = |level: u8| vec![Icon::Face(level), Icon::Attack("Explosion".into()), Icon::Armor("HeavyArmor".into())];
        (0..5).map(|i| Mark { cx: 160 + 220 * i, cy: 120, icons: strip(i as u8) }).collect()
    }

    #[test]
    fn marks_are_drawn_where_asked_and_nowhere_else() {
        let Some(f) = load_fonts() else { return };
        for (w, h) in [(458u32, 344u32), (1280, 960)] {
            let img = render_marks(w, h, &sample(), &f);
            assert_eq!((img.width(), img.height()), (w, h));
            let k = w as f32 / 1280.0;
            let drawn = |x: f32, y: f32| img.get_pixel((x * k) as u32, (y * k) as u32)[3] > 0;
            assert!(drawn(160.0 - 20.0, 120.0), "the face of the first strip, left of its centre");
            assert!(!drawn(160.0, 500.0), "empty space stays transparent");
            assert!(!drawn(5.0, 5.0));
            if let Some(dir) = std::env::var_os("BANNER_PREVIEW_DIR") {
                let bg = RgbaImage::from_pixel(w, h, Rgba([150, 170, 200, 255]));
                let mut out = bg;
                image::imageops::overlay(&mut out, &img, 0, 0);
                out.save(std::path::Path::new(&dir).join(format!("marks-{w}.png"))).unwrap();
            }
        }
    }

    #[test]
    fn faces_differ_by_level_and_types_by_kind() {
        let Some(f) = load_fonts() else { return };
        let one = |icon: Icon| render_marks(100, 100, &[Mark { cx: 640, cy: 480, icons: vec![icon] }], &f);
        let k = 100.0 / 1280.0;
        let at = |img: &RgbaImage| *img.get_pixel((640.0 * k) as u32, (468.0 * k) as u32);
        assert_ne!(at(&one(Icon::Face(4))), at(&one(Icon::Face(0))));
        assert_ne!(at(&one(Icon::Attack("Explosion".into()))), at(&one(Icon::Armor("Unarmed".into()))));
    }
}
