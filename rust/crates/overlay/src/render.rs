//! Banner rendering: pure pixels in, pixels out, so the design can be checked
//! without a game window. Colours follow the game UI: navy header band, light
//! cyan accent, white text.
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

// Palette (Blue Archive UI): header navy, panel navy, cyan accent, light text.
const NAVY_TOP: [u8; 3] = [44, 74, 116];
const NAVY_BOTTOM: [u8; 3] = [27, 47, 80];
const ACCENT: [u8; 3] = [76, 195, 255];
const ACCENT_LIGHT: [u8; 3] = [150, 224, 255];
const TRACK: [u8; 3] = [14, 28, 52];
const TEXT: [u8; 3] = [255, 255, 255];
const TEXT_SOFT: [u8; 3] = [196, 214, 236];
const WARN: [u8; 3] = [255, 196, 92];

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

impl Default for BannerState {
    fn default() -> Self {
        Self { phase: Phase::Counting, done: 0, total: None, note: String::new() }
    }
}

pub struct Fonts {
    pub regular: FontVec,
    pub bold: FontVec,
}

/// System Korean fonts (Malgun Gothic ships with Windows).
pub fn load_fonts() -> Option<Fonts> {
    let dir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into());
    let read = |names: &[&str]| -> Option<FontVec> {
        names.iter().find_map(|n| {
            let bytes = std::fs::read(format!("{dir}\\Fonts\\{n}")).ok()?;
            FontVec::try_from_vec(bytes).ok()
        })
    };
    let regular = read(&["malgun.ttf", "NanumGothic.ttf", "segoeui.ttf"])?;
    let bold = read(&["malgunbd.ttf", "NanumGothicBold.ttf", "segoeuib.ttf"]).unwrap_or_else(|| read(&["malgun.ttf"]).unwrap());
    Some(Fonts { regular, bold })
}

