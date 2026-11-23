//! The guide panel: a card with the current screen and, on mission screens, what the
//! mission holds. It sits beside the game window (right, else left), never on it.
//! Pure pixels in, pixels out, like the banner, so the look is checked without a game.
use crate::render::{draw_text, fill_rrect, lerp, text_width, Fonts, Ltrb, ACCENT, ACCENT_LIGHT, NAVY_BOTTOM, NAVY_TOP, TEXT, TEXT_SOFT, WARN};
use image::{Rgba, RgbaImage};

/// Design width; the panel is scaled to the game window.
pub const PANEL_BASE_W: f32 = 300.0;
/// Share of the game window's width the panel takes, within these bounds (screen pixels).
const WIDTH_SHARE: f32 = 0.68;
const MIN_W: i32 = 230;
const MAX_W: i32 = 420;

/// Colour family of a badge; the colours follow the game's own attack and armor icons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Neutral,
    Explosion,
    Pierce,
    Mystic,
    Sonic,
    LightArmor,
    HeavyArmor,
    Unarmed,
    ElasticArmor,
    CompositeArmor,
    Good,
}

impl Tone {
    /// Tone of a SchaleDB bullet or armor type code; Neutral for anything else.
    pub fn from_code(code: &str) -> Tone {
        match code {
            "Explosion" => Tone::Explosion,
            "Pierce" => Tone::Pierce,
            "Mystic" => Tone::Mystic,
            "Sonic" => Tone::Sonic,
            "LightArmor" => Tone::LightArmor,
            "HeavyArmor" => Tone::HeavyArmor,
            "Unarmed" => Tone::Unarmed,
            "ElasticArmor" => Tone::ElasticArmor,
            "CompositeArmor" => Tone::CompositeArmor,
            _ => Tone::Neutral,
        }
    }

