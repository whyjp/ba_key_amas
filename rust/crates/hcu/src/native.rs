//! In-process desktop control: the part of Microsoft's Hybrid Computer Use (MIT, `computer-use`
//! with its Windows provider) that this project uses, written against Win32 directly, so no
//! helper program is started.
//!
//! It answers the same requests the `computer-use` CLI did (`gui screenshot`, `gui zoom`, `gui click`,
//! `gui --json` with scroll/click actions, `apps`, `activate`) with the same JSON records, so
//! everything above [`Runner`] is unchanged, and the old process backend can still be used to
//! compare the two (`BAOCR_HCU=process`).
//!
//! Behaviour follows the reference implementation (`NativeDesktop*.cs`, `WindowsDesktopImageGeometry.cs`):
//! the screenshot is the whole virtual desktop delivered at no more than 1366x768; zoom regions and
//! click points are in that delivered frame; a click point maps to the native pixel
//! `(u + 0.5) * scale - 0.5` and is sent as a `SendInput` absolute coordinate on the virtual desktop;
//! a wheel notch is 120 units. Input is refused (nothing is sent) when the desktop's size is no longer
//! the one the last frame was taken at: the caller takes a fresh screenshot and repeats.
use crate::hcu::Runner;
use anyhow::{anyhow, bail, Result};
use image::{imageops, RgbImage};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

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
    /// Move the pointer to a desktop pixel and press/release the left button.
    fn click(&self, x: i32, y: i32) -> Result<()>;
    /// Move the pointer to a desktop pixel and turn the wheel by `notches` (positive = up).
    fn wheel(&self, x: i32, y: i32, notches: i32) -> Result<()>;
    /// `(ref, title, active)` of the visible top-level windows with a title.
    fn windows(&self) -> Vec<(String, String, bool)>;
    fn activate(&self, window_ref: &str) -> Result<()>;
}

/// Answers the CLI's requests from a [`Desktop`].
pub struct NativeRunner<D: Desktop> {
    desktop: D,
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    /// Native size of the desktop at the last screenshot: input is refused when it differs.
    frame_native: Option<(u32, u32)>,
    images: HashMap<String, RgbImage>,
    next: u64,
}

