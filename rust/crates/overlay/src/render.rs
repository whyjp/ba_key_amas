//! Banner rendering: pure pixels in, pixels out, so the design can be checked
//! without a game window. Flat light card with a navy header bar, like the game's own panels.
use ab_glyph::{point, Font, FontVec, PxScale, ScaleFont};
use image::{Rgba, RgbaImage};

/// Design size; the banner is scaled to the game window.
pub const BASE_W: f32 = 520.0;
pub const BASE_H: f32 = 108.0;

/// Share of the game window's width the banner takes.
const WIDTH_SHARE: f32 = 0.54;
/// Distance of the banner's top edge from the window's top, as a share of its height:
/// it sits between the game header and the info panel (which starts at 25.5%).
const TOP_SHARE: f32 = 0.07;

/// Banner rectangle (x, y, w, h) inside a client area of `client_w` x `client_h`
/// pixels, relative to the client's top-left. Everything is proportional to the
/// window, so any window size (and DPI) gets the same look and the same placement.
pub fn banner_layout(client_w: i32, client_h: i32) -> (i32, i32, u32, u32) {
    let w = ((client_w as f32 * WIDTH_SHARE).round() as i32).max(120);
    let h = (w as f32 * BASE_H / BASE_W).round() as i32;
    ((client_w - w) / 2, (client_h as f32 * TOP_SHARE).round() as i32, w as u32, h as u32)
}

/// Rectangle as (left, top, right, bottom) in screen pixels.
pub type Ltrb = (i32, i32, i32, i32);

/// Top-left of the banner in screen coordinates, chosen so it does not sit on the
/// game: directly above the game window, else directly below it, and only when
/// the monitor has no room on either side, inside the window in the band between
/// the game header and its info panel.
///
/// `client` and `outer` are the game's client area and whole window (with title
/// bar), `work` the monitor's work area (taskbar excluded), `size` the banner.
pub fn banner_origin(client: Ltrb, outer: Ltrb, work: Ltrb, size: (i32, i32)) -> (i32, i32) {
    let (cw, ch) = (client.2 - client.0, client.3 - client.1);
    let (bw, bh) = size;
    let gap = ((ch as f32 * 0.01).round() as i32).max(4);
    // centred on the window, but never pushed off the monitor
    let x = (client.0 + (cw - bw) / 2).clamp(work.0, (work.2 - bw).max(work.0));
    let above = outer.1 - bh - gap;
    if above >= work.1 {
        return (x, above);
    }
    let below = outer.3 + gap;
    if below + bh <= work.3 {
        return (x, below);
    }
    (x, client.1 + (ch as f32 * TOP_SHARE).round() as i32)
}

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
pub(crate) const RED: [u8; 3] = [196, 62, 60];
pub(crate) const TEXT: [u8; 3] = [255, 255, 255];

