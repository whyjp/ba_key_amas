//! Icons drawn over the game itself (on student portraits). The layer covers the game's
//! client area; positions are given in the 1280x960 frame the rest of the program works in
//! and scaled to the window, so any window size gets the same placement.
use crate::panel::Tone;
use crate::render::{draw_text, fill_rrect, text_width, Fonts, MUTED, NAVY, TEXT, WHITE};
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
    /// A name tag (a student's name): flat, as wide as its text. `muted` is for a count such as "+2".
    Tag { text: String, muted: bool },
}

/// A row (or column) of icons centred on (`cx`, `cy`) in the 1280x960 frame.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Mark {
    pub cx: i32,
    pub cy: i32,
    pub icons: Vec<Icon>,
    /// Stacked top to bottom instead of left to right.
    pub vertical: bool,
    /// `cx` is the left edge, not the middle (name tags start at their anchor).
    pub left: bool,
    /// Widest a tag may be (frame pixels); longer text is cut with "…". 0 = no limit.
    pub max_w: i32,
}

impl Mark {
    pub fn row(cx: i32, cy: i32, icons: Vec<Icon>) -> Mark {
        Mark { cx, cy, icons, vertical: false, left: false, max_w: 0 }
    }

    /// A column whose first icon starts at `top` and whose middle is at `cx`.
    pub fn column_from_top(cx: i32, top: i32, icons: Vec<Icon>) -> Mark {
        let height = column_height(&icons).round() as i32;
        Mark { cx, cy: top + height / 2, icons, vertical: true, left: false, max_w: 0 }
    }

    /// Name tags one under the other, starting at (`x_left`, `y_top`), none wider than `max_w`.
    pub fn tag_column(x_left: i32, y_top: i32, max_w: i32, tags: Vec<(String, bool)>) -> Mark {
        let icons: Vec<Icon> = tags.into_iter().map(|(text, muted)| Icon::Tag { text, muted }).collect();
        let height = column_height(&icons).round() as i32;
        Mark { cx: x_left, cy: y_top + height / 2, icons, vertical: true, left: true, max_w }
    }

    /// Where the column's first icon starts (frame pixels).
    pub fn top(&self) -> i32 {
        self.cy - (column_height(&self.icons).round() as i32) / 2
    }
}

/// Icon size in the 1280x960 frame.
const ICON: f32 = 20.0;
const GAP: f32 = 2.0;
/// Height of a name tag and the space inside it, left and right.
const TAG_H: f32 = 20.0;
const TAG_PAD: f32 = 6.0;

fn icon_height(icon: &Icon) -> f32 {
    if matches!(icon, Icon::Tag { .. }) {
        TAG_H
    } else {
        ICON
    }
}

/// Height of icons stacked one under the other.
fn column_height(icons: &[Icon]) -> f32 {
    icons.iter().map(icon_height).sum::<f32>() + icons.len().saturating_sub(1) as f32 * GAP
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

/// `text` cut with "…" so that it is at most `max` pixels wide at `px`.
fn fit_text(font: &ab_glyph::FontVec, px: f32, max: f32, text: &str) -> String {
    if text_width(font, px, text) <= max {
        return text.to_string();
    }
    let mut chars: Vec<char> = text.chars().collect();
    while !chars.is_empty() && text_width(font, px, &format!("{}…", chars.iter().collect::<String>())) > max {
        chars.pop();
    }
    format!("{}…", chars.iter().collect::<String>())
}

/// One name tag with its top-left corner at (x, y); returns its width. `k` scales frame pixels to the window.
fn draw_tag(img: &mut RgbaImage, fonts: &Fonts, text: &str, muted: bool, x: f32, y: f32, k: f32, max_w: f32) -> f32 {
    let px = 13.0 * k;
    let pad = TAG_PAD * k;
    let limit = if max_w > 0.0 { max_w * k } else { f32::MAX };
    let shown = fit_text(&fonts.bold, px, (limit - 2.0 * pad).max(px), text);
    let w = (text_width(&fonts.bold, px, &shown) + 2.0 * pad).min(limit);
    let h = TAG_H * k;
    let fill = if muted { MUTED } else { NAVY };
    fill_rrect(img, x, y, w, h, 3.0 * k, |_, _| (fill, 0.96));
    fill_rrect(img, x, y, w, h, 3.0 * k, |lx, ly| {
        let edge = lx < k || ly < k || lx > w - k || ly > h - k;
        (WHITE, if edge { 0.35 } else { 0.0 })
    });
    draw_text(img, &fonts.bold, px, x + pad, y + (h - px) / 2.0 - 0.5 * k, TEXT, 1.0, &shown);
    w
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
        Icon::Face(_) | Icon::Tag { .. } => None,
    }
}

fn tone(icon: &Icon) -> Tone {
    match icon {
        Icon::Attack(c) | Icon::Armor(c) => Tone::from_code(c),
        Icon::Face(_) | Icon::Tag { .. } => Tone::Neutral,
    }
}

