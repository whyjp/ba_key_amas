//! The guide panel: a card with the current screen and, on mission screens, what the
//! mission holds. It sits beside the game window (right, else left), never on it.
//! Pure pixels in, pixels out, like the banner, so the look is checked without a game.
use crate::render::{card, draw_text, fill_rrect, text_width, Fonts, Ltrb, AMBER, BODY, BORDER, INK, MUTED, NAVY, NAVY_SOFT, RULE, TRACK, WHITE};
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
    Mid,
    Bad,
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

    /// Flat, solid colours of the game's attack and armor tags (white text on all of them).
    pub(crate) fn rgb(self) -> [u8; 3] {
        match self {
            Tone::Neutral => [106, 118, 140],
            Tone::Explosion | Tone::LightArmor => [188, 52, 56],
            Tone::Pierce | Tone::HeavyArmor => [184, 130, 14],
            Tone::Mystic | Tone::Unarmed => [36, 108, 186],
            Tone::Sonic | Tone::ElasticArmor => [124, 74, 176],
            Tone::CompositeArmor => [52, 138, 98],
            Tone::Good => [34, 134, 82],
            Tone::Mid => [184, 130, 14],
            Tone::Bad => [196, 58, 58],
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
    /// A bar around the average: `permille` is 1000 at average, above is better. Drawn on
    /// a scale of 500..1500; `text` is shown beside the bar.
    Meter { label: String, permille: i32, text: String },
    /// The verdict as a wide badge.
    Verdict { label: String, text: String, tone: Tone },
    /// A clickable row; clicking it reports `id` through the overlay's events.
    Button { id: String, label: String, value: String, on: bool },
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PanelState {
    /// Where the player is, e.g. "편성".
    pub scene: String,
    /// Main line, e.g. "19-3 설원 지구 제2 지역". Empty on screens without one.
    pub title: String,
    pub rows: Vec<Row>,
}

/// The id of the header's hit area: pressing it starts a drag instead of reporting a click.
pub const DRAG_HANDLE: &str = "drag";

/// A clickable area of a rendered panel, in the panel image's pixels.
#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    pub id: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Hit {
    pub fn contains(&self, px: i32, py: i32) -> bool {
        let (px, py) = (px as f32, py as f32);
        px >= self.x && px < self.x + self.w && py >= self.y && py < self.y + self.h
    }
}

/// Panel width in screen pixels for a game client of `client_w` pixels.
pub fn panel_width(client_w: i32) -> i32 {
    ((client_w as f32 * WIDTH_SHARE).round() as i32).clamp(MIN_W, MAX_W)
}

/// Width of a panel drawn inside the game, narrower than the one beside it.
pub fn ingame_panel_width(client_w: i32) -> i32 {
    ((client_w as f32 * 0.46).round() as i32).clamp(190, 360)
}

/// A panel inside the game: at the top right of its client area, below the game's header bar.
pub fn ingame_origin(client: Ltrb, size: (i32, i32)) -> (i32, i32) {
    let (cw, ch) = (client.2 - client.0, client.3 - client.1);
    let margin = ((cw as f32 * 0.012).round() as i32).max(4);
    let top = client.1 + (ch as f32 * 0.075).round() as i32;
    let x = (client.2 - size.0 - margin).max(client.0);
    let y = top.min(client.3 - size.1).max(client.1);
    (x, y)
}

/// Where the panel goes in screen coordinates: beside the game window on the right,
/// else on the left, aligned with the top of the game. None when neither side has
/// room: the panel is then not shown rather than drawn over the game.
pub fn panel_origin(client: Ltrb, outer: Ltrb, work: Ltrb, size: (i32, i32)) -> Option<(i32, i32)> {
    crate::dock::dock_origin(client, outer, work, size, crate::dock::Dock::default())
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
    render_panel_hits(width, state, fonts).0
}

const HEAD_H: f32 = 26.0;

/// Like `render_panel`, also returning where its buttons are.
///
/// Look: one flat opaque card (pale body, 1px border, 5px corners) under a navy header bar
/// with a single cut corner, like the game's own panels. Type carries the hierarchy; colour
/// appears only on type tags, the verdict, meter fills and the active button.
pub fn render_panel_hits(width: u32, state: &PanelState, fonts: &Fonts) -> (RgbaImage, Vec<Hit>) {
    let mut hits: Vec<Hit> = Vec::new();
    let s = width as f32 / PANEL_BASE_W;
    let wf = width as f32;
    let (pad, label_w) = (PAD * s, LABEL_W * s);
    let value_w = wf - pad * 2.0 - label_w;

    // first pass: height
    let mut h = HEAD_H * s + 10.0 * s;
    if !state.title.is_empty() {
        h += wrap(&fonts.bold, 17.0 * s, wf - pad * 2.0, &state.title).len() as f32 * 23.0 * s + 6.0 * s;
    }
    h += 6.0 * s;
    for row in &state.rows {
        h += match row {
            Row::Info { value, .. } => wrap(&fonts.regular, 14.0 * s, value_w, value).len() as f32 * 21.0 * s + 5.0 * s,
            Row::Chips { chips, .. } => chip_layout(fonts, 12.0 * s, s, value_w, chips).0 as f32 * 25.0 * s + 5.0 * s,
            Row::Note(t) => wrap(&fonts.regular, 12.5 * s, wf - pad * 2.0, t).len() as f32 * 18.0 * s + 5.0 * s,
            Row::Meter { .. } => 22.0 * s + 5.0 * s,
            Row::Verdict { .. } => 28.0 * s + 7.0 * s,
            Row::Button { .. } => 24.0 * s + 5.0 * s,
        };
    }
    h += 8.0 * s;
    let hi = h.ceil() as u32;
    let hf = hi as f32;

    let mut img = RgbaImage::from_pixel(width, hi, Rgba([0, 0, 0, 0]));
    let head = HEAD_H * s;
    card(&mut img, wf, hf, s, head, 12.0 * s, BODY);
    // the header bar is where the card is picked up and moved
    hits.push(Hit { id: DRAG_HANDLE.into(), x: 0.0, y: 0.0, w: wf, h: head });

    // header bar: screen name left, tool name right (before the cut corner)
    let scene = if state.scene.is_empty() { "ba-scanner" } else { state.scene.as_str() };
    draw_text(&mut img, &fonts.bold, 12.5 * s, pad, (head - 12.5 * s * 1.2) / 2.0, WHITE, 1.0, scene);
    if !state.scene.is_empty() {
        let name = "ba-scanner";
        let nw = text_width(&fonts.regular, 10.5 * s, name);
        draw_text(&mut img, &fonts.regular, 10.5 * s, wf - 20.0 * s - nw, (head - 10.5 * s * 1.2) / 2.0, NAVY_SOFT, 1.0, name);
    }

    let mut y = head + 10.0 * s;
    if !state.title.is_empty() {
        for line in wrap(&fonts.bold, 17.0 * s, wf - pad * 2.0, &state.title) {
            draw_text(&mut img, &fonts.bold, 17.0 * s, pad, y, INK, 1.0, &line);
            y += 23.0 * s;
        }
        y += 6.0 * s;
    }
    y += 6.0 * s;
    let rule = |img: &mut RgbaImage, y: f32| fill_rrect(img, pad, y, wf - pad * 2.0, (1.0 * s).max(1.0), 0.0, |_, _| (RULE, 1.0));
    let n = state.rows.len();
    for (i, row) in state.rows.iter().enumerate() {
        let more = i + 1 < n;
        match row {
            Row::Info { label, value } => {
                draw_text(&mut img, &fonts.regular, 12.5 * s, pad, y + 2.0 * s, MUTED, 1.0, label);
                for line in wrap(&fonts.regular, 14.0 * s, value_w, value) {
                    draw_text(&mut img, &fonts.semibold, 14.0 * s, pad + label_w, y, INK, 1.0, &line);
                    y += 21.0 * s;
                }
                y += 5.0 * s;
                if more {
                    rule(&mut img, y - 3.0 * s);
                }
            }
            Row::Chips { label, chips } => {
                draw_text(&mut img, &fonts.regular, 12.5 * s, pad, y + 4.0 * s, MUTED, 1.0, label);
                let (lines, places) = chip_layout(fonts, 12.0 * s, s, value_w, chips);
                for (chip, (line, x, w)) in chips.iter().zip(places) {
                    let (cx, cy) = (pad + label_w + x, y + line as f32 * 25.0 * s);
                    let rgb = chip.tone.rgb();
                    fill_rrect(&mut img, cx, cy, w, 21.0 * s, 3.0 * s, |_, _| (rgb, 1.0));
                    draw_text(&mut img, &fonts.bold, 12.0 * s, cx + 8.0 * s, cy + (21.0 - 12.0 * 1.2) * 0.5 * s, WHITE, 1.0, &chip.text);
                }
                y += lines as f32 * 25.0 * s + 5.0 * s;
                if more {
                    rule(&mut img, y - 3.0 * s);
                }
            }
            Row::Note(t) => {
                for line in wrap(&fonts.regular, 12.5 * s, wf - pad * 2.0, t) {
                    draw_text(&mut img, &fonts.regular, 12.5 * s, pad, y, AMBER, 1.0, &line);
                    y += 18.0 * s;
                }
                y += 5.0 * s;
            }
            Row::Meter { label, permille, text } => {
                draw_text(&mut img, &fonts.regular, 12.5 * s, pad, y + 2.0 * s, MUTED, 1.0, label);
                let (bx, bw, bh) = (pad + label_w, value_w - 50.0 * s, 6.0 * s);
                let by = y + 7.0 * s;
                fill_rrect(&mut img, bx, by, bw, bh, 1.0 * s, |_, _| (TRACK, 1.0));
                let frac = ((*permille).clamp(500, 1500) as f32 - 500.0) / 1000.0;
                let (mid, pos) = (bx + bw / 2.0, bx + bw * frac);
                let (from, to) = if pos >= mid { (mid, pos) } else { (pos, mid) };
                let rgb = if (*permille - 1000).abs() < 30 { MUTED } else if *permille > 1000 { Tone::Good.rgb() } else { Tone::Bad.rgb() };
                if to - from > 1.0 {
                    fill_rrect(&mut img, from, by, to - from, bh, 1.0 * s, |_, _| (rgb, 1.0));
                }
                // the average mark stands taller than the bar
                fill_rrect(&mut img, mid - 0.75 * s, by - 3.0 * s, 1.5 * s, bh + 6.0 * s, 0.0, |_, _| (NAVY, 1.0));
                let tw = text_width(&fonts.semibold, 12.5 * s, text);
                draw_text(&mut img, &fonts.semibold, 12.5 * s, wf - pad - tw, y + 2.0 * s, INK, 1.0, text);
                y += 22.0 * s + 5.0 * s;
                if more {
                    rule(&mut img, y - 3.0 * s);
                }
            }
            Row::Button { id, label, value, on } => {
                let (bx, bw, bh) = (pad, wf - pad * 2.0, 24.0 * s);
                let (fill, ink, vink) = if *on { (NAVY, WHITE, WHITE) } else { (WHITE, INK, MUTED) };
                let b = (1.0 * s).max(1.0);
                fill_rrect(&mut img, bx, y, bw, bh, 4.0 * s, |x, py| {
                    let edge = x < b || py < b || x > bw - b - 0.5 || py > bh - b - 0.5;
                    (if edge && !*on { BORDER } else { fill }, 1.0)
                });
                let ty = y + (24.0 - 12.5 * 1.2) * 0.5 * s;
                draw_text(&mut img, &fonts.regular, 12.5 * s, bx + 9.0 * s, ty, ink, 1.0, label);
                let vw = text_width(&fonts.bold, 12.5 * s, value);
                draw_text(&mut img, &fonts.bold, 12.5 * s, bx + bw - 9.0 * s - vw, ty, vink, 1.0, value);
                hits.push(Hit { id: id.clone(), x: bx, y, w: bw, h: bh });
                y += 24.0 * s + 5.0 * s;
            }
            Row::Verdict { label, text, tone } => {
                let (bx, bw, bh) = (pad, wf - pad * 2.0, 28.0 * s);
                let rgb = tone.rgb();
                fill_rrect(&mut img, bx, y, bw, bh, 3.0 * s, |_, _| (rgb, 1.0));
                draw_text(&mut img, &fonts.regular, 12.5 * s, bx + 10.0 * s, y + (28.0 - 12.5 * 1.2) * 0.5 * s, WHITE, 0.9, label);
                let tw = text_width(&fonts.bold, 15.0 * s, text);
                draw_text(&mut img, &fonts.bold, 15.0 * s, bx + bw - 10.0 * s - tw, y + (28.0 - 15.0 * 1.2) * 0.5 * s, WHITE, 1.0, text);
                y += 28.0 * s + 7.0 * s;
            }
        }
    }
    (img, hits)
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
                Row::Chips {
                    label: "속성".into(),
                    chips: vec![
                        Chip { text: "폭발".into(), tone: Tone::Explosion },
                        Chip { text: "관통".into(), tone: Tone::Pierce },
                        Chip { text: "신비".into(), tone: Tone::Mystic },
                        Chip { text: "진동".into(), tone: Tone::Sonic },
                        Chip { text: "복합장갑".into(), tone: Tone::CompositeArmor },
                        Chip { text: "일반".into(), tone: Tone::Neutral },
                    ],
                },
                Row::Note("Hard 난이도는 적 구성이 다를 수 있습니다. 편성 전에 각 지역의 적 장갑과 지형을 다시 확인하세요".into()),
                Row::Verdict { label: "편성 판정".into(), text: "유리".into(), tone: Tone::Good },
                Row::Meter { label: "공격 상성".into(), permille: 1420, text: "+42%".into() },
                Row::Meter { label: "방어 상성".into(), permille: 870, text: "-13%".into() },
                Row::Meter { label: "지형 적응".into(), permille: 1000, text: "평균".into() },
                Row::Verdict { label: "방어 판정".into(), text: "불리".into(), tone: Tone::Bad },
                Row::Verdict { label: "지형 판정".into(), text: "보통".into(), tone: Tone::Mid },
                Row::Button { id: "ingame".into(), label: "인게임 표시".into(), value: "켜짐".into(), on: true },
                Row::Button { id: "place".into(), label: "임무 카드".into(), value: "게임 밖 · 100%".into(), on: false },
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
    fn buttons_report_where_they_are_and_what_they_are_called() {
        let Some(f) = load_fonts() else { return };
        let state = PanelState {
            scene: "메뉴".into(),
            title: String::new(),
            rows: vec![
                Row::Button { id: "ingame".into(), label: "인게임 표시".into(), value: "켜짐".into(), on: true },
                Row::Info { label: "지형".into(), value: "야외".into() },
                Row::Button { id: "place:mission".into(), label: "임무 카드".into(), value: "게임 밖".into(), on: false },
            ],
        };
        let (img, hits) = render_panel_hits(300, &state, &f);
        assert_eq!(hits[0].id, DRAG_HANDLE, "the header is the drag handle");
        assert!(hits[0].y == 0.0 && hits[0].h < 40.0 && hits[0].w == img.width() as f32);
        let hits: Vec<Hit> = hits.into_iter().skip(1).collect();
        assert_eq!(hits.iter().map(|h| h.id.as_str()).collect::<Vec<_>>(), vec!["ingame", "place:mission"]);
        let first = &hits[0];
        assert!(first.contains((first.x + 5.0) as i32, (first.y + 5.0) as i32) && !first.contains(1, 1));
        assert!(hits[1].y > first.y + first.h, "the second button is below the first");
        assert!(hits.iter().all(|h| h.x + h.w <= img.width() as f32 && h.y + h.h <= img.height() as f32), "buttons are inside the card");
    }

    #[test]
    fn the_ingame_panel_sits_inside_the_game_below_its_header() {
        let client = (100, 50, 560, 400);
        let size = (200, 160);
        let (x, y) = ingame_origin(client, size);
        assert!(x >= client.0 && x + size.0 <= client.2 && y + size.1 <= client.3, "inside the client area: {x},{y}");
        assert!(y > client.1 + 20, "below the header bar");
        // a panel taller than the room is pushed up, never out of the window
        let (_, y) = ingame_origin(client, (200, 340));
        assert!(y >= client.1 && y + 340 <= client.3 || y == client.1);
        assert_eq!(ingame_panel_width(458), 211);
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