/// A flat card: opaque `body` with a 1px `border`, rounded corners, and a navy header of
/// height `head` whose top-right corner is cut at 45 degrees (the one game-like detail).
pub(crate) fn card(img: &mut RgbaImage, w: f32, h: f32, s: f32, head: f32, cut: f32, body: [u8; 3]) {
    let b = (1.0 * s).max(1.0);
    fill_rrect(img, 0.0, 0.0, w, h, 5.0 * s, |x, y| {
        // inside the cut triangle: transparent, anti-aliased along the diagonal
        let d = (w - cut + y) - x;
        let a = (0.5 + d * 0.7071).clamp(0.0, 1.0);
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

/// Coverage (0..1) of pixel centre (px,py) by a rounded rect, anti-aliased by a 1px ramp.
fn rrect_cov(px: f32, py: f32, x: f32, y: f32, w: f32, h: f32, r: f32) -> f32 {
    let (cx, cy) = (x + w / 2.0, y + h / 2.0);
    let (qx, qy) = ((px - cx).abs() - (w / 2.0 - r), (py - cy).abs() - (h / 2.0 - r));
    let d = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0) - r;
    (0.5 - d).clamp(0.0, 1.0)
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

fn percent(state: &BannerState) -> Option<f32> {
    state.total.filter(|t| *t > 0).map(|t| (state.done as f32 / t as f32).clamp(0.0, 1.0))
}

/// Render the banner at `width` px (height follows the design ratio).
/// `tick` drives the dots and the indeterminate bar (about 10 per second).
pub fn render(width: u32, state: &BannerState, tick: u32, fonts: &Fonts) -> RgbaImage {
    let s = width as f32 / BASE_W;
    let (w, h) = (width, (BASE_H * s).round() as u32);
    let mut img = RgbaImage::from_pixel(w, h, Rgba([0, 0, 0, 0]));
    let (wf, hf) = (w as f32, h as f32);

    let head = 30.0 * s;
    card(&mut img, wf, hf, s, head, 12.0 * s, BODY);
    let pad = 18.0 * s;

    // header bar: name left, count right
    draw_text(&mut img, &fonts.bold, 14.0 * s, pad, 6.5 * s, WHITE, 1.0, "ba-scanner");
    let count = match (&state.phase, state.total) {
        (Phase::Counting, _) if state.done > 0 => format!("{}명 확인", state.done),
        (Phase::Counting, _) => "집계 중".to_string(),
        (_, Some(t)) => format!("{}/{}", state.done, t),
        (_, None) => format!("{}", state.done),
    };
    let cw = text_width(&fonts.semibold, 14.0 * s, &count);
    draw_text(&mut img, &fonts.semibold, 14.0 * s, wf - 22.0 * s - cw, 6.5 * s, NAVY_SOFT.map(|c| c.saturating_add(70)), 1.0, &count);

    // what is happening, with animated dots
    let dots = ".".repeat(1 + ((tick / 6) % 3) as usize);
    let (line, dotted) = match state.phase {
        Phase::Counting => ("학생 수를 확인하고 있습니다", true),
        Phase::Scanning => ("학생데이터를 스캔하고있습니다", true),
        Phase::Done => ("스캔이 완료되었습니다", false),
        Phase::Failed => ("스캔이 중단되었습니다", false),
    };
    draw_text(&mut img, &fonts.bold, 16.0 * s, pad, 38.0 * s, INK, 1.0, line);
    if dotted {
        let lw = text_width(&fonts.bold, 16.0 * s, line);
        draw_text(&mut img, &fonts.bold, 16.0 * s, pad + lw, 38.0 * s, BLUE, 1.0, &dots);
    }

    // progress bar: thin, flat, square-ish
    let fill = if state.phase == Phase::Failed { RED } else { BLUE };
    let (bx, by, bw, bh) = (pad, 67.0 * s, wf - pad * 2.0, 6.0 * s);
    fill_rrect(&mut img, bx, by, bw, bh, 1.5 * s, |_, _| (TRACK, 1.0));
    match (percent(state), &state.phase) {
        (Some(p), _) if p > 0.0 => {
            let fw = (bw * p).max(bh);
            fill_rrect(&mut img, bx, by, fw, bh, 1.5 * s, |_, _| (fill, 1.0));
        }
        (None, Phase::Counting | Phase::Scanning) => {
            // indeterminate: a segment sweeps back and forth so the wait is visibly alive
            let seg = bw * 0.28;
            let t = (tick % 60) as f32 / 60.0;
            let pos = (1.0 - (2.0 * t - 1.0).abs()) * (bw - seg);
            fill_rrect(&mut img, bx, by, bw, bh, 1.5 * s, |x, _| (fill, if x >= pos && x <= pos + seg { 1.0 } else { 0.0 }));
        }
        _ => {}
    }

    // hint (+ percent)
    let hint = match state.phase {
        Phase::Failed if !state.note.is_empty() => state.note.as_str(),
        Phase::Done => "잠시 후 이 안내는 사라집니다",
        _ => "작업 중입니다 · 완료될 때까지 게임을 조작하지 마세요",
    };
    let hint_color = if state.phase == Phase::Failed { RED } else { MUTED };
    draw_text(&mut img, &fonts.regular, 12.0 * s, pad, 84.0 * s, hint_color, 1.0, hint);
    if let Some(p) = percent(state) {
        let t = format!("{:.0}%", p * 100.0);
        let tw = text_width(&fonts.semibold, 12.0 * s, &t);
        draw_text(&mut img, &fonts.semibold, 12.0 * s, wf - pad - tw, 84.0 * s, INK, 1.0, &t);
    }
    img
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fonts() -> Option<Fonts> {
        load_fonts()
    }

    #[test]
    fn renders_every_phase_and_scales() {
        let Some(f) = fonts() else { return };
        let states = [
            BannerState { phase: Phase::Counting, ..Default::default() },
            BannerState { phase: Phase::Scanning, done: 12, total: Some(106), note: String::new() },
            BannerState { phase: Phase::Scanning, done: 3, total: None, note: String::new() },
            BannerState { phase: Phase::Done, done: 106, total: Some(106), note: String::new() },
            BannerState { phase: Phase::Failed, done: 40, total: Some(106), note: "포커스를 잃어 중단했습니다".into() },
        ];
        for (i, st) in states.iter().enumerate() {
            for w in [520u32, 780] {
                let img = render(w, st, 7, &f);
                assert_eq!(img.width(), w);
                assert_eq!(img.height(), (BASE_H * w as f32 / BASE_W).round() as u32);
                // corners are transparent (rounded), the middle is opaque card
                assert_eq!(img.get_pixel(0, 0)[3], 0);
                assert!(img.get_pixel(w / 2, img.height() / 2)[3] > 200);
            }
            if let Some(dir) = std::env::var_os("BANNER_PREVIEW_DIR") {
                let path = std::path::Path::new(&dir).join(format!("banner-{i}.png"));
                render(780, st, 7, &f).save(path).unwrap();
            }
        }
    }

    /// The banner is a fixed fraction of the window at every window size, and never
    /// reaches the game's info panel (which starts at 25.5% of the height).
    #[test]
    fn layout_is_proportional_to_the_window() {
        let base = banner_layout(1280, 960);
        for (cw, ch) in [(455, 341), (640, 480), (910, 682), (1280, 960), (1920, 1440), (3840, 2880)] {
            let (x, y, w, h) = banner_layout(cw, ch);
            let k = cw as f32 / 1280.0;
            assert!((w as f32 - base.2 as f32 * k).abs() <= 1.5, "{cw}: width {w}");
            assert!((h as f32 - base.3 as f32 * k).abs() <= 1.5, "{cw}: height {h}");
            assert!((y as f32 - base.1 as f32 * k).abs() <= 1.5, "{cw}: top {y}");
            assert_eq!(x, (cw - w as i32) / 2, "{cw}: centred");
            assert!((y + h as i32) as f32 / (ch as f32) < 0.245, "{cw}: stays above the info panel");
        }
    }

    #[test]
    fn banner_stays_off_the_game_when_it_can() {
        let work = (0, 0, 1920, 1040);
        let size = (490, 102);
        // a window in the middle of the screen: the banner sits above it
        let client = (400, 300, 1310, 982);
        let outer = (392, 270, 1318, 990);
        let (x, y) = banner_origin(client, outer, work, size);
        assert!(y + size.1 <= outer.1, "above the window, not on it: y={y}");
        assert_eq!(x, 400 + (910 - 490) / 2);
        // a window at the very top: below it
        let outer_top = (392, 0, 1318, 720);
        let (_, y) = banner_origin((400, 30, 1310, 712), outer_top, work, size);
        assert!(y >= outer_top.3, "below the window: y={y}");
        // a window filling the work area: only then inside, in the header/panel band
        let full = (0, 0, 1920, 1040);
        let (_, y) = banner_origin((0, 31, 1920, 1040), full, work, size);
        assert!(y > 31 && y < 31 + (1009.0 * 0.245) as i32, "inside band: y={y}");
        // never off the monitor horizontally
        let (x, _) = banner_origin((0, 300, 300, 600), (0, 270, 300, 610), work, size);
        assert!(x >= 0);
    }

    #[test]
    fn progress_fill_tracks_the_count() {
        let Some(f) = fonts() else { return };
        let mut half = BannerState { phase: Phase::Scanning, done: 53, total: Some(106), note: String::new() };
        let w = 520u32;
        let img = render(w, &half, 0, &f);
        let y = (70.0 * w as f32 / BASE_W) as u32;
        let filled = |x: u32| img.get_pixel(x, y)[0] < 120;
        let mid = (18.0 + (w as f32 - 36.0) * 0.5) as u32;
        assert!(filled(mid - 20) && !filled(mid + 40), "fill should end near the middle");
        half.done = 0;
        assert!(percent(&half) == Some(0.0));
    }
}