impl<D: Desktop> NativeRunner<D> {
    pub fn new(desktop: D) -> Self {
        Self { desktop, state: Mutex::new(State::default()) }
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

    fn screenshot(&self) -> Result<String> {
        let (x, y, w, h) = self.desktop.bounds();
        let img = self.desktop.grab(x, y, w, h)?;
        let (dw, dh) = delivered_size((w, h));
        let delivered = if (dw, dh) == (w, h) { img } else { imageops::resize(&img, dw, dh, imageops::FilterType::CatmullRom) };
        self.state.lock().unwrap().frame_native = Some((w, h));
        let id = self.store(delivered);
        Ok(json!({"ok": true, "images": [id], "screen": {"width": dw, "height": dh}}).to_string())
    }

    fn zoom(&self, region: [f64; 4]) -> Result<String> {
        let native = self.native();
        if self.state.lock().unwrap().frame_native != Some(native) {
            return Ok(failure("gui_frame_stale", "the desktop's size changed since the last screenshot; take a new one"));
        }
        let g = match zoom_geometry(region, native) {
            Ok(g) => g,
            Err(e) => return Ok(failure("invalid_zoom_region", &e.to_string())),
        };
        let (ox, oy, _, _) = self.desktop.bounds();
        let crop = self.desktop.grab(ox + g.native.0, oy + g.native.1, g.native.2 as u32, g.native.3 as u32)?;
        let img = if g.size == crop.dimensions() { crop } else { imageops::resize(&crop, g.size.0, g.size.1, imageops::FilterType::CatmullRom) };
        let id = self.store(img);
        let (sx, sy) = (g.size.0 as f64 / (g.region[2] - g.region[0]), g.size.1 as f64 / (g.region[3] - g.region[1]));
        Ok(json!({"ok": true, "images": [id], "evidence": {"zoom": {"region": g.region, "scale_x": sx, "scale_y": sy}}}).to_string())
    }

    /// Where a frame point is on the desktop, or None when the frame is stale.
    fn locate(&self, x: f64, y: f64) -> Option<(i32, i32)> {
        let (ox, oy, w, h) = self.desktop.bounds();
        if self.state.lock().unwrap().frame_native != Some((w, h)) {
            return None;
        }
        let (dw, dh) = delivered_size((w, h));
        if !(x.is_finite() && y.is_finite() && x >= 0.0 && y >= 0.0 && x <= dw as f64 - 1.0 && y <= dh as f64 - 1.0) {
            return None;
        }
        Some((ox + to_native(x, dw, w).round() as i32, oy + to_native(y, dh, h).round() as i32))
    }

    fn input(&self, actions: &[Action]) -> Result<String> {
        // every point is checked before anything is sent
        let mut points = Vec::new();
        for a in actions {
            let (x, y) = match a {
                Action::Click { x, y } | Action::Scroll { x, y, .. } => (*x, *y),
            };
            match self.locate(x, y) {
                Some(p) => points.push(p),
                None => return Ok(failure("gui_frame_stale", "the point is outside the last frame or the desktop changed; take a new screenshot")),
            }
        }
        let mut done = 0;
        for (a, (px, py)) in actions.iter().zip(points) {
            match a {
                Action::Click { .. } => self.desktop.click(px, py)?,
                Action::Scroll { delta, .. } => self.desktop.wheel(px, py, -*delta)?,
            }
            done += 1;
        }
        let (dw, dh) = delivered_size(self.native());
        Ok(json!({"ok": true, "input_emitted": done > 0, "completed": done, "screen": {"width": dw, "height": dh}}).to_string())
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
        fn AttachThreadInput(from: u32, to: u32, attach: i32) -> i32;
        fn keybd_event(vk: u8, scan: u8, flags: u32, extra: usize);
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
    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentThreadId() -> u32;
    }

    const SRCCOPY_CAPTUREBLT: u32 = 0x00CC_0020 | 0x4000_0000;
    const MOVE: u32 = 0x0001;
    const LEFT_DOWN: u32 = 0x0002;
    const LEFT_UP: u32 = 0x0004;
    const WHEEL: u32 = 0x0800;
    const ABSOLUTE: u32 = 0x8000;
    const VIRTUAL_DESK: u32 = 0x4000;
    const SW_RESTORE: i32 = 9;
    const VK_MENU: u8 = 0x12;
    const KEYEVENTF_KEYUP: u32 = 2;

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

        fn click(&self, x: i32, y: i32) -> Result<()> {
            self.move_to(x, y)?;
            self.mouse(LEFT_DOWN, 0, 0, 0)?;
            if let Err(e) = self.mouse(LEFT_UP, 0, 0, 0) {
                let _ = self.mouse(LEFT_UP, 0, 0, 0); // never leave the button down
                return Err(e);
            }
            Ok(())
        }

        fn wheel(&self, x: i32, y: i32, notches: i32) -> Result<()> {
            self.move_to(x, y)?;
            self.mouse(WHEEL, 0, 0, (notches * 120) as u32)?;
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

        fn activate(&self, window_ref: &str) -> Result<()> {
            let hwnd: isize = window_ref.strip_prefix('w').and_then(|n| n.parse().ok()).ok_or_else(|| anyhow!("bad window reference {window_ref}"))?;
            unsafe {
                if IsWindow(hwnd) == 0 {
                    bail!("window {window_ref} is gone");
                }
                if IsIconic(hwnd) != 0 {
                    ShowWindow(hwnd, SW_RESTORE);
                }
                // Windows only lets the foreground owner hand the focus over: attach to its input
                // queue (and tap Alt) for the moment of the switch.
                let fg = GetForegroundWindow();
                let (fg_thread, me) = (GetWindowThreadProcessId(fg, std::ptr::null_mut()), GetCurrentThreadId());
                let attached = fg_thread != 0 && fg_thread != me && AttachThreadInput(me, fg_thread, 1) != 0;
                keybd_event(VK_MENU, 0, 0, 0);
                keybd_event(VK_MENU, 0, KEYEVENTF_KEYUP, 0);
                SetForegroundWindow(hwnd);
                if attached {
                    AttachThreadInput(me, fg_thread, 0);
                }
                for _ in 0..30 {
                    if GetForegroundWindow() == hwnd {
                        return Ok(());
                    }
                    std::thread::sleep(Duration::from_millis(25));
                }
            }
            bail!("window {window_ref} did not become the active window")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb;

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

    /// A 2000x1000 desktop whose left half is red and right half green; records what is sent.
    struct Fake {
        log: Mutex<Vec<String>>,
        size: Mutex<(u32, u32)>,
    }
    impl Fake {
        fn new() -> Self {
            Fake { log: Mutex::new(Vec::new()), size: Mutex::new((2000, 1000)) }
        }
    }
    impl Desktop for &Fake {
        fn bounds(&self) -> (i32, i32, u32, u32) {
            let s = *self.size.lock().unwrap();
            (0, 0, s.0, s.1)
        }
        fn grab(&self, x: i32, y: i32, w: u32, h: u32) -> Result<RgbImage> {
            self.log.lock().unwrap().push(format!("grab {x} {y} {w} {h}"));
            Ok(RgbImage::from_fn(w, h, |px, _| if (x as u32 + px) < 1000 { Rgb([200, 0, 0]) } else { Rgb([0, 200, 0]) }))
        }
        fn click(&self, x: i32, y: i32) -> Result<()> {
            self.log.lock().unwrap().push(format!("click {x} {y}"));
            Ok(())
        }
        fn wheel(&self, x: i32, y: i32, n: i32) -> Result<()> {
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
}
