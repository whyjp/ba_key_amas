//! The guide panel: a card with the current screen and, on mission screens, what the
//! mission holds. It sits beside the game window (right, else left), never on it.
//! Pure pixels in, pixels out, like the banner, so the look is checked without a game.
use crate::render::{card, draw_text, fill_rrect, text_width, Fonts, Ltrb, AMBER, BLUE, BODY, BORDER, INK, MUTED, NAVY, NAVY_SOFT, RULE, TRACK, WHITE};
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
    /// Outcome ladder, best to worst: deep green, green, neutral slate, orange, red.
    Excellent,
    Good,
    Even,
    Poor,
    Bad,
}

/// How well a student suits the battle: the one scale behind the card's badges and the
/// marks over the portraits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Grade {
    Excellent,
    Good,
    Average,
    Poor,
}

impl Grade {
    pub fn tone(self) -> Tone {
        match self {
            Grade::Excellent => Tone::Excellent,
            Grade::Good => Tone::Good,
            Grade::Average => Tone::Even,
            Grade::Poor => Tone::Bad,
        }
    }
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
            Tone::Excellent => [10, 98, 58],
            Tone::Good => [46, 150, 90],
            Tone::Even => [226, 231, 241],
            Tone::Poor => [214, 110, 44],
            Tone::Bad => [196, 58, 58],
        }
    }

    /// Text colour on a badge of this tone.
    pub(crate) fn ink(self) -> [u8; 3] {
        if self == Tone::Even {
            [64, 76, 100]
        } else {
            WHITE
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chip {
    pub text: String,
    pub tone: Tone,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatrixRow {
    pub label: Chip,
    /// One per column of the matrix.
    pub cells: Vec<Chip>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FitEntry {
    /// None for a student who is not rated (a special judged by its skill).
    pub grade: Option<Grade>,
    /// The badge text: the grade's name, or "SP".
    pub badge: String,
    pub name: String,
    pub tags: Vec<Chip>,
    /// Muted text at the right instead of tags.
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row {
    /// "지형   야외"
    Info { label: String, value: String },
    /// "적 방어   [중장갑 6] [특수장갑 3]"
    Chips { label: String, chips: Vec<Chip> },
    /// A plain line (wrapped), e.g. a hint or a warning.
    Note(String),
    /// A small table of outcomes: `cols` across, one `MatrixRow` per line, each cell a badge
    /// whose tone says whether the outcome is good for the player.
    Matrix { heading: String, cols: Vec<Chip>, rows: Vec<MatrixRow> },
    /// A scale of badges with a line of explanation under it.
    Legend { chips: Vec<Chip>, text: String },
    /// One line per student: a grade badge, the name, the type tags.
    Fits(Vec<FitEntry>),
    /// The verdict as a wide badge.
    Verdict { label: String, text: String, tone: Tone },
    /// A clickable row; clicking it reports `id` through the overlay's events.
    Button { id: String, label: String, value: String, on: bool },
    /// Like `Info`, and a tooltip shows `tip` while the pointer is over the row.
    Hint { label: String, value: String, tip: String },
    /// A small section title.
    Heading(String),
    /// A section title that folds: pressing it reports `id`; `summary` is shown at the right while folded.
    Section { id: String, title: String, open: bool, summary: String },
    /// A bar from empty to `done` of `total` (just the text when the total is not known yet).
    Progress { label: String, done: usize, total: Option<usize>, text: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PanelState {
    /// Where the player is, e.g. "편성".
    pub scene: String,
    /// Main line, e.g. "19-3 설원 지구 제2 지역". Empty on screens without one.
    pub title: String,
    pub rows: Vec<Row>,
    /// A button in the header's right corner (id, tooltip), drawn as a power icon.
    pub close: Option<(String, String)>,
}

/// The id of the header's hit area: pressing it starts a drag instead of reporting a click.
pub const DRAG_HANDLE: &str = "drag";
/// The id of a tooltip row's hit area (it reports no click; the tooltip is its purpose).
pub const HINT_AREA: &str = "hint";

/// A clickable area of a rendered panel, in the panel image's pixels.
#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    pub id: String,
    /// Shown as a tooltip while the pointer is over the area.
    pub tip: Option<String>,
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

/// `s` cut to `max_w` px at text size `px`, with an ellipsis when it had to be cut.
fn clip(font: &ab_glyph::FontVec, px: f32, max_w: f32, s: &str) -> String {
    if text_width(font, px, s) <= max_w {
        return s.to_string();
    }
    let mut out: String = s.to_string();
    while out.pop().is_some() {
        let cut = format!("{out}…");
        if text_width(font, px, &cut) <= max_w {
            return cut;
        }
    }
    String::new()
}

const BADGE_H: f32 = 19.0;
const MATRIX_STEP: f32 = 23.0;
const FIT_STEP: f32 = 26.0;
const LEGEND_TEXT_PX: f32 = 10.5;

/// A flat badge `w` wide with `text` centred in it, shrunk to fit when it is too long.
#[allow(clippy::too_many_arguments)]
fn badge(img: &mut RgbaImage, fonts: &Fonts, x: f32, y: f32, w: f32, h: f32, px: f32, text: &str, tone: Tone, alpha: f32) {
    let (rgb, ink) = (tone.rgb(), tone.ink());
    fill_rrect(img, x, y, w, h, 3.0 * h / BADGE_H, |_, _| (rgb, alpha));
    let px = px.min(px * (w - 4.0) / text_width(&fonts.bold, px, text).max(1.0));
    let tw = text_width(&fonts.bold, px, text);
    draw_text(img, &fonts.bold, px, x + (w - tw) / 2.0, y + (h - px * 1.2) / 2.0, ink, alpha, text);
}

/// Matrix layout: the width of the label column and of each value column.
fn matrix_columns(fonts: &Fonts, s: f32, inner_w: f32, cols: usize, rows: &[MatrixRow]) -> (f32, f32) {
    let px = 11.0 * s;
    let label_w = rows.iter().map(|r| text_width(&fonts.bold, px, &r.label.text) + 12.0 * s).fold(40.0 * s, f32::max).min(inner_w * 0.34);
    let col_w = ((inner_w - label_w) / cols.max(1) as f32).min(72.0 * s);
    (label_w, col_w)
}

/// One student's line: the better the grade, the stronger the line (tinted band and bar for
/// the best, plain for average, faded for poor, a neutral badge for the unrated).
fn draw_fit_entry(img: &mut RgbaImage, fonts: &Fonts, s: f32, pad: f32, wf: f32, y: f32, e: &FitEntry) {
    let alpha = if e.grade == Some(Grade::Poor) { 0.55 } else { 1.0 };
    let (band, bar) = match e.grade {
        Some(Grade::Excellent) => (Some([212, 238, 221]), Tone::Excellent),
        Some(Grade::Good) => (Some([234, 246, 238]), Tone::Good),
        _ => (None, Tone::Even),
    };
    let h = 23.0 * s;
    if let Some(rgb) = band {
        fill_rrect(img, pad - 6.0 * s, y, wf - pad * 2.0 + 12.0 * s, h, 3.0 * s, |_, _| (rgb, 1.0));
        fill_rrect(img, pad - 6.0 * s, y, 3.5 * s, h, 1.5 * s, |_, _| (bar.rgb(), 1.0));
    }
    let tone = e.grade.map_or(Tone::Even, Grade::tone);
    let badge_w = 44.0 * s;
    badge(img, fonts, pad, y + 2.0 * s, badge_w, 19.0 * s, 11.5 * s, &e.badge, tone, alpha);
    let right = wf - pad;
    let mut x = right;
    for t in e.tags.iter().rev() {
        let w = text_width(&fonts.bold, 10.5 * s, &t.text) + 10.0 * s;
        x -= w;
        badge(img, fonts, x, y + 3.5 * s, w, 16.0 * s, 10.5 * s, &t.text, t.tone, alpha);
        x -= 3.0 * s;
    }
    if !e.note.is_empty() {
        let nw = text_width(&fonts.regular, 10.5 * s, &e.note);
        draw_text(img, &fonts.regular, 10.5 * s, right - nw, y + (h - 10.5 * s * 1.2) / 2.0, MUTED, 1.0, &e.note);
        x = right - nw - 4.0 * s;
    }
    let name_x = pad + badge_w + 8.0 * s;
    let name = clip(&fonts.semibold, 13.0 * s, (x - name_x - 4.0 * s).max(10.0 * s), &e.name);
    let ink = if e.grade == Some(Grade::Poor) { MUTED } else { INK };
    let font = if matches!(e.grade, Some(Grade::Excellent)) { &fonts.bold } else { &fonts.semibold };
    draw_text(img, font, 13.0 * s, name_x, y + (h - 13.0 * s * 1.2) / 2.0, ink, 1.0, &name);
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
            Row::Matrix { rows, .. } => 24.0 * s + (rows.len() + 1) as f32 * MATRIX_STEP * s + 5.0 * s,
            Row::Legend { text, .. } => 22.0 * s + wrap(&fonts.regular, LEGEND_TEXT_PX * s, wf - pad * 2.0, text).len() as f32 * 15.0 * s + 4.0 * s,
            Row::Fits(entries) => entries.len() as f32 * FIT_STEP * s + 5.0 * s,
            Row::Verdict { .. } => 28.0 * s + 7.0 * s,
            Row::Button { .. } => 24.0 * s + 5.0 * s,
            Row::Hint { value, .. } => wrap(&fonts.regular, 14.0 * s, value_w, value).len() as f32 * 21.0 * s + 5.0 * s,
            Row::Heading(_) => 21.0 * s + 4.0 * s,
            Row::Section { .. } => 24.0 * s + 4.0 * s,
            Row::Progress { .. } => 22.0 * s + 5.0 * s,
        };
    }
    h += 8.0 * s;
    let hi = h.ceil() as u32;
    let hf = hi as f32;

    let mut img = RgbaImage::from_pixel(width, hi, Rgba([0, 0, 0, 0]));
    let head = HEAD_H * s;
    card(&mut img, wf, hf, s, head, 12.0 * s, BODY);
    // the close button first: hits are searched in order, and the header below it is a drag handle
    let icon_cx = wf - 36.0 * s;
    if let Some((id, tip)) = &state.close {
        let box_w = 26.0 * s;
        hits.push(Hit { id: id.clone(), tip: Some(tip.clone()), x: icon_cx - box_w / 2.0, y: 0.0, w: box_w, h: head });
    }
    // the header bar is where the card is picked up and moved
    hits.push(Hit { id: DRAG_HANDLE.into(), tip: None, x: 0.0, y: 0.0, w: wf, h: head });

    // header bar: screen name left, tool name right (before the cut corner)
    let scene = if state.scene.is_empty() { "ba-scanner" } else { state.scene.as_str() };
    draw_text(&mut img, &fonts.bold, 12.5 * s, pad, (head - 12.5 * s * 1.2) / 2.0, WHITE, 1.0, scene);
    if !state.scene.is_empty() {
        let name = "ba-scanner";
        let nw = text_width(&fonts.regular, 10.5 * s, name);
        let right = if state.close.is_some() { icon_cx - 16.0 * s } else { wf - 20.0 * s };
        draw_text(&mut img, &fonts.regular, 10.5 * s, right - nw, (head - 10.5 * s * 1.2) / 2.0, NAVY_SOFT, 1.0, name);
    }
    if state.close.is_some() {
        power_icon(&mut img, icon_cx, head / 2.0, 6.5 * s, s);
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
            Row::Hint { label, value, tip } => {
                draw_text(&mut img, &fonts.regular, 12.5 * s, pad, y + 2.0 * s, MUTED, 1.0, label);
                let top = y;
                for line in wrap(&fonts.regular, 14.0 * s, value_w, value) {
                    draw_text(&mut img, &fonts.semibold, 14.0 * s, pad + label_w, y, INK, 1.0, &line);
                    y += 21.0 * s;
                }
                hits.push(Hit { id: HINT_AREA.into(), tip: Some(tip.clone()), x: pad, y: top, w: wf - pad * 2.0, h: y - top });
                y += 5.0 * s;
                if more {
                    rule(&mut img, y - 3.0 * s);
                }
            }
            Row::Heading(text) => {
                y += 4.0 * s;
                draw_text(&mut img, &fonts.bold, 11.5 * s, pad, y, NAVY, 1.0, text);
                y += 17.0 * s;
                fill_rrect(&mut img, pad, y, wf - pad * 2.0, (1.0 * s).max(1.0), 0.0, |_, _| (BORDER, 1.0));
                y += 4.0 * s;
            }
            Row::Section { id, title, open, summary } => {
                let top = y;
                triangle(&mut img, pad + 4.0 * s, y + 12.0 * s, 4.5 * s, *open, NAVY);
                draw_text(&mut img, &fonts.bold, 12.5 * s, pad + 16.0 * s, y + 4.0 * s, NAVY, 1.0, title);
                if !summary.is_empty() {
                    let sw = text_width(&fonts.regular, 12.0 * s, summary);
                    draw_text(&mut img, &fonts.regular, 12.0 * s, wf - pad - sw, y + 5.0 * s, MUTED, 1.0, summary);
                }
                y += 24.0 * s;
                fill_rrect(&mut img, pad, y, wf - pad * 2.0, (1.0 * s).max(1.0), 0.0, |_, _| (BORDER, 1.0));
                hits.push(Hit { id: id.clone(), tip: None, x: pad, y: top, w: wf - pad * 2.0, h: y - top });
                y += 4.0 * s;
            }
            Row::Progress { label, done, total, text } => {
                draw_text(&mut img, &fonts.regular, 12.5 * s, pad, y + 2.0 * s, MUTED, 1.0, label);
                let tw = text_width(&fonts.semibold, 12.5 * s, text);
                let (bx, bw, bh) = (pad + label_w, value_w - tw - 8.0 * s, 8.0 * s);
                let by = y + 6.0 * s;
                fill_rrect(&mut img, bx, by, bw, bh, 1.0 * s, |_, _| (TRACK, 1.0));
                if let Some(t) = total.filter(|t| *t > 0) {
                    let fw = (bw * (*done as f32 / t as f32).clamp(0.0, 1.0)).max(if *done > 0 { 2.0 * s } else { 0.0 });
                    if fw > 0.0 {
                        fill_rrect(&mut img, bx, by, fw, bh, 1.0 * s, |_, _| (BLUE, 1.0));
                    }
                }
                draw_text(&mut img, &fonts.semibold, 12.5 * s, wf - pad - tw, y + 2.0 * s, INK, 1.0, text);
                y += 22.0 * s + 5.0 * s;
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
                    draw_text(&mut img, &fonts.bold, 12.0 * s, cx + 8.0 * s, cy + (21.0 - 12.0 * 1.2) * 0.5 * s, chip.tone.ink(), 1.0, &chip.text);
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
            Row::Matrix { heading, cols, rows } => {
                draw_text(&mut img, &fonts.bold, 11.5 * s, pad, y, NAVY, 1.0, heading);
                fill_rrect(&mut img, pad, y + 17.0 * s, wf - pad * 2.0, (1.0 * s).max(1.0), 0.0, |_, _| (BORDER, 1.0));
                y += 24.0 * s;
                let (label_w, col_w) = matrix_columns(fonts, s, wf - pad * 2.0, cols.len(), rows);
                let (bh, gap) = (BADGE_H * s, 4.0 * s);
                for (i, c) in cols.iter().enumerate() {
                    badge(&mut img, fonts, pad + label_w + i as f32 * col_w, y, col_w - gap, bh, 11.0 * s, &c.text, c.tone, 1.0);
                }
                y += MATRIX_STEP * s;
                for r in rows {
                    badge(&mut img, fonts, pad, y, label_w - 6.0 * s, bh, 11.0 * s, &r.label.text, r.label.tone, 1.0);
                    for (i, c) in r.cells.iter().enumerate() {
                        badge(&mut img, fonts, pad + label_w + i as f32 * col_w, y, col_w - gap, bh, 11.5 * s, &c.text, c.tone, 1.0);
                    }
                    y += MATRIX_STEP * s;
                }
                y += 5.0 * s;
            }
            Row::Legend { chips, text } => {
                let (mut x, bh) = (pad, 17.0 * s);
                for c in chips {
                    let w = text_width(&fonts.bold, 10.5 * s, &c.text) + 12.0 * s;
                    badge(&mut img, fonts, x, y, w, bh, 10.5 * s, &c.text, c.tone, 1.0);
                    x += w + 4.0 * s;
                }
                y += 22.0 * s;
                for line in wrap(&fonts.regular, LEGEND_TEXT_PX * s, wf - pad * 2.0, text) {
                    draw_text(&mut img, &fonts.regular, LEGEND_TEXT_PX * s, pad, y, MUTED, 1.0, &line);
                    y += 15.0 * s;
                }
                y += 4.0 * s;
            }
            Row::Fits(entries) => {
                for e in entries {
                    draw_fit_entry(&mut img, fonts, s, pad, wf, y, e);
                    y += FIT_STEP * s;
                }
                y += 5.0 * s;
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
                hits.push(Hit { id: id.clone(), tip: None, x: bx, y, w: bw, h: bh });
                y += 24.0 * s + 5.0 * s;
            }
            Row::Verdict { label, text, tone } => {
                let (bx, bw, bh) = (pad, wf - pad * 2.0, 28.0 * s);
                let rgb = tone.rgb();
                fill_rrect(&mut img, bx, y, bw, bh, 3.0 * s, |_, _| (rgb, 1.0));
                draw_text(&mut img, &fonts.regular, 12.5 * s, bx + 10.0 * s, y + (28.0 - 12.5 * 1.2) * 0.5 * s, tone.ink(), 0.9, label);
                let tw = text_width(&fonts.bold, 15.0 * s, text);
                draw_text(&mut img, &fonts.bold, 15.0 * s, bx + bw - 10.0 * s - tw, y + (28.0 - 15.0 * 1.2) * 0.5 * s, tone.ink(), 1.0, text);
                y += 28.0 * s + 7.0 * s;
            }
        }
    }
    (img, hits)
}

/// A power symbol (ring with a gap on top and a bar through it) in white, centred at (cx, cy).
fn power_icon(img: &mut RgbaImage, cx: f32, cy: f32, r: f32, s: f32) {
    let t = (1.7 * s).max(1.2);
    fill_rrect(img, cx - r, cy - r, 2.0 * r, 2.0 * r, r, |_, _| (WHITE, 1.0));
    fill_rrect(img, cx - r + t, cy - r + t, 2.0 * (r - t), 2.0 * (r - t), r - t, |_, _| (NAVY, 1.0));
    let gap = 2.4 * s;
    fill_rrect(img, cx - gap, cy - r - 1.0, 2.0 * gap, r - 0.2 * s, 0.0, |_, _| (NAVY, 1.0));
    fill_rrect(img, cx - t / 2.0, cy - r - 0.6 * s, t, r + 0.4 * s, t / 2.0, |_, _| (WHITE, 1.0));
}

/// A small solid triangle centred at (cx, cy): pointing down when `open`, right when folded.
fn triangle(img: &mut RgbaImage, cx: f32, cy: f32, half: f32, open: bool, color: [u8; 3]) {
    let n = (half * 2.0).round().max(2.0) as i32;
    for i in 0..n {
        let f = i as f32 / n as f32;
        if open {
            let span = 2.0 * half * (1.0 - f);
            fill_rrect(img, cx - span / 2.0, cy - half + i as f32, span.max(1.0), 1.0, 0.0, |_, _| (color, 1.0));
        } else {
            let span = 2.0 * half * (1.0 - (2.0 * f - 1.0).abs());
            fill_rrect(img, cx - half * 0.5, cy - half + i as f32, span.max(1.0), 1.0, 0.0, |_, _| (color, 1.0));
        }
    }
}

/// A small flat tooltip: `text` (lines split at '\n') in white on navy. `scale` is the card's
/// scale (panel width over its design width), so the tooltip grows with the window.
pub fn render_tooltip(scale: f32, text: &str, fonts: &Fonts) -> RgbaImage {
    let s = scale;
    let (px, pad) = (12.5 * s, 8.0 * s);
    let lines: Vec<&str> = text.lines().collect();
    let w = lines.iter().map(|l| text_width(&fonts.semibold, px, l)).fold(0.0f32, f32::max) + pad * 2.0;
    let line_h = px * 1.45;
    let h = lines.len() as f32 * line_h + pad * 1.4;
    let mut img = RgbaImage::from_pixel(w.ceil() as u32, h.ceil() as u32, Rgba([0, 0, 0, 0]));
    let (wf, hf) = (w.ceil(), h.ceil());
    fill_rrect(&mut img, 0.0, 0.0, wf, hf, 3.0 * s, |x, y| {
        let edge = x < 1.0 || y < 1.0 || x > wf - 1.0 || y > hf - 1.0;
        (if edge { NAVY_SOFT } else { NAVY }, 1.0)
    });
    for (i, l) in lines.iter().enumerate() {
        draw_text(&mut img, &fonts.semibold, px, pad, pad * 0.7 + i as f32 * line_h, WHITE, 1.0, l);
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
                Row::Verdict { label: "방어 판정".into(), text: "불리".into(), tone: Tone::Bad },
                Row::Verdict { label: "지형 판정".into(), text: "평균".into(), tone: Tone::Even },
                Row::Button { id: "ingame".into(), label: "인게임 표시".into(), value: "켜짐".into(), on: true },
                Row::Button { id: "place".into(), label: "임무 카드".into(), value: "게임 밖 · 100%".into(), on: false },
            ],
            ..Default::default()
        }
    }

    #[test]
    fn the_close_icon_comes_before_the_drag_handle_and_a_section_row_reports_its_id() {
        let Some(f) = load_fonts() else { return };
        let state = PanelState {
            scene: "상태".into(),
            rows: vec![Row::Section { id: "section:x".into(), title: "학생 정보".into(), open: false, summary: "3일 경과".into() }, Row::Note("본문".into())],
            close: Some(("quit".into(), "애드온 종료".into())),
            ..Default::default()
        };
        let (img, hits) = render_panel_hits(300, &state, &f);
        let at = |id: &str| hits.iter().position(|h| h.id == id).unwrap_or_else(|| panic!("no hit {id}"));
        assert!(at("quit") < at(DRAG_HANDLE), "the icon wins over the header it sits on");
        let quit = &hits[at("quit")];
        assert_eq!(quit.tip.as_deref(), Some("애드온 종료"));
        assert!(quit.x > 300.0 * 0.7 && quit.x + quit.w < 300.0 - 8.0, "inside the header, left of its cut corner: {} {}", quit.x, quit.w);
        let section = &hits[at("section:x")];
        assert!(section.w > 200.0 && section.h > 15.0);
        // the icon is white on the navy header
        let (cx, cy) = ((quit.x + quit.w / 2.0) as u32, (quit.h / 2.0) as u32);
        let white = (cx - 7..cx + 7).any(|x| (cy - 7..cy + 7).any(|y| img.get_pixel(x, y).0[..3] == [255, 255, 255]));
        assert!(white, "a white icon in the header");
        if let Some(dir) = std::env::var_os("BANNER_PREVIEW_DIR") {
            img.save(std::path::Path::new(&dir).join("close-section.png")).unwrap();
            let open = PanelState { rows: vec![Row::Section { id: "section:x".into(), title: "학생 정보".into(), open: true, summary: String::new() }, Row::Note("본문".into())], ..state };
            render_panel(300, &open, &f).save(std::path::Path::new(&dir).join("close-section-open.png")).unwrap();
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
    fn hint_rows_carry_their_tooltip_and_progress_rows_draw_a_bar() {
        let Some(f) = load_fonts() else { return };
        let state = PanelState {
            scene: "상태".into(),
            title: String::new(),
            rows: vec![
                Row::Heading("학생 정보".into()),
                Row::Hint { label: "갱신".into(), value: "3일 2시간 경과".into(), tip: "2026-11-20 17:49".into() },
                Row::Progress { label: "진행".into(), done: 50, total: Some(100), text: "50/100".into() },
            ],
            ..Default::default()
        };
        let (img, hits) = render_panel_hits(300, &state, &f);
        let hint = hits.iter().find(|h| h.id == HINT_AREA).expect("a hit area for the hint row");
        assert_eq!(hint.tip.as_deref(), Some("2026-11-20 17:49"));
        assert!(hits.iter().filter(|h| h.id != DRAG_HANDLE).count() == 1, "only the hint row has an area");
        // the bar is half full: blue at its first quarter, track colour at its last
        let y = img.height() - 22;
        let at = |x: u32| *img.get_pixel(x, y);
        let blue_found = (60..200).any(|x| { let p = at(x); p[2] > 170 && p[0] < 90 });
        assert!(blue_found, "a blue fill on the progress row");
        let tip = render_tooltip(1.0, "마지막 스캔\n2026-11-20 17:49", &f);
        assert!(tip.width() > 80 && tip.height() > 30, "{}x{}", tip.width(), tip.height());
        assert_eq!(tip.get_pixel(0, 0)[3], 0, "rounded corner");
        if let Some(dir) = std::env::var_os("BANNER_PREVIEW_DIR") {
            img.save(std::path::Path::new(&dir).join("rows.png")).unwrap();
            tip.save(std::path::Path::new(&dir).join("tip.png")).unwrap();
        }
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
            ..Default::default()
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