    fn rgb(self) -> [u8; 3] {
        match self {
            Tone::Neutral => [96, 112, 140],
            Tone::Explosion | Tone::LightArmor => [178, 58, 62],
            Tone::Pierce | Tone::HeavyArmor => [196, 146, 48],
            Tone::Mystic | Tone::Unarmed => [48, 120, 184],
            Tone::Sonic | Tone::ElasticArmor => [132, 84, 184],
            Tone::CompositeArmor => [78, 150, 110],
            Tone::Good => [46, 150, 96],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chip {
    pub text: String,
    pub tone: Tone,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row {
    /// "지형   야외"
    Info { label: String, value: String },
    /// "적 방어   [중장갑 6] [특수장갑 3]"
    Chips { label: String, chips: Vec<Chip> },
    /// A plain line (wrapped), e.g. a hint or a warning.
    Note(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PanelState {
    /// Where the player is, e.g. "편성".
    pub scene: String,
    /// Main line, e.g. "19-3 설원 지구 제2 지역". Empty on screens without one.
    pub title: String,
    pub rows: Vec<Row>,
}

/// Panel width in screen pixels for a game client of `client_w` pixels.
pub fn panel_width(client_w: i32) -> i32 {
    ((client_w as f32 * WIDTH_SHARE).round() as i32).clamp(MIN_W, MAX_W)
}

/// Where the panel goes in screen coordinates: beside the game window on the right,
/// else on the left, aligned with the top of the game. None when neither side has
/// room: the panel is then not shown rather than drawn over the game.
pub fn panel_origin(client: Ltrb, outer: Ltrb, work: Ltrb, size: (i32, i32)) -> Option<(i32, i32)> {
    let (pw, ph) = size;
    let gap = (((client.3 - client.1) as f32 * 0.012).round() as i32).max(6);
    let y = client.1.clamp(work.1, (work.3 - ph).max(work.1));
    let right = outer.2 + gap;
    if right + pw <= work.2 {
        return Some((right, y));
    }
    let left = outer.0 - gap - pw;
    if left >= work.0 {
        return Some((left, y));
    }
    None
}

const LABEL_W: f32 = 62.0;
const PAD: f32 = 16.0;

/// Wrap `s` into lines no wider than `max_w` px at text size `px`.
fn wrap(font: &ab_glyph::FontVec, px: f32, max_w: f32, s: &str) -> Vec<String> {
    let mut lines = vec![String::new()];
    for word in s.split_whitespace() {
        let cur = lines.last_mut().unwrap();
        let candidate = if cur.is_empty() { word.to_string() } else { format!("{cur} {word}") };
        if text_width(font, px, &candidate) <= max_w || cur.is_empty() {
            *cur = candidate;
        } else {
            lines.push(word.to_string());
        }
    }
    lines
}

/// Chip rows: how many lines the chips take at `max_w`, and each chip's (line, x offset, width).
fn chip_layout(fonts: &Fonts, px: f32, s: f32, max_w: f32, chips: &[Chip]) -> (usize, Vec<(usize, f32, f32)>) {
    let (pad, gap) = (8.0 * s, 5.0 * s);
    let (mut line, mut x) = (0usize, 0.0f32);
    let mut out = Vec::new();
    for c in chips {
        let w = text_width(&fonts.bold, px, &c.text) + pad * 2.0;
        if x > 0.0 && x + w > max_w {
            line += 1;
            x = 0.0;
        }
        out.push((line, x, w));
        x += w + gap;
    }
    (line + 1, out)
}

/// Render the panel at `width` px; the height follows its content.
pub fn render_panel(width: u32, state: &PanelState, fonts: &Fonts) -> RgbaImage {
    let s = width as f32 / PANEL_BASE_W;
    let wf = width as f32;
    let (pad, label_w) = (PAD * s, LABEL_W * s);
    let value_w = wf - pad * 2.0 - label_w;

    // first pass: height
    let mut h = 14.0 * s + 16.0 * s; // top pad + scene line
    if !state.title.is_empty() {
        h += 6.0 * s + wrap(&fonts.bold, 17.0 * s, wf - pad * 2.0, &state.title).len() as f32 * 23.0 * s;
    }
    h += 10.0 * s;
    for row in &state.rows {
        h += match row {
            Row::Info { value, .. } => wrap(&fonts.regular, 14.0 * s, value_w, value).len() as f32 * 21.0 * s + 5.0 * s,
            Row::Chips { chips, .. } => chip_layout(fonts, 12.0 * s, s, value_w, chips).0 as f32 * 25.0 * s + 5.0 * s,
            Row::Note(t) => wrap(&fonts.regular, 12.5 * s, wf - pad * 2.0, t).len() as f32 * 18.0 * s + 5.0 * s,
        };
    }
    h += 12.0 * s;
    let hi = h.ceil() as u32;
    let hf = hi as f32;

    let mut img = RgbaImage::from_pixel(width, hi, Rgba([0, 0, 0, 0]));
    fill_rrect(&mut img, 0.0, 0.0, wf, hf, 14.0 * s, |_, y| (lerp(NAVY_TOP, NAVY_BOTTOM, y / hf), 0.93));
    fill_rrect(&mut img, 0.0, 0.0, wf, hf, 14.0 * s, |x, y| {
        let edge = x < 1.2 * s || y < 1.2 * s || x > wf - 1.2 * s || y > hf - 1.2 * s;
        (TEXT, if edge { 0.16 } else { 0.0 })
    });
    fill_rrect(&mut img, 8.0 * s, 14.0 * s, 4.0 * s, hf - 28.0 * s, 2.0 * s, |_, _| (ACCENT, 1.0));

    let mut y = 14.0 * s;
    draw_text(&mut img, &fonts.bold, 12.5 * s, pad, y, ACCENT_LIGHT, 1.0, &format!("ba-scanner · {}", state.scene));
    y += 16.0 * s;
    if !state.title.is_empty() {
        y += 6.0 * s;
        for line in wrap(&fonts.bold, 17.0 * s, wf - pad * 2.0, &state.title) {
            draw_text(&mut img, &fonts.bold, 17.0 * s, pad, y, TEXT, 1.0, &line);
            y += 23.0 * s;
        }
    }
    y += 10.0 * s;
    for row in &state.rows {
        match row {
            Row::Info { label, value } => {
                draw_text(&mut img, &fonts.regular, 12.5 * s, pad, y + 1.5 * s, TEXT_SOFT, 0.9, label);
                for line in wrap(&fonts.regular, 14.0 * s, value_w, value) {
                    draw_text(&mut img, &fonts.bold, 14.0 * s, pad + label_w, y, TEXT, 1.0, &line);
                    y += 21.0 * s;
                }
                y += 5.0 * s;
            }
            Row::Chips { label, chips } => {
                draw_text(&mut img, &fonts.regular, 12.5 * s, pad, y + 4.0 * s, TEXT_SOFT, 0.9, label);
                let (lines, places) = chip_layout(fonts, 12.0 * s, s, value_w, chips);
                for (chip, (line, x, w)) in chips.iter().zip(places) {
                    let (cx, cy) = (pad + label_w + x, y + line as f32 * 25.0 * s);
                    let rgb = chip.tone.rgb();
                    fill_rrect(&mut img, cx, cy, w, 21.0 * s, 6.0 * s, |_, py| (lerp(rgb, [0, 0, 0], py / (21.0 * s) * 0.25), 0.95));
                    draw_text(&mut img, &fonts.bold, 12.0 * s, cx + 8.0 * s, cy + 3.0 * s, TEXT, 1.0, &chip.text);
                }
                y += lines as f32 * 25.0 * s + 5.0 * s;
            }
            Row::Note(t) => {
                for line in wrap(&fonts.regular, 12.5 * s, wf - pad * 2.0, t) {
                    draw_text(&mut img, &fonts.regular, 12.5 * s, pad, y, WARN, 0.95, &line);
                    y += 18.0 * s;
                }
                y += 5.0 * s;
            }
        }
    }
    img
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::load_fonts;

    fn sample() -> PanelState {
        PanelState {
            scene: "편성".into(),
            title: "19-3 설원 지구 제2 지역".into(),
            rows: vec![
                Row::Info { label: "지형".into(), value: "야외".into() },
                Row::Info { label: "추천 Lv".into(), value: "85".into() },
                Row::Chips { label: "적 공격".into(), chips: vec![Chip { text: "일반 31".into(), tone: Tone::Neutral }] },
                Row::Chips {
                    label: "적 방어".into(),
                    chips: vec![
                        Chip { text: "중장갑 14".into(), tone: Tone::HeavyArmor },
                        Chip { text: "특수장갑 12".into(), tone: Tone::Unarmed },
                        Chip { text: "경장갑 3".into(), tone: Tone::LightArmor },
                    ],
                },
                Row::Chips { label: "추천 공격".into(), chips: vec![Chip { text: "신비 135%".into(), tone: Tone::Mystic }, Chip { text: "관통 120%".into(), tone: Tone::Pierce }] },
                Row::Note("Hard 난이도는 적 구성이 다를 수 있습니다".into()),
            ],
        }
    }

    #[test]
    fn the_panel_renders_at_any_width_with_content_height() {
        let Some(f) = load_fonts() else { return };
        for w in [230u32, 300, 420] {
            let img = render_panel(w, &sample(), &f);
            assert_eq!(img.width(), w);
            assert!(img.height() > 120, "{w}: {}", img.height());
            assert_eq!(img.get_pixel(0, 0)[3], 0, "rounded corner is transparent");
            assert!(img.get_pixel(w / 2, img.height() / 2)[3] > 200);
            if let Some(dir) = std::env::var_os("BANNER_PREVIEW_DIR") {
                img.save(std::path::Path::new(&dir).join(format!("panel-{w}.png"))).unwrap();
            }
        }
        let bare = render_panel(300, &PanelState { scene: "메인 메뉴".into(), ..Default::default() }, &f);
        assert!(bare.height() < render_panel(300, &sample(), &f).height(), "less content, shorter card");
    }

    #[test]
    fn long_chip_rows_wrap_instead_of_overflowing() {
        let Some(f) = load_fonts() else { return };
        let chips: Vec<Chip> = (0..8).map(|i| Chip { text: format!("특수장갑 {i}"), tone: Tone::Unarmed }).collect();
        let (lines, places) = chip_layout(&f, 12.0, 1.0, 200.0, &chips);
        assert!(lines >= 3, "{lines}");
        assert!(places.iter().all(|(_, x, w)| x + w <= 200.0 + 0.01), "every chip stays inside the row");
    }

    #[test]
    fn the_panel_sits_beside_the_window_and_never_on_it() {
        let work = (0, 0, 1920, 1040);
        let size = (300, 400);
        // room on the right
        let client = (300, 100, 760, 450);
        let outer = (292, 70, 768, 458);
        let (x, y) = panel_origin(client, outer, work, size).unwrap();
        assert!(x >= outer.2 && x + size.0 <= work.2, "right of the window: {x}");
        assert_eq!(y, client.1, "top aligned with the game");
        // window at the right edge: left side
        let (x, _) = panel_origin((1500, 100, 1900, 400), (1492, 70, 1908, 408), work, size).unwrap();
        assert!(x + size.0 <= 1492, "left of the window: {x}");
        // a window filling the monitor: nowhere to put it
        assert_eq!(panel_origin((0, 31, 1920, 1040), (0, 0, 1920, 1040), work, size), None);
        // never below the work area
        let (_, y) = panel_origin((300, 900, 760, 1030), (292, 870, 768, 1038), work, size).unwrap();
        assert!(y + size.1 <= work.3, "{y}");
    }

    #[test]
    fn width_follows_the_window_within_bounds() {
        assert_eq!(panel_width(458), 311);
        assert_eq!(panel_width(200), MIN_W);
        assert_eq!(panel_width(1920), MAX_W);
    }

    #[test]
    fn tones_follow_the_games_colour_families() {
        assert_eq!(Tone::from_code("Explosion").rgb(), Tone::from_code("LightArmor").rgb());
        assert_eq!(Tone::from_code("Mystic").rgb(), Tone::from_code("Unarmed").rgb());
        assert_eq!(Tone::from_code("Whatever"), Tone::Neutral);
    }
}
