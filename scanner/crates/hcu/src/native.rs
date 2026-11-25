//! In-process desktop control: the part of Microsoft's Hybrid Computer Use (MIT, `computer-use`
//! with its Windows provider) that this project uses, written against Win32 directly, so no
//! helper program is started.
//!
//! It answers the same requests the `computer-use` CLI did (`gui screenshot`, `gui zoom`, `gui click`,
//! `gui --json` with scroll/click actions, `apps`, `activate`) with the same JSON records, so
//! everything above [`Runner`] is unchanged. The old helper-process backend is gone.
//!
//! Behaviour follows the reference implementation (`NativeDesktop*.cs`, `WindowsDesktopImageGeometry.cs`):
//! the screenshot is the whole virtual desktop delivered at no more than 1366x768; zoom regions and
//! click points are in that delivered frame; a click point maps to the native pixel
//! `(u + 0.5) * scale - 0.5` and is sent as a `SendInput` absolute coordinate on the virtual desktop;
//! a wheel notch is 120 units.
//!
//! Input is guarded here, at the boundary, so no caller has to remember to (see `docs/hcu-native.md`):
//! nothing is sent when the frame is stale (desktop size or the windows over the point changed since
//! the screenshot), when no monitor shows the point, or when the window under it is not the intended
//! one. Input that stopped half way is answered `effect_unknown` and is never repeated.
use crate::audit::{iso_utc, AuditSink};
use crate::guard::{failure_record, same_over, Refusal, SendError, WinInfo};
use crate::hcu::Runner;
use anyhow::{anyhow, bail, Result};
use image::{imageops, RgbImage};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

const MAX_DELIVERED: (u32, u32) = (1366, 768);
const MIN_ZOOM_SIDE: f64 = 8.0;
const MAX_ZOOM_SCALE: f64 = 4.0;

/// The size an image of `native` pixels is delivered at: shrunk to fit 1366x768, never enlarged.
pub fn delivered_size(native: (u32, u32)) -> (u32, u32) {
    let scale = 1f64.min(MAX_DELIVERED.0 as f64 / native.0 as f64).min(MAX_DELIVERED.1 as f64 / native.1 as f64);
    (((native.0 as f64 * scale).round() as u32).max(1), ((native.1 as f64 * scale).round() as u32).max(1))
}

/// A delivered-frame coordinate as a native desktop pixel offset.
pub fn to_native(delivered: f64, delivered_extent: u32, native_extent: u32) -> f64 {
    let scale = native_extent as f64 / delivered_extent as f64;
    ((delivered + 0.5) * scale - 0.5).clamp(0.0, native_extent as f64 - 1.0)
}

/// `SendInput` absolute coordinate (0..65535 over the virtual desktop) that lands on `pixel`.
pub fn to_absolute(pixel: i64, length: i64) -> i32 {
    ((pixel * 65536 + length - 1) / length) as i32
}

/// What a zoom looks at: the native crop, the region it covers in frame pixels, and the image size.
#[derive(Debug, Clone, PartialEq)]
pub struct ZoomGeometry {
    pub native: (i32, i32, i32, i32), // x, y, w, h
    pub region: [f64; 4],
    pub size: (u32, u32),
}

pub fn zoom_geometry(region: [f64; 4], native: (u32, u32)) -> Result<ZoomGeometry> {
    let frame = delivered_size(native);
    let [x0, y0, x1, y1] = region;
    if !(0.0 <= x0 && x0 < x1 && x1 <= frame.0 as f64 && 0.0 <= y0 && y0 < y1 && y1 <= frame.1 as f64) {
        bail!("zoom region must lie inside the {}x{} frame with x0 < x1 and y0 < y1", frame.0, frame.1);
    }
    if x1 - x0 < MIN_ZOOM_SIDE || y1 - y0 < MIN_ZOOM_SIDE {
        bail!("zoom region sides must be at least {MIN_ZOOM_SIDE} frame pixels");
    }
    let (per_x, per_y) = (native.0 as f64 / frame.0 as f64, native.1 as f64 / frame.1 as f64);
    let left = ((x0 * per_x + 1e-9).floor() as i32).max(0);
    let top = ((y0 * per_y + 1e-9).floor() as i32).max(0);
    let right = ((x1 * per_x - 1e-9).ceil() as i32).min(native.0 as i32);
    let bottom = ((y1 * per_y - 1e-9).ceil() as i32).min(native.1 as i32);
    let (cw, ch) = ((right - left) as f64, (bottom - top) as f64);
    let enlarge = MAX_ZOOM_SCALE.min(MAX_DELIVERED.0 as f64 / cw).min(MAX_DELIVERED.1 as f64 / ch);
    Ok(ZoomGeometry {
        native: (left, top, right - left, bottom - top),
        region: [left as f64 / per_x, top as f64 / per_y, right as f64 / per_x, bottom as f64 / per_y],
        size: (((cw * enlarge).round() as u32).max(1), ((ch * enlarge).round() as u32).max(1)),
    })
}

/// A request whose answer the desktop cannot give right now, reported like the CLI's failure record.
fn failure(code: &str, hint: &str) -> String {
    json!({"ok": false, "code": code, "hint": hint, "input_emitted": false}).to_string()
}

/// One action of a `gui --json` request.
#[derive(Debug, Clone, PartialEq)]
enum Action {
    Click { x: f64, y: f64 },
    Scroll { x: f64, y: f64, delta: i32 },
}

fn parse_actions(payload: &str) -> Result<Vec<Action>> {
    let v: Value = serde_json::from_str(payload).map_err(|e| anyhow!("gui --json: {e}"))?;
    let list = v.get("actions").and_then(Value::as_array).ok_or_else(|| anyhow!("gui --json: no actions"))?;
    list.iter()
        .map(|a| {
            let num = |k: &str| a.get(k).and_then(Value::as_f64).ok_or_else(|| anyhow!("action without {k}"));
            match a.get("type").and_then(Value::as_str) {
                Some("click") => Ok(Action::Click { x: num("x")?, y: num("y")? }),
                Some("scroll") => Ok(Action::Scroll { x: num("x")?, y: num("y")?, delta: num("scroll_y")? as i32 }),
                other => bail!("unsupported action {other:?}"),
            }
        })
        .collect()
}

/// The desktop as the runner needs it. Tests supply a fake; Windows supplies the real one.
pub trait Desktop: Send + Sync {
    /// (x, y, width, height) of the virtual desktop in physical pixels.
    fn bounds(&self) -> (i32, i32, u32, u32);
    /// The pixels of a rectangle of the virtual desktop (absolute coordinates).
    fn grab(&self, x: i32, y: i32, w: u32, h: u32) -> Result<RgbImage>;
    /// Move the pointer to a desktop pixel and press/release the left button. An `Err` never
    /// leaves the button down (best effort) and says whether anything was already sent.
    fn click(&self, x: i32, y: i32) -> Result<(), SendError>;
    /// Move the pointer to a desktop pixel and turn the wheel by `notches` (positive = up).
    fn wheel(&self, x: i32, y: i32, notches: i32) -> Result<(), SendError>;
    /// `(ref, title, active)` of the visible top-level windows with a title.
    fn windows(&self) -> Vec<(String, String, bool)>;
    fn activate(&self, window_ref: &str) -> Result<()>;
    /// The visible top-level windows of other programs, front to back.
    fn layout(&self) -> Vec<WinInfo>;
    /// The top-level window that receives a click at a desktop pixel (own windows included, flagged).
    fn window_at(&self, x: i32, y: i32) -> Option<WinInfo>;
    /// Whether a monitor shows this desktop pixel (the virtual desktop can have dead space).
    fn monitor_contains(&self, x: i32, y: i32) -> bool;
}