fn over(dst: &mut Rgba<u8>, rgb: [u8; 3], alpha: f32) {
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

fn fill_rrect(img: &mut RgbaImage, x: f32, y: f32, w: f32, h: f32, r: f32, color: impl Fn(f32, f32) -> ([u8; 3], f32)) {
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

fn lerp(a: [u8; 3], b: [u8; 3], t: f32) -> [u8; 3] {
    let t = t.clamp(0.0, 1.0);
    [0, 1, 2].map(|i| (a[i] as f32 + (b[i] as f32 - a[i] as f32) * t).round() as u8)
}

pub fn text_width(font: &FontVec, px: f32, s: &str) -> f32 {
    let f = font.as_scaled(PxScale::from(px));
    s.chars().map(|c| f.h_advance(f.glyph_id(c))).sum()
}

/// Draw `s` with its top at `y` (so rows line up regardless of the glyphs).
fn draw_text(img: &mut RgbaImage, font: &FontVec, px: f32, x: f32, y: f32, rgb: [u8; 3], alpha: f32, s: &str) {
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

    // card: navy gradient, soft border, cyan accent strip on the left
    fill_rrect(&mut img, 0.0, 0.0, wf, hf, 16.0 * s, |_, y| (lerp(NAVY_TOP, NAVY_BOTTOM, y / hf), 0.94));
    fill_rrect(&mut img, 0.0, 0.0, wf, hf, 16.0 * s, |x, y| {
        let edge = x < 1.2 * s || y < 1.2 * s || x > wf - 1.2 * s || y > hf - 1.2 * s;
        (TEXT, if edge { 0.16 } else { 0.0 })
    });
    let accent = match state.phase {
        Phase::Failed => WARN,
        _ => ACCENT,
    };
    fill_rrect(&mut img, 12.0 * s, 14.0 * s, 5.0 * s, hf - 28.0 * s, 2.5 * s, |_, _| (accent, 1.0));

    let pad = 30.0 * s;
    // row 1: title + count
    draw_text(&mut img, &fonts.bold, 21.0 * s, pad, 10.0 * s, ACCENT_LIGHT, 1.0, "ba-scanner");
    let count = match (&state.phase, state.total) {
        (Phase::Counting, _) if state.done > 0 => format!("{}명 확인", state.done),
        (Phase::Counting, _) => "집계 중".to_string(),
        (_, Some(t)) => format!("{}/{}", state.done, t),
        (_, None) => format!("{}", state.done),
    };
    let cw = text_width(&fonts.bold, 22.0 * s, &count);
    draw_text(&mut img, &fonts.bold, 22.0 * s, wf - 22.0 * s - cw, 9.0 * s, TEXT, 1.0, &count);

    // row 2: what is happening, with animated dots
    let dots = ".".repeat(1 + ((tick / 6) % 3) as usize);
    let (line, dotted) = match state.phase {
        Phase::Counting => ("학생 수를 확인하고 있습니다", true),
        Phase::Scanning => ("학생데이터를 스캔하고있습니다", true),
        Phase::Done => ("스캔이 완료되었습니다", false),
        Phase::Failed => ("스캔이 중단되었습니다", false),
    };
    draw_text(&mut img, &fonts.regular, 15.0 * s, pad, 40.0 * s, TEXT, 0.96, line);
    if dotted {
        let lw = text_width(&fonts.regular, 15.0 * s, line);
        draw_text(&mut img, &fonts.regular, 15.0 * s, pad + lw, 40.0 * s, ACCENT_LIGHT, 1.0, &dots);
    }

    // progress bar
    let (bx, by, bw, bh) = (pad, 68.0 * s, wf - pad - 22.0 * s, 8.0 * s);
    fill_rrect(&mut img, bx, by, bw, bh, bh / 2.0, |_, _| (TRACK, 0.85));
    match (percent(state), &state.phase) {
        (Some(p), _) if p > 0.0 => {
            let fw = (bw * p).max(bh);
            fill_rrect(&mut img, bx, by, fw, bh, bh / 2.0, |x, _| (lerp(ACCENT, ACCENT_LIGHT, x / fw.max(1.0)), 1.0));
        }
        (None, Phase::Counting | Phase::Scanning) => {
            // indeterminate: a segment sweeps back and forth so the wait is visibly alive
            let seg = bw * 0.28;
            let t = (tick % 60) as f32 / 60.0;
            let pos = (1.0 - (2.0 * t - 1.0).abs()) * (bw - seg);
            fill_rrect(&mut img, bx, by, bw, bh, bh / 2.0, |x, _| {
                let inside = x >= pos && x <= pos + seg;
                (lerp(ACCENT, ACCENT_LIGHT, (x - pos) / seg), if inside { 1.0 } else { 0.0 })
            });
        }
        _ => {}
    }

    // row 4: hint (+ percent)
    let hint = match state.phase {
        Phase::Failed if !state.note.is_empty() => state.note.as_str(),
        Phase::Done => "잠시 후 이 안내는 사라집니다",
        _ => "작업 중입니다 · 완료될 때까지 게임을 조작하지 마세요",
    };
    let hint_color = if state.phase == Phase::Failed { WARN } else { TEXT_SOFT };
    draw_text(&mut img, &fonts.regular, 12.0 * s, pad, 84.0 * s, hint_color, 0.9, hint);
    if let Some(p) = percent(state) {
        let t = format!("{:.0}%", p * 100.0);
        let tw = text_width(&fonts.regular, 12.0 * s, &t);
        draw_text(&mut img, &fonts.regular, 12.0 * s, wf - 22.0 * s - tw, 84.0 * s, TEXT_SOFT, 0.9, &t);
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
    fn progress_fill_tracks_the_count() {
        let Some(f) = fonts() else { return };
        let mut half = BannerState { phase: Phase::Scanning, done: 53, total: Some(106), note: String::new() };
        let w = 520u32;
        let img = render(w, &half, 0, &f);
        let y = (72.0 * w as f32 / BASE_W) as u32;
        let cyan = |x: u32| {
            let p = img.get_pixel(x, y);
            p[2] > 200 && p[0] < 200
        };
        let mid = (30.0 + (w as f32 - 52.0) * 0.5) as u32;
        assert!(cyan(mid - 20) && !cyan(mid + 40), "fill should end near the middle");
        half.done = 0;
        assert!(percent(&half) == Some(0.0));
    }
}