/// Draw one icon with its top-left at (x, y), `size` px square.
fn draw_icon(img: &mut RgbaImage, fonts: &Fonts, icon: &Icon, x: f32, y: f32, size: f32) {
    if let Icon::Tag { .. } = icon {
        return; // name tags are drawn by `draw_tag`, which needs the frame scale
    }
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
        let total = if m.vertical { column_height(&m.icons) * k } else { row_width(m.icons.len()) * k };
        let (mut x, mut y) = match (m.vertical, m.left) {
            (true, true) => (m.cx as f32 * k, m.top() as f32 * k),
            (true, false) => (m.cx as f32 * k - size / 2.0, m.top() as f32 * k),
            (false, _) => (m.cx as f32 * k - total / 2.0, m.cy as f32 * k - size / 2.0),
        };
        for icon in &m.icons {
            match icon {
                Icon::Tag { text, muted } => {
                    draw_tag(&mut img, fonts, text, *muted, x, y, k, m.max_w as f32);
                }
                _ => draw_icon(&mut img, fonts, icon, x, y, size),
            }
            if m.vertical {
                y += (icon_height(icon) + GAP) * k;
            } else {
                x += (ICON + GAP) * k;
            }
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
        (0..5).map(|i| if i % 2 == 0 { Mark::row(160 + 220 * i, 120, strip(i as u8)) } else { Mark::column_from_top(160 + 220 * i, 90, strip(i as u8)) }).collect()
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
    fn a_column_stacks_downward_from_its_top_and_a_row_does_not() {
        let Some(f) = load_fonts() else { return };
        let icons = vec![Icon::Face(4), Icon::Attack("Explosion".into()), Icon::Armor("HeavyArmor".into())];
        let col = Mark::column_from_top(300, 200, icons.clone());
        assert_eq!(col.top(), 200);
        let img = render_marks(1280, 960, &[col.clone()], &f);
        let drawn = |x: u32, y: u32| img.get_pixel(x, y)[3] > 0;
        assert!(drawn(300, 210) && drawn(300, 232) && drawn(300, 254), "three icons one below the other");
        assert!(!drawn(300, 190) && !drawn(300, 275), "nothing above the top or below the stack");
        assert!(!drawn(280, 232) && !drawn(322, 232), "one icon wide");
    }

    #[test]
    fn name_tags_stack_from_the_left_edge_and_long_names_are_cut_to_the_card_width() {
        let Some(f) = load_fonts() else { return };
        let tags = vec![("호시노".to_string(), false), ("아주아주아주긴이름의학생".to_string(), false), ("+2".to_string(), true)];
        let m = Mark::tag_column(100, 200, 90, tags);
        assert_eq!(m.top(), 200);
        let img = render_marks(1280, 960, &[m], &f);
        let drawn = |x: u32, y: u32| img.get_pixel(x, y)[3] > 0;
        // three tags of 20 px with 2 px between
        assert!(drawn(103, 208) && drawn(103, 230) && drawn(103, 252), "one tag under the other");
        assert!(!drawn(103, 221) && !drawn(103, 243), "gaps between them");
        assert!(!drawn(96, 208), "nothing left of the anchor");
        let width_at = |y: u32| (0..400).filter(|x| drawn(*x, y)).count();
        assert!(width_at(208) < 60, "a short name makes a short tag: {}", width_at(208));
        assert!(width_at(230) <= 90, "a long name is cut to the card width: {}", width_at(230));
        assert!(width_at(230) > 80, "and fills it: {}", width_at(230));
        if let Some(dir) = std::env::var_os("BANNER_PREVIEW_DIR") {
            // what one, three and six students look like on cards of 150 px
            let set = |n: usize| -> Vec<(String, bool)> {
                let names = ["호시노(무장)", "시로코*테러", "아코", "나구사(수영복)", "하나코", "미카"];
                if n <= 3 {
                    names.iter().take(n).map(|s| (s.to_string(), false)).collect()
                } else {
                    let mut v: Vec<(String, bool)> = names.iter().take(2).map(|s| (s.to_string(), false)).collect();
                    v.push((format!("+{}", n - 2), true));
                    v
                }
            };
            let marks: Vec<Mark> = [1usize, 3, 6].iter().enumerate().map(|(i, n)| Mark::tag_column(40 + 190 * i as i32, 40, 150, set(*n))).collect();
            let mut base = RgbaImage::from_pixel(640, 140, Rgba([226, 233, 244, 255]));
            for i in 0..3 {
                for y in 30..130 {
                    for x in (30 + 190 * i)..(190 + 190 * i) {
                        base.put_pixel(x, y, Rgba([252, 252, 253, 255]));
                    }
                }
            }
            image::imageops::overlay(&mut base, &render_marks(640, 140, &marks, &f), 0, 0);
            base.save(std::path::Path::new(&dir).join("tags.png")).unwrap();
        }
    }

    #[test]
    fn faces_differ_by_level_and_types_by_kind() {
        let Some(f) = load_fonts() else { return };
        let one = |icon: Icon| render_marks(100, 100, &[Mark::row(640, 480, vec![icon])], &f);
        let k = 100.0 / 1280.0;
        let at = |img: &RgbaImage| *img.get_pixel((640.0 * k) as u32, (468.0 * k) as u32);
        assert_ne!(at(&one(Icon::Face(4))), at(&one(Icon::Face(0))));
        assert_ne!(at(&one(Icon::Attack("Explosion".into()))), at(&one(Icon::Armor("Unarmed".into()))));
    }
}