/// After input: how long to wait for the screen to be still again.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Settle {
    /// Longest wait; zero turns waiting off.
    pub cap: Duration,
    /// Time between the captures that are compared.
    pub step: Duration,
}

impl Settle {
    pub const OFF: Settle = Settle { cap: Duration::ZERO, step: Duration::from_millis(60) };

    /// After a wheel movement: 600 ms unless `BAOCR_HCU_SETTLE_MS` says otherwise (0 = do not wait).
    pub fn from_env() -> Self {
        let ms = std::env::var("BAOCR_HCU_SETTLE_MS").ok().and_then(|v| v.trim().parse::<u64>().ok()).unwrap_or(600);
        Settle { cap: Duration::from_millis(ms), ..Settle::OFF }
    }

    /// After a click: none unless `BAOCR_HCU_CLICK_SETTLE_MS` says otherwise. Every caller that clicks waits for the
    /// screen to change and hold still itself (`settle`, `wait_stable_next`), and the game never holds still to the
    /// pixel (an animated background), so this wait always ran to its cap: 600 ms of the 730 ms a click took.
    pub fn click_from_env() -> Self {
        let ms = std::env::var("BAOCR_HCU_CLICK_SETTLE_MS").ok().and_then(|v| v.trim().parse::<u64>().ok()).unwrap_or(0);
        Settle { cap: Duration::from_millis(ms), ..Settle::OFF }
    }
}

/// The frame the last screenshot showed, which coordinates are relative to.
#[derive(Clone)]
struct Frame {
    native: (u32, u32),
    layout: Vec<WinInfo>,
}

/// Where an input point landed once it passed the guards.
struct Located {
    native: (i32, i32),
    window: WinInfo,
}

/// Answers the CLI's requests from a [`Desktop`], refusing input that no longer matches what was seen.
pub struct NativeRunner<D: Desktop> {
    desktop: D,
    state: Mutex<State>,
    /// Exact title of the only window input may reach; None = whichever window the frame showed there.
    target: Option<String>,
    settle: Settle,
    /// The wait after input that is only clicks (`settle` is for the rest).
    click_settle: Settle,
    audit: Option<Box<dyn AuditSink>>,
}

#[derive(Default)]
struct State {
    frame: Option<Frame>,
    images: HashMap<String, RgbImage>,
    next: u64,
}

/// Attempts at a screenshot while windows move under it.
const CAPTURE_ATTEMPTS: u32 = 3;

/// How an input request ended.
struct Outcome<'a> {
    code: &'a str,
    hint: &'a str,
    emitted: bool,
}

impl<D: Desktop> NativeRunner<D> {
    /// No target window, no settling, no audit log: the bare protocol with the frame guards.
    pub fn new(desktop: D) -> Self {
        Self { desktop, state: Mutex::new(State::default()), target: None, settle: Settle::OFF, click_settle: Settle::OFF, audit: None }
    }

    /// Input is only sent when the window under the point has exactly this title.
    pub fn with_target(mut self, title: &str) -> Self {
        self.target = Some(title.to_string());
        self
    }

    pub fn with_settle(mut self, settle: Settle) -> Self {
        self.settle = settle;
        self
    }

    pub fn with_click_settle(mut self, settle: Settle) -> Self {
        self.click_settle = settle;
        self
    }

    pub fn with_audit(mut self, sink: Box<dyn AuditSink>) -> Self {
        self.audit = Some(sink);
        self
    }

    fn store(&self, img: RgbImage) -> String {
        let mut s = self.state.lock().unwrap();
        s.next += 1;
        let id = format!("mem:{}", s.next);
        // old frames are never read again; keep the table small
        if s.images.len() > 8 {
            s.images.clear();
        }
        s.images.insert(id.clone(), img);
        id
    }

    fn native(&self) -> (u32, u32) {
        let (_, _, w, h) = self.desktop.bounds();
        (w, h)
    }

    fn frame(&self) -> Option<Frame> {
        self.state.lock().unwrap().frame.clone()
    }

    fn screenshot(&self) -> Result<String> {
        // a window that moves while the pixels are read would pair them with the wrong layout
        for attempt in 1..=CAPTURE_ATTEMPTS {
            let (before, bounds) = (self.desktop.layout(), self.desktop.bounds());
            let (x, y, w, h) = bounds;
            let img = self.desktop.grab(x, y, w, h)?;
            if self.desktop.bounds() != bounds || !same_over(&before, &self.desktop.layout(), [x, y, x + w as i32, y + h as i32]) {
                if attempt < CAPTURE_ATTEMPTS {
                    std::thread::sleep(Duration::from_millis(25));
                }
                continue;
            }
            let (dw, dh) = delivered_size((w, h));
            let delivered = if (dw, dh) == (w, h) { img } else { imageops::resize(&img, dw, dh, imageops::FilterType::CatmullRom) };
            self.state.lock().unwrap().frame = Some(Frame { native: (w, h), layout: before });
            let id = self.store(delivered);
            return Ok(json!({"ok": true, "images": [id], "screen": {"width": dw, "height": dh}}).to_string());
        }
        bail!("desktop_layout_unstable: windows kept moving during every screenshot attempt")
    }

    fn zoom(&self, region: [f64; 4]) -> Result<String> {
        let native = self.native();
        if self.frame().filter(|f| f.native == native).is_none() {
            return Ok(failure("gui_frame_stale", "the desktop's size changed since the last screenshot; take a new one"));
        }
        let g = match zoom_geometry(region, native) {
            Ok(g) => g,
            Err(e) => return Ok(failure("invalid_zoom_region", &e.to_string())),
        };
        let (ox, oy, _, _) = self.desktop.bounds();
        let (left, top) = (ox + g.native.0, oy + g.native.1);
        // A zoom only looks: nothing is sent, so it does not compare window layouts (listing every window
        // four times per capture cost about 250 ms of a 370 ms capture on a busy desktop). Input checks the
        // layout over its own point against the frame, which is where staleness can do harm.
        let crop = self.desktop.grab(left, top, g.native.2 as u32, g.native.3 as u32)?;
        let img = if g.size == crop.dimensions() { crop } else { imageops::resize(&crop, g.size.0, g.size.1, imageops::FilterType::CatmullRom) };
        let id = self.store(img);
        let (sx, sy) = (g.size.0 as f64 / (g.region[2] - g.region[0]), g.size.1 as f64 / (g.region[3] - g.region[1]));
        Ok(json!({"ok": true, "images": [id], "evidence": {"zoom": {"region": g.region, "scale_x": sx, "scale_y": sy}}}).to_string())
    }

