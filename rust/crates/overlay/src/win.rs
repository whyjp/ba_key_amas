//! Win32 layered window that shows the banner over the game.
//!
//! WS_EX_LAYERED + per-pixel alpha (UpdateLayeredWindow), WS_EX_TRANSPARENT
//! (clicks pass through), WS_EX_NOACTIVATE (never takes focus), WS_EX_TOPMOST,
//! WS_EX_TOOLWINDOW (no taskbar button). WDA_EXCLUDEFROMCAPTURE keeps it out of
//! screenshots where the OS supports it.
use crate::panel::{panel_origin, panel_width, render_panel};
use crate::render::{banner_layout, banner_origin, load_fonts, render, Fonts, Ltrb};
use crate::Shown;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

#[repr(C)]
struct Point {
    x: i32,
    y: i32,
}
#[repr(C)]
struct Size {
    cx: i32,
    cy: i32,
}
#[repr(C)]
struct Rect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}
#[repr(C)]
struct MonitorInfo {
    size: u32,
    monitor: Rect,
    work: Rect,
    flags: u32,
}
#[repr(C)]
struct Msg {
    hwnd: isize,
    message: u32,
    wparam: usize,
    lparam: isize,
    time: u32,
    pt: Point,
    private: u32,
}
#[repr(C)]
struct WndClassExW {
    cb_size: u32,
    style: u32,
    wnd_proc: unsafe extern "system" fn(isize, u32, usize, isize) -> isize,
    cb_cls_extra: i32,
    cb_wnd_extra: i32,
    instance: isize,
    icon: isize,
    cursor: isize,
    background: isize,
    menu_name: *const u16,
    class_name: *const u16,
    icon_sm: isize,
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
#[repr(C)]
struct BitmapInfo {
    header: BitmapInfoHeader,
    colors: [u32; 1],
}
#[repr(C)]
struct BlendFunction {
    op: u8,
    flags: u8,
    alpha: u8,
    format: u8,
}

#[link(name = "user32")]
extern "system" {
    fn SetProcessDPIAware() -> i32;
    fn RegisterClassExW(c: *const WndClassExW) -> u16;
    fn CreateWindowExW(ex: u32, class: *const u16, title: *const u16, style: u32, x: i32, y: i32, w: i32, h: i32, parent: isize, menu: isize, inst: isize, param: *const c_void) -> isize;
    fn DefWindowProcW(h: isize, m: u32, w: usize, l: isize) -> isize;
    fn ShowWindow(h: isize, cmd: i32) -> i32;
    fn DestroyWindow(h: isize) -> i32;
    fn PeekMessageW(m: *mut Msg, h: isize, min: u32, max: u32, remove: u32) -> i32;
    fn TranslateMessage(m: *const Msg) -> i32;
    fn DispatchMessageW(m: *const Msg) -> isize;
    fn UpdateLayeredWindow(h: isize, dst: isize, pos: *const Point, size: *const Size, src: isize, src_pos: *const Point, key: u32, blend: *const BlendFunction, flags: u32) -> i32;
    fn SetWindowDisplayAffinity(h: isize, affinity: u32) -> i32;
    fn GetDC(h: isize) -> isize;
    fn ReleaseDC(h: isize, dc: isize) -> i32;
    fn FindWindowW(class: *const u16, title: *const u16) -> isize;
    fn GetClientRect(h: isize, r: *mut Rect) -> i32;
    fn GetWindowRect(h: isize, r: *mut Rect) -> i32;
    fn MonitorFromWindow(h: isize, flags: u32) -> isize;
    fn GetMonitorInfoW(m: isize, info: *mut MonitorInfo) -> i32;
    fn ClientToScreen(h: isize, p: *mut Point) -> i32;
    fn IsIconic(h: isize) -> i32;
}
#[link(name = "gdi32")]
extern "system" {
    fn CreateCompatibleDC(dc: isize) -> isize;
    fn CreateDIBSection(dc: isize, bmi: *const BitmapInfo, usage: u32, bits: *mut *mut c_void, section: isize, offset: u32) -> isize;
    fn SelectObject(dc: isize, obj: isize) -> isize;
    fn DeleteObject(obj: isize) -> i32;
    fn DeleteDC(dc: isize) -> i32;
}
#[link(name = "kernel32")]
extern "system" {
    fn GetModuleHandleW(name: *const u16) -> isize;
}

const WS_POPUP: u32 = 0x8000_0000;
const WS_EX_TOPMOST: u32 = 0x8;
const WS_EX_TRANSPARENT: u32 = 0x20;
const WS_EX_TOOLWINDOW: u32 = 0x80;
const WS_EX_LAYERED: u32 = 0x8_0000;
const WS_EX_NOACTIVATE: u32 = 0x0800_0000;
const SW_HIDE: i32 = 0;
const SW_SHOWNOACTIVATE: i32 = 4;
const ULW_ALPHA: u32 = 2;
const WDA_EXCLUDEFROMCAPTURE: u32 = 0x11;
const PM_REMOVE: u32 = 1;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

unsafe extern "system" fn wnd_proc(h: isize, m: u32, w: usize, l: isize) -> isize {
    DefWindowProcW(h, m, w, l)
}

/// Where the game is: client area and whole window (both left/top/right/bottom, screen
/// pixels) and the work area of the monitor it is on. None if hidden or minimised.
fn game_geometry() -> Option<(Ltrb, Ltrb, Ltrb)> {
    unsafe {
        let title = std::env::var("BAOCR_OVERLAY_TITLE").unwrap_or_else(|_| "Blue Archive".into()); // override: demos/tests only
        let hwnd = FindWindowW(std::ptr::null(), wide(&title).as_ptr());
        if hwnd == 0 || IsIconic(hwnd) != 0 {
            return None;
        }
        let mut c = Rect { left: 0, top: 0, right: 0, bottom: 0 };
        let mut o = Rect { left: 0, top: 0, right: 0, bottom: 0 };
        let mut p = Point { x: 0, y: 0 };
        if GetClientRect(hwnd, &mut c) == 0 || ClientToScreen(hwnd, &mut p) == 0 || GetWindowRect(hwnd, &mut o) == 0 {
            return None;
        }
        let (cw, ch) = (c.right - c.left, c.bottom - c.top);
        if cw <= 100 || ch <= 100 {
            return None;
        }
        let mut mi = MonitorInfo { size: std::mem::size_of::<MonitorInfo>() as u32, monitor: Rect { left: 0, top: 0, right: 0, bottom: 0 }, work: Rect { left: 0, top: 0, right: 0, bottom: 0 }, flags: 0 };
        let hmon = MonitorFromWindow(hwnd, 2); // MONITOR_DEFAULTTONEAREST
        let work = if hmon != 0 && GetMonitorInfoW(hmon, &mut mi) != 0 {
            (mi.work.left, mi.work.top, mi.work.right, mi.work.bottom)
        } else {
            (i32::MIN / 2, i32::MIN / 2, i32::MAX / 2, i32::MAX / 2)
        };
        Some(((p.x, p.y, p.x + cw, p.y + ch), (o.left, o.top, o.right, o.bottom), work))
    }
}

/// A premultiplied-BGRA DIB the banner is rendered into.
struct Canvas {
    dc: isize,
    bitmap: isize,
    old: isize,
    bits: *mut u8,
    w: i32,
    h: i32,
}

impl Canvas {
    unsafe fn new(screen_dc: isize, w: i32, h: i32) -> Option<Canvas> {
        let dc = CreateCompatibleDC(screen_dc);
        if dc == 0 {
            return None;
        }
        let bmi = BitmapInfo {
            header: BitmapInfoHeader { size: std::mem::size_of::<BitmapInfoHeader>() as u32, width: w, height: -h, planes: 1, bit_count: 32, compression: 0, size_image: 0, x_ppm: 0, y_ppm: 0, clr_used: 0, clr_important: 0 },
            colors: [0],
        };
        let mut bits: *mut c_void = std::ptr::null_mut();
        let bitmap = CreateDIBSection(dc, &bmi, 0, &mut bits, 0, 0);
        if bitmap == 0 || bits.is_null() {
            DeleteDC(dc);
            return None;
        }
        let old = SelectObject(dc, bitmap);
        Some(Canvas { dc, bitmap, old, bits: bits as *mut u8, w, h })
    }