    /// Everything that must hold right before input at a frame point: the frame still describes the
    /// desktop, a monitor shows the pixel, nothing was stacked over it since, and the window that
    /// receives the click is the intended one. The add-on's own windows never qualify: they are
    /// click-through, so a click that would land on one is not aimed at the game.
    fn guard(&self, frame: &Frame, x: f64, y: f64) -> Result<Located, Refusal> {
        let (ox, oy, w, h) = self.desktop.bounds();
        if frame.native != (w, h) {
            return Err(Refusal::Stale("the desktop's size changed since the last screenshot".into()));
        }
        let (dw, dh) = delivered_size((w, h));
        if !(x.is_finite() && y.is_finite() && x >= 0.0 && y >= 0.0 && x <= dw as f64 - 1.0 && y <= dh as f64 - 1.0) {
            return Err(Refusal::Stale("the point is outside the last frame".into()));
        }
        let (px, py) = (ox + to_native(x, dw, w).round() as i32, oy + to_native(y, dh, h).round() as i32);
        if !self.desktop.monitor_contains(px, py) {
            return Err(Refusal::OffMonitor);
        }
        if !same_over(&frame.layout, &self.desktop.layout(), [px, py, px + 1, py + 1]) {
            return Err(Refusal::Stale("the windows over the point changed since the frame its coordinates came from".into()));
        }
        let Some(window) = self.desktop.window_at(px, py) else {
            return Err(Refusal::WrongWindow("no window receives a click at that point".into()));
        };
        if window.own {
            return Err(Refusal::WrongWindow("the add-on's own window is under the point".into()));
        }
        if let Some(title) = &self.target {
            if &window.title != title {
                return Err(Refusal::WrongWindow(format!("the window under the point is \"{}\", not \"{title}\"", window.title)));
            }
        }
        Ok(Located { native: (px, py), window })
    }

    fn input(&self, actions: &[Action]) -> Result<String> {
        let at = |a: &Action| match a {
            Action::Click { x, y } | Action::Scroll { x, y, .. } => (*x, *y),
        };
        let started = Instant::now();
        let Some(frame) = self.frame() else {
            let hint = Refusal::Stale("no screenshot has been taken yet".into()).hint();
            return Ok(self.finish(actions, &[], 0, Some(Outcome { code: "gui_frame_stale", hint: &hint, emitted: false }), started));
        };
        // every point is checked before anything is sent
        let mut located: Vec<Located> = Vec::new();
        for a in actions {
            match self.guard(&frame, at(a).0, at(a).1) {
                Ok(l) => located.push(l),
                Err(r) => return Ok(self.finish(actions, &located, 0, Some(Outcome { code: r.code(), hint: &r.hint(), emitted: false }), started)),
            }
        }
        let mut done = 0;
        let mut stop: Option<(&str, String, bool)> = None;
        for (i, a) in actions.iter().enumerate() {
            // the first point was checked a moment ago; later ones may follow a click that changed the screen
            if i > 0 {
                if let Err(r) = self.guard(&frame, at(a).0, at(a).1) {
                    stop = Some((r.code(), r.hint(), false));
                    break;
                }
            }
            let (px, py) = located[i].native;
            let sent = match a {
                Action::Click { .. } => self.desktop.click(px, py),
                Action::Scroll { delta, .. } => self.desktop.wheel(px, py, -*delta),
            };
            if let Err(e) = sent {
                stop = Some(("input_failed", format!("{:#}", e.error), e.emitted));
                break;
            }
            done += 1;
        }
        if let Some((code, hint, emitted)) = stop {
            // Some events were sent and the rest were not (or one failed half way): the effect on the
            // game is unknown, so the caller must look at the screen, not repeat the input.
            if done > 0 || emitted {
                let hint = format!("{code}: {hint}; {done} of {} actions completed", actions.len());
                return Ok(self.finish(actions, &located, done, Some(Outcome { code: "effect_unknown", hint: &hint, emitted: true }), started));
            }
            return Ok(self.finish(actions, &located, done, Some(Outcome { code, hint: &hint, emitted: false }), started));
        }
        let only_clicks = actions.iter().all(|a| matches!(a, Action::Click { .. }));
        self.wait_still(located.first().map(|l| l.window.rect), if only_clicks { self.click_settle } else { self.settle });
        Ok(self.finish(actions, &located, done, None, started))
    }

    /// The answer record, and the audit line for it.
    fn finish(&self, actions: &[Action], located: &[Located], done: usize, failed: Option<Outcome>, started: Instant) -> String {
        let kind = match actions {
            [Action::Click { .. }] => "click",
            a if a.iter().all(|x| matches!(x, Action::Scroll { .. })) => "scroll",
            _ => "batch",
        };
        if let Some(sink) = &self.audit {
            let first = located.first();
            let requested = actions.first().map(|a| match a {
                Action::Click { x, y } | Action::Scroll { x, y, .. } => json!([x, y]),
            });
            sink.record(&json!({
                "t": iso_utc(SystemTime::now()),
                "kind": kind,
                "actions": actions.len(),
                "frame": requested,
                "native": first.map(|l| json!([l.native.0, l.native.1])),
                "window": first.map(|l| json!({"title": l.window.title.chars().take(80).collect::<String>(), "class": l.window.class})),
                "code": failed.as_ref().map_or("ok", |o| o.code),
                "input_emitted": failed.as_ref().map_or(done > 0, |o| o.emitted),
                "completed": done,
                "ms": started.elapsed().as_millis() as u64,
            }));
        }
        match failed {
            Some(o) => failure_record(o.code, o.hint, o.emitted, done),
            None => {
                let (dw, dh) = delivered_size(self.native());
                json!({"ok": true, "input_emitted": done > 0, "completed": done, "screen": {"width": dw, "height": dh}}).to_string()
            }
        }
    }

    /// Returns once the window the input went to has stopped changing, or after the cap: what
    /// the game shows right after a click is often still moving, and the caller reads it next.
    fn wait_still(&self, area: Option<[i32; 4]>, settle: Settle) {
        let (Some(area), true) = (area, settle.cap > Duration::ZERO) else { return };
        let (ox, oy, w, h) = self.desktop.bounds();
        let (x0, y0) = (area[0].max(ox), area[1].max(oy));
        let (x1, y1) = (area[2].min(ox + w as i32), area[3].min(oy + h as i32));
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        let started = Instant::now();
        let mut prev: Option<RgbImage> = None;
        loop {
            std::thread::sleep(settle.step);
            if started.elapsed() >= settle.cap {
                return;
            }
            let Ok(cur) = self.desktop.grab(x0, y0, (x1 - x0) as u32, (y1 - y0) as u32) else { return };
            if prev.as_ref().is_some_and(|p| p.dimensions() == cur.dimensions() && p.as_raw() == cur.as_raw()) {
                return;
            }
            prev = Some(cur);
        }
    }

    fn apps(&self) -> String {
        self.desktop
            .windows()
            .into_iter()
            .map(|(r, title, active)| format!("{r} \"{title}\"  app=window{}\n", if active { "  active" } else { "" }))
            .collect()
    }
}

impl<D: Desktop> Runner for NativeRunner<D> {
    fn run(&self, args: &[String]) -> Result<String> {
        let a: Vec<&str> = args.iter().map(String::as_str).collect();
        match a.as_slice() {
            ["gui", "screenshot", ..] => self.screenshot(),
            ["gui", "zoom", x0, y0, x1, y1, ..] => {
                let n = [x0, y0, x1, y1].map(|s| s.parse::<f64>());
                match n {
                    [Ok(a), Ok(b), Ok(c), Ok(d)] => self.zoom([a, b, c, d]),
                    _ => bail!("gui zoom: four numbers expected"),
                }
            }
            ["gui", "click", x, y, ..] => {
                let (x, y) = (x.parse::<f64>()?, y.parse::<f64>()?);
                self.input(&[Action::Click { x, y }])
            }
            ["gui", "--json", payload, ..] => self.input(&parse_actions(payload)?),
            ["apps", ..] => Ok(self.apps()),
            ["activate", r, ..] => {
                self.desktop.activate(r)?;
                Ok(json!({"ok": true}).to_string())
            }
            other => bail!("native desktop: unsupported request {other:?}"),
        }
    }

    fn load_image(&self, path: &str) -> Result<RgbImage> {
        self.state.lock().unwrap().images.remove(path).ok_or_else(|| anyhow!("no such frame {path}"))
    }
}

/// Pause between wheel notches of one request, so the game's list keeps up.
pub const WHEEL_GAP: Duration = Duration::from_millis(12);

#[cfg(windows)]
pub use win::WinDesktop;

#[cfg(windows)]
mod win {
    use super::*;
    use std::ffi::c_void;

    #[repr(C)]
    struct Rect {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Point {
        x: i32,
        y: i32,
    }
    #[repr(C)]
    struct MouseInput {
        dx: i32,
        dy: i32,
        data: u32,
        flags: u32,
        time: u32,
        extra: usize,
    }
    #[repr(C)]
    struct Input {
        kind: u32,
        mouse: MouseInput,
    }
    #[repr(C)]
    struct BitmapInfoHeader {
        size: u32,
        width: i32,
        height: i32,
        planes: u16,
        bit_count: u16,
        compression: u32,
        size_image: u32,
        x_ppm: i32,
        y_ppm: i32,
        clr_used: u32,
        clr_important: u32,
    }

    #[link(name = "user32")]
    extern "system" {
        fn SetProcessDPIAware() -> i32;
        fn GetSystemMetrics(index: i32) -> i32;
        fn GetDC(hwnd: isize) -> isize;
        fn ReleaseDC(hwnd: isize, dc: isize) -> i32;
        fn SendInput(n: u32, inputs: *const Input, size: i32) -> u32;
        fn EnumWindows(cb: unsafe extern "system" fn(isize, isize) -> i32, l: isize) -> i32;
        fn IsWindowVisible(hwnd: isize) -> i32;
        fn GetWindowTextW(hwnd: isize, buf: *mut u16, max: i32) -> i32;
        fn GetForegroundWindow() -> isize;
        fn SetForegroundWindow(hwnd: isize) -> i32;
        fn ShowWindow(hwnd: isize, cmd: i32) -> i32;
        fn IsIconic(hwnd: isize) -> i32;
        fn IsWindow(hwnd: isize) -> i32;
        fn GetWindowRect(hwnd: isize, r: *mut Rect) -> i32;
        fn GetWindowThreadProcessId(hwnd: isize, pid: *mut u32) -> u32;
        fn GetClassNameW(hwnd: isize, buf: *mut u16, max: i32) -> i32;
        fn GetWindowLongW(hwnd: isize, index: i32) -> i32;
        fn WindowFromPoint(p: Point) -> isize;
        fn GetAncestor(hwnd: isize, flags: u32) -> isize;
        fn MonitorFromPoint(p: Point, flags: u32) -> isize;
    }
    #[link(name = "gdi32")]
    extern "system" {
        fn CreateCompatibleDC(dc: isize) -> isize;
        fn CreateCompatibleBitmap(dc: isize, w: i32, h: i32) -> isize;
        fn SelectObject(dc: isize, obj: isize) -> isize;
        fn BitBlt(dst: isize, x: i32, y: i32, w: i32, h: i32, src: isize, sx: i32, sy: i32, rop: u32) -> i32;
        fn GetDIBits(dc: isize, bmp: isize, start: u32, lines: u32, bits: *mut c_void, info: *mut BitmapInfoHeader, usage: u32) -> i32;
        fn DeleteObject(obj: isize) -> i32;
        fn DeleteDC(dc: isize) -> i32;
    }
    #[link(name = "dwmapi")]
    extern "system" {
        fn DwmGetWindowAttribute(hwnd: isize, attribute: u32, value: *mut c_void, size: u32) -> i32;
    }

    const SRCCOPY_CAPTUREBLT: u32 = 0x00CC_0020 | 0x4000_0000;
    const MOVE: u32 = 0x0001;
    const LEFT_DOWN: u32 = 0x0002;
    const LEFT_UP: u32 = 0x0004;
    const WHEEL: u32 = 0x0800;
    const ABSOLUTE: u32 = 0x8000;
    const VIRTUAL_DESK: u32 = 0x4000;
    const SW_RESTORE: i32 = 9;
    const GWL_EXSTYLE: i32 = -20;
    const WS_EX_TRANSPARENT: u32 = 0x20;
    const WS_EX_LAYERED: u32 = 0x0008_0000;
    const GA_ROOT: u32 = 2;
    const DWMWA_CLOAKED: u32 = 14;
    const RELEASE_TRIES: u32 = 3;

    /// The real desktop.
    pub struct WinDesktop;

    impl WinDesktop {
        pub fn new() -> Self {
            unsafe {
                SetProcessDPIAware(); // physical pixels, like the reference implementation
            }
            WinDesktop
        }

        fn mouse(&self, flags: u32, dx: i32, dy: i32, data: u32) -> Result<()> {
            let input = Input { kind: 0, mouse: MouseInput { dx, dy, data, flags, time: 0, extra: 0 } };
            let sent = unsafe { SendInput(1, &input, std::mem::size_of::<Input>() as i32) };
            if sent != 1 {
                bail!("SendInput sent {sent} of 1 events (the desktop may be locked or the game runs elevated)");
            }
            Ok(())
        }

        /// Release the left button, trying again: a button left down would keep dragging in the game.
        fn release_left(&self) -> Result<()> {
            let mut last = Ok(());
            for _ in 0..RELEASE_TRIES {
                last = self.mouse(LEFT_UP, 0, 0, 0);
                if last.is_ok() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            last
        }

        fn info(&self, hwnd: isize) -> Option<WinInfo> {
            unsafe {
                let mut r = Rect { left: 0, top: 0, right: 0, bottom: 0 };
                if GetWindowRect(hwnd, &mut r) == 0 {
                    return None;
                }
                let (mut title, mut class) = ([0u16; 256], [0u16; 256]);
                let tn = GetWindowTextW(hwnd, title.as_mut_ptr(), 256).max(0) as usize;
                let cn = GetClassNameW(hwnd, class.as_mut_ptr(), 256).max(0) as usize;
                let mut pid = 0u32;
                GetWindowThreadProcessId(hwnd, &mut pid);
                Some(WinInfo {
                    hwnd,
                    rect: [r.left, r.top, r.right, r.bottom],
                    class: String::from_utf16_lossy(&class[..cn]),
                    title: String::from_utf16_lossy(&title[..tn]),
                    own: pid == std::process::id(),
                })
            }
        }

        fn move_to(&self, x: i32, y: i32) -> Result<()> {
            let (ox, oy, w, h) = self.bounds();
            let ax = to_absolute((x - ox) as i64, w as i64);
            let ay = to_absolute((y - oy) as i64, h as i64);
            self.mouse(MOVE | ABSOLUTE | VIRTUAL_DESK, ax, ay, 0)
        }
    }