    unsafe fn blit(&mut self, img: &image::RgbaImage) {
        let n = (self.w * self.h) as usize;
        let dst = std::slice::from_raw_parts_mut(self.bits, n * 4);
        for (i, p) in img.pixels().take(n).enumerate() {
            let a = p[3] as u32;
            // premultiplied BGRA
            dst[i * 4] = (p[2] as u32 * a / 255) as u8;
            dst[i * 4 + 1] = (p[1] as u32 * a / 255) as u8;
            dst[i * 4 + 2] = (p[0] as u32 * a / 255) as u8;
            dst[i * 4 + 3] = a as u8;
        }
    }
}

impl Drop for Canvas {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.dc, self.old);
            DeleteObject(self.bitmap);
            DeleteDC(self.dc);
        }
    }
}

pub struct Worker {
    stop: Arc<AtomicBool>,
    handle: JoinHandle<()>,
}

impl Worker {
    pub fn start(state: Arc<Mutex<Shown>>) -> Option<Worker> {
        let fonts = match load_fonts() {
            Some(f) => f,
            None => {
                eprintln!("overlay: no Korean system font found; running without the banner");
                return None;
            }
        };
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let handle = std::thread::spawn(move || run(state, flag, fonts));
        Some(Worker { stop, handle })
    }

    pub fn stop(self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = self.handle.join();
    }
}

fn run(state: Arc<Mutex<Shown>>, stop: Arc<AtomicBool>, fonts: Fonts) {
    unsafe {
        SetProcessDPIAware();
        let inst = GetModuleHandleW(std::ptr::null());
        let class = wide("BaScannerBanner");
        let wc = WndClassExW {
            cb_size: std::mem::size_of::<WndClassExW>() as u32,
            style: 0,
            wnd_proc,
            cb_cls_extra: 0,
            cb_wnd_extra: 0,
            instance: inst,
            icon: 0,
            cursor: 0,
            background: 0,
            menu_name: std::ptr::null(),
            class_name: class.as_ptr(),
            icon_sm: 0,
        };
        RegisterClassExW(&wc); // already registered on a second run: harmless
        let title = wide("ba-scanner");
        let hwnd = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            class.as_ptr(),
            title.as_ptr(),
            WS_POPUP,
            0,
            0,
            10,
            10,
            0,
            0,
            inst,
            std::ptr::null(),
        );
        if hwnd == 0 {
            eprintln!("overlay: could not create the banner window");
            return;
        }
        // Keep the banner out of the screenshots the scanner reads (Windows 10 2004+).
        let capturable = std::env::var_os("BAOCR_OVERLAY_CAPTURABLE").is_some(); // tests only
        if !capturable && SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE) == 0 {
            // Without capture exclusion the banner would appear in the frames being read
            // and hide fields: reading the game matters more than showing progress.
            eprintln!("overlay: this Windows version cannot hide the banner from screen capture; running without it");
            DestroyWindow(hwnd);
            return;
        }
        let screen_dc = GetDC(0);
        let mut canvas: Option<Canvas> = None;
        let mut visible = false;
        let mut rect = None;
        let mut tick: u32 = 0;
        while !stop.load(Ordering::Relaxed) {
            let mut msg: Msg = std::mem::zeroed();
            while PeekMessageW(&mut msg, 0, 0, 0, PM_REMOVE) != 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            if tick % 8 == 0 {
                rect = game_geometry(); // follow the window if it moves
            }
            match rect {
                None => {
                    if visible {
                        ShowWindow(hwnd, SW_HIDE);
                        visible = false;
                    }
                }
                Some((client, outer, work)) => {
                    let (cw, ch) = (client.2 - client.0, client.3 - client.1);
                    let shown = state.lock().map(|g| g.clone()).unwrap_or(Shown::Hidden);
                    // what to draw and where: the banner goes above/below the window, the panel beside it
                    let placed = match &shown {
                        Shown::Hidden => None,
                        Shown::Banner(s) => {
                            let (_, _, bw, _) = banner_layout(cw, ch); // proportional to this window
                            let img = render(bw, s, tick, &fonts);
                            let at = banner_origin(client, outer, work, (img.width() as i32, img.height() as i32));
                            Some((img, at))
                        }
                        Shown::Panel(p) => {
                            let img = render_panel(panel_width(cw) as u32, p, &fonts);
                            panel_origin(client, outer, work, (img.width() as i32, img.height() as i32)).map(|at| (img, at))
                        }
                    };
                    match placed {
                        None => {
                            if visible {
                                ShowWindow(hwnd, SW_HIDE);
                                visible = false;
                            }
                        }
                        Some((img, (px, py))) => {
                            let (w, h) = (img.width() as i32, img.height() as i32);
                            if canvas.as_ref().map(|c| (c.w, c.h)) != Some((w, h)) {
                                canvas = Canvas::new(screen_dc, w, h);
                            }
                            if let Some(c) = canvas.as_mut() {
                                c.blit(&img);
                                let pos = Point { x: px, y: py };
                                let size = Size { cx: w, cy: h };
                                let src = Point { x: 0, y: 0 };
                                let blend = BlendFunction { op: 0, flags: 0, alpha: 255, format: 1 };
                                UpdateLayeredWindow(hwnd, screen_dc, &pos, &size, c.dc, &src, 0, &blend, ULW_ALPHA);
                                if !visible {
                                    ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                                    visible = true;
                                }
                            }
                        }
                    }
                }
            }
            tick = tick.wrapping_add(1);
            std::thread::sleep(Duration::from_millis(90));
        }
        drop(canvas);
        ReleaseDC(0, screen_dc);
        DestroyWindow(hwnd);
    }
}