    impl Default for WinDesktop {
        fn default() -> Self {
            Self::new()
        }
    }

    impl Desktop for WinDesktop {
        fn bounds(&self) -> (i32, i32, u32, u32) {
            unsafe { (GetSystemMetrics(76), GetSystemMetrics(77), GetSystemMetrics(78).max(1) as u32, GetSystemMetrics(79).max(1) as u32) }
        }

        fn grab(&self, x: i32, y: i32, w: u32, h: u32) -> Result<RgbImage> {
            unsafe {
                let screen = GetDC(0);
                let dc = CreateCompatibleDC(screen);
                let bmp = CreateCompatibleBitmap(screen, w as i32, h as i32);
                let old = SelectObject(dc, bmp);
                let ok = BitBlt(dc, 0, 0, w as i32, h as i32, screen, x, y, SRCCOPY_CAPTUREBLT) != 0;
                let mut bgra = vec![0u8; w as usize * h as usize * 4];
                let mut info = BitmapInfoHeader { size: std::mem::size_of::<BitmapInfoHeader>() as u32, width: w as i32, height: -(h as i32), planes: 1, bit_count: 32, compression: 0, size_image: 0, x_ppm: 0, y_ppm: 0, clr_used: 0, clr_important: 0 };
                let lines = if ok { GetDIBits(dc, bmp, 0, h, bgra.as_mut_ptr() as *mut c_void, &mut info, 0) } else { 0 };
                SelectObject(dc, old);
                DeleteObject(bmp);
                DeleteDC(dc);
                ReleaseDC(0, screen);
                if !ok || lines as u32 != h {
                    bail!("screen capture failed (BitBlt {ok}, {lines} of {h} lines)");
                }
                let mut rgb = Vec::with_capacity(w as usize * h as usize * 3);
                for p in bgra.chunks_exact(4) {
                    rgb.extend_from_slice(&[p[2], p[1], p[0]]);
                }
                RgbImage::from_raw(w, h, rgb).ok_or_else(|| anyhow!("capture buffer size"))
            }
        }

        fn click(&self, x: i32, y: i32) -> Result<(), SendError> {
            self.move_to(x, y).map_err(SendError::not_sent)?;
            // from here on the pointer has moved: any failure leaves an input the player can see
            self.mouse(LEFT_DOWN, 0, 0, 0).map_err(SendError::sent)?;
            self.release_left().map_err(SendError::sent)
        }

        fn wheel(&self, x: i32, y: i32, notches: i32) -> Result<(), SendError> {
            self.move_to(x, y).map_err(SendError::not_sent)?;
            self.mouse(WHEEL, 0, 0, (notches * 120) as u32).map_err(SendError::sent)?;
            std::thread::sleep(WHEEL_GAP);
            Ok(())
        }

        fn windows(&self) -> Vec<(String, String, bool)> {
            unsafe extern "system" fn each(hwnd: isize, out: isize) -> i32 {
                let list = &mut *(out as *mut Vec<(String, String, bool)>);
                if IsWindowVisible(hwnd) != 0 {
                    let mut buf = [0u16; 256];
                    let n = GetWindowTextW(hwnd, buf.as_mut_ptr(), 256);
                    if n > 0 {
                        let mut r = Rect { left: 0, top: 0, right: 0, bottom: 0 };
                        GetWindowRect(hwnd, &mut r);
                        if r.right > r.left && r.bottom > r.top {
                            list.push((format!("w{hwnd}"), String::from_utf16_lossy(&buf[..n as usize]), hwnd == GetForegroundWindow()));
                        }
                    }
                }
                1
            }
            let mut list: Vec<(String, String, bool)> = Vec::new();
            unsafe {
                EnumWindows(each, &mut list as *mut _ as isize);
            }
            list
        }

        fn layout(&self) -> Vec<WinInfo> {
            unsafe extern "system" fn each(hwnd: isize, out: isize) -> i32 {
                let list = &mut *(out as *mut Vec<isize>);
                if IsWindowVisible(hwnd) != 0 && IsIconic(hwnd) == 0 {
                    list.push(hwnd);
                }
                1
            }
            let mut all: Vec<isize> = Vec::new();
            unsafe {
                EnumWindows(each, &mut all as *mut _ as isize);
            }
            all.into_iter()
                .filter(|&hwnd| unsafe {
                    // hidden by the shell (other virtual desktops, suspended apps)
                    let mut cloaked = 0u32;
                    DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED, &mut cloaked as *mut u32 as *mut c_void, 4);
                    // click-through windows cannot receive a click, so they cannot change what one hits
                    let ex = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
                    cloaked == 0 && ex & (WS_EX_LAYERED | WS_EX_TRANSPARENT) != (WS_EX_LAYERED | WS_EX_TRANSPARENT)
                })
                .filter_map(|hwnd| self.info(hwnd))
                .filter(|w| !w.own && w.rect[2] > w.rect[0] && w.rect[3] > w.rect[1])
                .collect()
        }

        fn window_at(&self, x: i32, y: i32) -> Option<WinInfo> {
            unsafe {
                let hit = WindowFromPoint(Point { x, y });
                if hit == 0 {
                    return None;
                }
                let root = GetAncestor(hit, GA_ROOT);
                self.info(if root == 0 { hit } else { root })
            }
        }

        fn monitor_contains(&self, x: i32, y: i32) -> bool {
            unsafe { MonitorFromPoint(Point { x, y }, 0) != 0 } // MONITOR_DEFAULTTONULL
        }

        fn activate(&self, window_ref: &str) -> Result<()> {
            let hwnd: isize = window_ref.strip_prefix('w').and_then(|n| n.parse().ok()).ok_or_else(|| anyhow!("bad window reference {window_ref}"))?;
            unsafe {
                if IsWindow(hwnd) == 0 {
                    bail!("window {window_ref} is gone");
                }
                if IsIconic(hwnd) != 0 {
                    ShowWindow(hwnd, SW_RESTORE);
                }
                // A plain request, no input tricks (no key taps, no input-queue attachment): Windows
                // may refuse it while the player works in another window. Then the scan does not
                // start, and the player is asked to click the game window.
                if GetForegroundWindow() == hwnd {
                    return Ok(());
                }
                SetForegroundWindow(hwnd);
                for _ in 0..30 {
                    if GetForegroundWindow() == hwnd {
                        return Ok(());
                    }
                    std::thread::sleep(Duration::from_millis(25));
                }
            }
            bail!("window {window_ref} did not become the active window: click the game window once, then start again")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb;
    use std::sync::Arc;

    #[test]
    fn delivery_never_enlarges_and_fits_1366_by_768() {
        assert_eq!(delivered_size((1920, 1080)), (1365, 768));
        assert_eq!(delivered_size((2560, 1440)), (1365, 768));
        assert_eq!(delivered_size((1280, 720)), (1280, 720));
        assert_eq!(delivered_size((3840, 1080)), (1366, 384));
    }

    #[test]
    fn frame_points_map_to_native_pixels_and_absolute_coordinates() {
        // the reference's pinned examples: 2560 native shown as 1365 wide
        assert!((to_native(0.0, 1365, 2560) - 0.438).abs() < 0.01);
        assert_eq!(to_native(1364.0, 1365, 2560).round(), 2559.0, "the last delivered pixel is the last native one");
        assert_eq!(to_native(5000.0, 1365, 2560), 2559.0, "clamped inside the desktop");
        // an absolute coordinate lands on its pixel: floor(n * length / 65536) == pixel
        for (p, len) in [(0i64, 2560i64), (1, 2560), (1279, 2560), (2559, 2560), (777, 1920)] {
            let n = to_absolute(p, len) as i64;
            assert_eq!(n * len / 65536, p, "pixel {p} of {len}");
        }
    }

    #[test]
    fn zoom_snaps_outward_to_whole_native_pixels_and_caps_enlargement() {
        let g = zoom_geometry([100.0, 100.0, 200.0, 150.0], (2560, 1440)).unwrap();
        assert_eq!(g.native, (187, 187, 189, 95)); // floor(100*1.8755) .. ceil(200*1.8755)
        assert_eq!(g.size, (756, 380), "enlarged 4x (the cap), not to the 1366 limit");
        assert!(g.region[0] <= 100.0 && g.region[2] >= 200.0, "the covered region contains the request");
        assert!(zoom_geometry([0.0, 0.0, 5.0, 100.0], (2560, 1440)).is_err(), "sides under 8 frame pixels");
        assert!(zoom_geometry([0.0, 0.0, 1400.0, 100.0], (2560, 1440)).is_err(), "outside the frame");
    }

    #[test]
    fn actions_are_read_from_the_cli_payload() {
        let a = parse_actions(r#"{"actions":[{"type":"scroll","x":10.5,"y":20,"scroll_y":-3},{"type":"click","x":1,"y":2}]}"#).unwrap();
        assert_eq!(a, vec![Action::Scroll { x: 10.5, y: 20.0, delta: -3 }, Action::Click { x: 1.0, y: 2.0 }]);
        assert!(parse_actions(r#"{"actions":[{"type":"key"}]}"#).is_err());
    }

    /// A 2000x1000 desktop whose left half is red and right half green, with the game window over
    /// all of it; records what is sent.
    struct Fake {
        log: Mutex<Vec<String>>,
        size: Mutex<(u32, u32)>,
        /// every window, front to back, the add-on's own included
        wins: Mutex<Vec<WinInfo>>,
        /// the monitors' rectangles; empty means the whole desktop is shown
        monitors: Mutex<Vec<[i32; 4]>>,
        /// the nth click (0 based) fails; whether it had already sent something
        fail_click: Mutex<Option<(usize, bool)>>,
        /// a click replaces the window list (a popup opening)
        on_click: Mutex<Option<Vec<WinInfo>>>,
        /// every grab differs from the one before (the screen never settles)
        animate: Mutex<bool>,
        clicks: Mutex<usize>,
        grabs: Mutex<u8>,
    }
    fn win(hwnd: isize, title: &str, rect: [i32; 4]) -> WinInfo {
        WinInfo { hwnd, rect, class: "UnityWndClass".into(), title: title.into(), own: false }
    }
    impl Fake {
        fn new() -> Self {
            Fake {
                log: Mutex::new(Vec::new()),
                size: Mutex::new((2000, 1000)),
                wins: Mutex::new(vec![win(13, "Blue Archive", [0, 0, 2000, 1000])]),
                monitors: Mutex::new(Vec::new()),
                fail_click: Mutex::new(None),
                on_click: Mutex::new(None),
                animate: Mutex::new(false),
                clicks: Mutex::new(0),
                grabs: Mutex::new(0),
            }
        }
        fn sent(&self) -> Vec<String> {
            self.log.lock().unwrap().iter().filter(|l| l.starts_with("click") || l.starts_with("wheel")).cloned().collect()
        }
    }
    impl Desktop for &Fake {
        fn bounds(&self) -> (i32, i32, u32, u32) {
            let s = *self.size.lock().unwrap();
            (0, 0, s.0, s.1)
        }
        fn grab(&self, x: i32, y: i32, w: u32, h: u32) -> Result<RgbImage> {
            self.log.lock().unwrap().push(format!("grab {x} {y} {w} {h}"));
            let tick = if *self.animate.lock().unwrap() {
                let mut g = self.grabs.lock().unwrap();
                *g = g.wrapping_add(1);
                *g
            } else {
                0
            };
            Ok(RgbImage::from_fn(w, h, |px, _| if (x as u32 + px) < 1000 { Rgb([200, tick, 0]) } else { Rgb([0, 200, tick]) }))
        }
        fn click(&self, x: i32, y: i32) -> Result<(), SendError> {
            let n = {
                let mut c = self.clicks.lock().unwrap();
                *c += 1;
                *c - 1
            };
            if let Some((at, emitted)) = *self.fail_click.lock().unwrap() {
                if n == at {
                    return Err(SendError { emitted, error: anyhow!("SendInput refused") });
                }
            }
            self.log.lock().unwrap().push(format!("click {x} {y}"));
            if let Some(next) = self.on_click.lock().unwrap().take() {
                *self.wins.lock().unwrap() = next;
            }
            Ok(())
        }
        fn wheel(&self, x: i32, y: i32, n: i32) -> Result<(), SendError> {
            self.log.lock().unwrap().push(format!("wheel {x} {y} {n}"));
            Ok(())
        }
        fn windows(&self) -> Vec<(String, String, bool)> {
            vec![("w7".into(), "Chrome".into(), false), ("w13".into(), "Blue Archive".into(), true)]
        }
        fn activate(&self, r: &str) -> Result<()> {
            self.log.lock().unwrap().push(format!("activate {r}"));
            Ok(())
        }
        fn layout(&self) -> Vec<WinInfo> {
            self.wins.lock().unwrap().iter().filter(|w| !w.own).cloned().collect()
        }
        fn window_at(&self, x: i32, y: i32) -> Option<WinInfo> {
            self.wins.lock().unwrap().iter().find(|w| w.contains(x, y)).cloned()
        }
        fn monitor_contains(&self, x: i32, y: i32) -> bool {
            let m = self.monitors.lock().unwrap();
            m.is_empty() || m.iter().any(|r| x >= r[0] && x < r[2] && y >= r[1] && y < r[3])
        }
    }

    /// The runner the app builds: input only for the exact-title game window.
    fn game_runner(fake: &Fake) -> NativeRunner<&Fake> {
        NativeRunner::new(fake).with_target("Blue Archive")
    }

    fn ask(r: &NativeRunner<&Fake>, args: &[&str]) -> Value {
        let owned: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        serde_json::from_str(&r.run(&owned).unwrap()).unwrap()
    }

    #[test]
    fn screenshot_zoom_click_and_scroll_speak_the_cli_protocol() {
        let fake = Fake::new();
        let r = NativeRunner::new(&fake);
        // input before any screenshot is refused with nothing sent
        let v = ask(&r, &["gui", "click", "10", "10", "--records"]);
        assert_eq!((v["ok"].as_bool(), v["code"].as_str(), v["input_emitted"].as_bool()), (Some(false), Some("gui_frame_stale"), Some(false)));
        assert!(fake.log.lock().unwrap().iter().all(|l| !l.starts_with("click")));

        let shot = ask(&r, &["gui", "screenshot", "--records"]);
        assert_eq!((shot["screen"]["width"].as_u64(), shot["screen"]["height"].as_u64()), (Some(1366), Some(683)));
        let img = r.load_image(shot["images"][0].as_str().unwrap()).unwrap();
        assert_eq!(img.dimensions(), (1366, 683));
        assert_eq!(img.get_pixel(100, 300).0, [200, 0, 0]);
        assert_eq!(img.get_pixel(1300, 300).0, [0, 200, 0]);

        // a zoom of the red/green seam reports where its pixels came from
        let z = ask(&r, &["gui", "zoom", "600", "100", "760", "200", "--records"]);
        let region: Vec<f64> = z["evidence"]["zoom"]["region"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
        assert!(region[0] <= 600.0 && region[2] >= 760.0);
        let zi = r.load_image(z["images"][0].as_str().unwrap()).unwrap();
        let sx = z["evidence"]["zoom"]["scale_x"].as_f64().unwrap();
        assert!((zi.width() as f64 - (region[2] - region[0]) * sx).abs() < 1.0);

        // click and scroll go to the native pixel of the frame point
        let c = ask(&r, &["gui", "click", "683", "341.5", "--records"]);
        assert_eq!((c["ok"].as_bool(), c["input_emitted"].as_bool(), c["completed"].as_i64()), (Some(true), Some(true), Some(1)));
        let s = ask(&r, &["gui", "--json", r#"{"actions":[{"type":"scroll","x":100,"y":100,"scroll_y":-2},{"type":"scroll","x":100,"y":100,"scroll_y":-2}]}"#, "--records"]);
        assert_eq!(s["completed"].as_i64(), Some(2));
        let log = fake.log.lock().unwrap().clone();
        assert!(log.contains(&"click 1000 500".to_string()), "{log:?}");
        assert_eq!(log.iter().filter(|l| l.starts_with("wheel 147 147 2")).count(), 2, "{log:?}");

        // a desktop that changed size makes the old frame stale: nothing is sent
        *fake.size.lock().unwrap() = (1000, 800);
        let before = fake.log.lock().unwrap().len();
        let v = ask(&r, &["gui", "click", "10", "10", "--records"]);
        assert_eq!(v["code"].as_str(), Some("gui_frame_stale"));
        assert_eq!(fake.log.lock().unwrap().len(), before);
    }

    #[test]
    fn a_point_outside_the_frame_sends_nothing_not_even_the_valid_ones() {
        let fake = Fake::new();
        let r = NativeRunner::new(&fake);
        ask(&r, &["gui", "screenshot"]);
        let v = ask(&r, &["gui", "--json", r#"{"actions":[{"type":"click","x":5,"y":5},{"type":"click","x":5000,"y":5}]}"#]);
        assert_eq!(v["ok"].as_bool(), Some(false));
        assert!(fake.log.lock().unwrap().iter().all(|l| !l.starts_with("click")));
    }

    #[test]
    fn windows_are_listed_in_the_format_the_title_lookup_reads() {
        let fake = Fake::new();
        let r = NativeRunner::new(&fake);
        let apps = r.run(&["apps".to_string()]).unwrap();
        assert_eq!(crate::hcu::window_ref(&apps, "Blue Archive").as_deref(), Some("w13"));
        assert!(crate::hcu::window_active(&apps, "Blue Archive"));
        assert!(!crate::hcu::window_active(&apps, "Chrome"));
        ask(&r, &["activate", "w13", "--records"]);
        assert!(fake.log.lock().unwrap().contains(&"activate w13".to_string()));
    }

    struct Collect(Arc<Mutex<Vec<Value>>>);
    impl AuditSink for Collect {
        fn record(&self, line: &Value) {
            self.0.lock().unwrap().push(line.clone());
        }
    }

    fn code(v: &Value) -> (Option<bool>, Option<&str>, Option<bool>) {
        (v["ok"].as_bool(), v["code"].as_str(), v["input_emitted"].as_bool())
    }

    const CLICK: &[&str] = &["gui", "click", "683", "341.5", "--records"];
    fn two_clicks(a: u32, b: u32) -> String {
        format!(r#"{{"actions":[{{"type":"click","x":{a},"y":100}},{{"type":"click","x":{b},"y":100}}]}}"#)
    }

    #[test]
    fn a_window_stacked_over_the_point_after_the_frame_refuses_the_click() {
        let fake = Fake::new();
        let r = game_runner(&fake);
        ask(&r, &["gui", "screenshot"]);
        fake.wins.lock().unwrap().insert(0, win(5, "Chrome", [900, 400, 1100, 600]));
        let v = ask(&r, CLICK);
        assert_eq!(code(&v), (Some(false), Some("gui_frame_stale"), Some(false)));
        assert!(fake.sent().is_empty(), "nothing sent");
        // a fresh screenshot sees the new layout, and then the browser is the window under the point
        ask(&r, &["gui", "screenshot"]);
        let v = ask(&r, CLICK);
        assert_eq!(code(&v), (Some(false), Some("target_window_mismatch"), Some(false)));
        assert!(v["hint"].as_str().unwrap().contains("Chrome"), "{v}");
        assert!(fake.sent().is_empty());
    }

    #[test]
    fn windows_that_do_not_touch_the_point_leave_the_frame_valid() {
        let fake = Fake::new();
        let r = game_runner(&fake);
        ask(&r, &["gui", "screenshot"]);
        fake.wins.lock().unwrap().insert(0, win(5, "Chrome", [0, 0, 100, 100]));
        assert_eq!(ask(&r, CLICK)["ok"].as_bool(), Some(true));
    }

    #[test]
    fn zoom_looks_without_comparing_layouts_and_input_still_refuses_a_stale_frame() {
        let fake = Fake::new();
        let r = game_runner(&fake);
        ask(&r, &["gui", "screenshot"]);
        // a window stacked over the click point (native 1000,500) after the frame: the zoom still answers, it only looks
        fake.wins.lock().unwrap().insert(0, win(6, "Chrome", [900, 400, 1100, 600]));
        assert_eq!(ask(&r, &["gui", "zoom", "100", "100", "300", "300"])["ok"].as_bool(), Some(true));
        // but a click at that place is refused, because the frame does not describe the window layout any more
        let v = ask(&r, CLICK);
        assert_eq!(v["ok"].as_bool(), Some(false));
        assert!(fake.sent().is_empty());
    }

    #[test]
    fn the_add_ons_own_window_is_never_a_target_and_never_makes_a_frame_stale() {
        let fake = Fake::new();
        let r = game_runner(&fake);
        ask(&r, &["gui", "screenshot"]);
        // the overlay appears over the point after the frame: not a layout change ...
        fake.wins.lock().unwrap().insert(0, WinInfo { own: true, ..win(9, "overlay", [0, 0, 2000, 1000]) });
        ask(&r, &["gui", "zoom", "100", "100", "300", "300"]);
        // ... but a click that would land on it is not aimed at the game
        let v = ask(&r, CLICK);
        assert_eq!(code(&v), (Some(false), Some("target_window_mismatch"), Some(false)));
        assert!(v["hint"].as_str().unwrap().contains("own window"), "{v}");
        assert!(fake.sent().is_empty());
    }

    #[test]
    fn a_point_on_no_monitor_is_refused() {
        let fake = Fake::new();
        *fake.monitors.lock().unwrap() = vec![[0, 0, 1000, 1000]];
        let r = game_runner(&fake);
        ask(&r, &["gui", "screenshot"]);
        let v = ask(&r, CLICK); // native x 1000: dead space
        assert_eq!(code(&v), (Some(false), Some("target_off_monitor"), Some(false)));
        let v = ask(&r, &["gui", "click", "100", "100"]);
        assert_eq!(v["ok"].as_bool(), Some(true));
        assert_eq!(fake.sent().len(), 1);
    }

    #[test]
    fn a_failed_send_before_anything_went_out_is_a_plain_failure() {
        let fake = Fake::new();
        *fake.fail_click.lock().unwrap() = Some((0, false));
        let r = game_runner(&fake);
        ask(&r, &["gui", "screenshot"]);
        let v = ask(&r, CLICK);
        assert_eq!(code(&v), (Some(false), Some("input_failed"), Some(false)));
    }

    #[test]
    fn input_that_stopped_half_way_is_effect_unknown_and_reports_what_was_done() {
        let fake = Fake::new();
        *fake.fail_click.lock().unwrap() = Some((1, false));
        let r = game_runner(&fake);
        ask(&r, &["gui", "screenshot"]);
        let v = ask(&r, &["gui", "--json", &two_clicks(100, 200)]);
        assert_eq!(code(&v), (Some(false), Some("effect_unknown"), Some(true)));
        assert_eq!(v["completed"].as_i64(), Some(1));
        assert_eq!(fake.sent().len(), 1);
        // one click that failed after the pointer moved is unknown too
        *fake.fail_click.lock().unwrap() = Some((2, true));
        let v = ask(&r, CLICK);
        assert_eq!(code(&v), (Some(false), Some("effect_unknown"), Some(true)));
    }

    #[test]
    fn a_change_between_the_actions_of_one_request_stops_the_rest() {
        let fake = Fake::new();
        // the first click opens a popup over the second point
        *fake.on_click.lock().unwrap() = Some(vec![win(5, "Popup", [1500, 0, 2000, 500]), win(13, "Blue Archive", [0, 0, 2000, 1000])]);
        let r = game_runner(&fake);
        ask(&r, &["gui", "screenshot"]);
        let v = ask(&r, &["gui", "--json", &two_clicks(100, 1300)]);
        assert_eq!(code(&v), (Some(false), Some("effect_unknown"), Some(true)));
        assert_eq!(v["completed"].as_i64(), Some(1));
        assert_eq!(fake.sent().len(), 1, "the second click was not sent");
    }

    #[test]
    fn every_input_call_leaves_one_audit_line_without_content() {
        let fake = Fake::new();
        let lines = Arc::new(Mutex::new(Vec::new()));
        let r = game_runner(&fake).with_audit(Box::new(Collect(lines.clone())));
        ask(&r, &["gui", "screenshot"]);
        ask(&r, &["gui", "zoom", "100", "100", "300", "300"]);
        assert!(lines.lock().unwrap().is_empty(), "reading is not logged");
        ask(&r, CLICK);
        ask(&r, &["gui", "--json", r#"{"actions":[{"type":"scroll","x":100,"y":100,"scroll_y":-2},{"type":"scroll","x":100,"y":100,"scroll_y":-2}]}"#]);
        fake.wins.lock().unwrap().insert(0, win(5, "Chrome", [0, 0, 2000, 1000]));
        ask(&r, CLICK);
        let l = lines.lock().unwrap().clone();
        assert_eq!(l.len(), 3);
        assert_eq!((l[0]["kind"].as_str(), l[0]["code"].as_str(), l[0]["input_emitted"].as_bool()), (Some("click"), Some("ok"), Some(true)));
        assert_eq!(l[0]["native"], json!([1000, 500]));
        assert_eq!(l[0]["window"], json!({"title": "Blue Archive", "class": "UnityWndClass"}));
        assert_eq!((l[1]["kind"].as_str(), l[1]["actions"].as_i64(), l[1]["completed"].as_i64()), (Some("scroll"), Some(2), Some(2)));
        assert_eq!((l[2]["code"].as_str(), l[2]["input_emitted"].as_bool(), l[2]["native"].is_null()), (Some("gui_frame_stale"), Some(false), true));
        let keys: Vec<&str> = l[0].as_object().unwrap().keys().map(String::as_str).collect();
        assert!(keys.iter().all(|k| ["t", "kind", "actions", "frame", "native", "window", "code", "input_emitted", "completed", "ms"].contains(k)), "{keys:?}");
    }

    #[test]
    fn input_waits_for_a_still_screen_but_not_longer_than_the_cap() {
        let fake = Fake::new();
        // a small game window, so the fake's pixel generation does not dominate the timing
        *fake.wins.lock().unwrap() = vec![win(13, "Blue Archive", [0, 0, 400, 300])];
        let click = ["gui", "click", "100", "100", "--records"];
        let settle = Settle { cap: Duration::from_millis(600), step: Duration::from_millis(60) };
        let r = game_runner(&fake).with_click_settle(settle);
        ask(&r, &["gui", "screenshot"]);
        let t = Instant::now();
        ask(&r, &click);
        let still = t.elapsed();
        *fake.animate.lock().unwrap() = true;
        let t = Instant::now();
        ask(&r, &click);
        let moving = t.elapsed();
        println!("settle latency: still screen {still:?}, never-still screen {moving:?} (cap 600 ms, step 60 ms)");
        assert!(still < Duration::from_millis(300), "{still:?}");
        assert!(moving >= Duration::from_millis(540) && moving < Duration::from_millis(900), "{moving:?}");
        let off = game_runner(&fake).with_click_settle(Settle::OFF);
        ask(&off, &["gui", "screenshot"]);
        let t = Instant::now();
        ask(&off, &click);
        assert!(t.elapsed() < Duration::from_millis(50));
        // the wait that is set for wheel movements does not hold up a click: callers wait for the screen themselves
        let wheel_only = game_runner(&fake).with_settle(settle);
        ask(&wheel_only, &["gui", "screenshot"]);
        let t = Instant::now();
        ask(&wheel_only, &click);
        assert!(t.elapsed() < Duration::from_millis(50), "{:?}", t.elapsed());
    }
}
