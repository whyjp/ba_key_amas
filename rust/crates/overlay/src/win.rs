//! Win32 layered window that shows the banner over the game.
//!
//! WS_EX_LAYERED + per-pixel alpha (UpdateLayeredWindow), WS_EX_TRANSPARENT
//! (clicks pass through), WS_EX_NOACTIVATE (never takes focus), WS_EX_TOPMOST,
//! WS_EX_TOOLWINDOW (no taskbar button). WDA_EXCLUDEFROMCAPTURE keeps it out of
//! screenshots where the OS supports it.
use crate::cache::{Memo, Tracker};
use crate::dock::{dock_from_drop, dock_origin, Dock};
use crate::panel::{ingame_origin, ingame_panel_width, panel_width, render_panel, render_panel_hits, render_tooltip, Hit, PanelState, DRAG_HANDLE, PANEL_BASE_W};
use crate::render::{load_fonts, Fonts, Ltrb};
use crate::marks::{render_marks, Mark};
use crate::wake::Wake;
use crate::{Card, Shown};
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// `BAOCR_PROFILE=1`: time the parts of the overlay thread's tick and print the totals every 5 s.
mod prof {
    use super::*;

    #[derive(Clone, Copy)]
    pub enum Part {
        Geometry,
        Covered,
        RenderCard,
        RenderIngame,
        RenderMarks,
        Blit,
        Update,
        Busy,
    }
    const NAMES: [&str; 8] = ["game_geometry", "covered", "render_card", "render_ingame", "render_marks", "blit", "UpdateLayeredWindow", "tick busy"];
    static ON: AtomicBool = AtomicBool::new(false);
    static NS: [AtomicU64; 8] = [const { AtomicU64::new(0) }; 8];
    static N: [AtomicU32; 8] = [const { AtomicU32::new(0) }; 8];

    pub fn enable() {
        ON.store(std::env::var_os("BAOCR_PROFILE").is_some(), Ordering::Relaxed);
    }

    pub fn time<T>(part: Part, f: impl FnOnce() -> T) -> T {
        if !ON.load(Ordering::Relaxed) {
            return f();
        }
        let t = Instant::now();
        let r = f();
        add(part, t.elapsed());
        r
    }

    pub fn add(part: Part, d: Duration) {
        if ON.load(Ordering::Relaxed) {
            NS[part as usize].fetch_add(d.as_nanos() as u64, Ordering::Relaxed);
            N[part as usize].fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Print and reset the totals once `window` has passed since `since`.
    pub fn report(since: &mut Instant, window: Duration) {
        if !ON.load(Ordering::Relaxed) || since.elapsed() < window {
            return;
        }
        let secs = since.elapsed().as_secs_f64();
        *since = Instant::now();
        let ticks = N[Part::Busy as usize].swap(0, Ordering::Relaxed).max(1);
        let mut line = format!("overlay profile {secs:.1}s, {ticks} ticks:");
        for (i, name) in NAMES.iter().enumerate() {
            let (ns, n) = (NS[i].swap(0, Ordering::Relaxed), if i == Part::Busy as usize { ticks } else { N[i].swap(0, Ordering::Relaxed) });
            if n > 0 {
                line += &format!(" {name} {:.2}ms x{n} (sum {:.0}ms);", ns as f64 / 1e6 / n as f64, ns as f64 / 1e6);
            }
        }
        eprintln!("{line}");
    }
}

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
    fn GetForegroundWindow() -> isize;
    fn GetCursorPos(p: *mut Point) -> i32;
    fn GetAsyncKeyState(vk: i32) -> i16;
    fn GetClassNameW(h: isize, buf: *mut u16, max: i32) -> i32;
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

/// Left clicks on the overlay's windows: (window, x, y) in the window's own pixels.
static CLICKS: Mutex<Vec<(isize, i32, i32)>> = Mutex::new(Vec::new());
const WM_LBUTTONDOWN: u32 = 0x0201;

unsafe extern "system" fn wnd_proc(h: isize, m: u32, w: usize, l: isize) -> isize {
    if m == WM_LBUTTONDOWN {
        if let Ok(mut c) = CLICKS.lock() {
            c.push((h, (l & 0xFFFF) as i16 as i32, ((l >> 16) & 0xFFFF) as i16 as i32));
        }
        return 0;
    }
    DefWindowProcW(h, m, w, l)
}

/// Where the game is: client area and whole window (both left/top/right/bottom, screen
/// pixels) and the work area of the monitor it is on. None if hidden or minimised.
fn game_geometry() -> Option<(Ltrb, Ltrb, Ltrb, isize)> {
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
        Some(((p.x, p.y, p.x + cw, p.y + ch), (o.left, o.top, o.right, o.bottom), work, hwnd))
    }
}

/// Is another window in front of the game and over it? Then the overlay must not be drawn on
/// top of that window. The game itself, the overlay's own windows and the desktop shell do not count.
unsafe fn covered(game: isize, own: [isize; 4], outer: Ltrb) -> bool {
    let fg = GetForegroundWindow();
    if fg == 0 || fg == game || own.contains(&fg) {
        return false;
    }
    let mut class = [0u16; 64];
    let n = GetClassNameW(fg, class.as_mut_ptr(), 64).max(0) as usize;
    let class = String::from_utf16_lossy(&class[..n]);
    if matches!(class.as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd") {
        return false;
    }
    let mut r = Rect { left: 0, top: 0, right: 0, bottom: 0 };
    GetWindowRect(fg, &mut r) != 0 && r.left < outer.2 && r.right > outer.0 && r.top < outer.3 && r.bottom > outer.1
}

const VK_LBUTTON: i32 = 0x01;

unsafe fn left_button_down() -> bool {
    GetAsyncKeyState(VK_LBUTTON) as u16 & 0x8000 != 0
}

unsafe fn cursor() -> (i32, i32) {
    let mut p = Point { x: 0, y: 0 };
    GetCursorPos(&mut p);
    (p.x, p.y)
}

/// The work area of the primary monitor (taskbar excluded).
fn primary_work() -> Ltrb {
    unsafe {
        let mut mi = MonitorInfo { size: std::mem::size_of::<MonitorInfo>() as u32, monitor: Rect { left: 0, top: 0, right: 0, bottom: 0 }, work: Rect { left: 0, top: 0, right: 0, bottom: 0 }, flags: 0 };
        let hmon = MonitorFromWindow(0, 1); // MONITOR_DEFAULTTOPRIMARY
        if hmon != 0 && GetMonitorInfoW(hmon, &mut mi) != 0 {
            (mi.work.left, mi.work.top, mi.work.right, mi.work.bottom)
        } else {
            (0, 0, 1920, 1080)
        }
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
    pub fn start(state: Arc<Mutex<Shown>>, events: Arc<Mutex<Vec<String>>>, wake: Arc<Wake>) -> Option<Worker> {
        let fonts = match load_fonts() {
            Some(f) => f,
            None => {
                eprintln!("overlay: no Korean system font found; running without the banner");
                return None;
            }
        };
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let handle = std::thread::spawn(move || run(state, events, wake, flag, fonts));
        Some(Worker { stop, handle })
    }

    pub fn stop(self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = self.handle.join();
    }
}

/// One layered, click-through, capture-excluded window and the bitmap it shows.
struct Layer {
    hwnd: isize,
    canvas: Option<Canvas>,
    visible: bool,
    /// What was last drawn (a key of the content, position and size), to skip identical redraws.
    drawn: Option<u64>,
}

/// `click_through`: mouse clicks go to whatever is below (the game). The card beside the game
/// is not click-through, so its buttons work; it never takes focus either way.
unsafe fn create_layer(inst: isize, class: &[u16], title: &str, click_through: bool) -> Option<Layer> {
    let title = wide(title);
    let ex = WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | if click_through { WS_EX_TRANSPARENT } else { 0 };
    let hwnd = CreateWindowExW(ex, class.as_ptr(), title.as_ptr(), WS_POPUP, 0, 0, 10, 10, 0, 0, inst, std::ptr::null());
    if hwnd == 0 {
        return None;
    }
    // Keep it out of the screenshots the scanner reads (Windows 10 2004+).
    let capturable = std::env::var_os("BAOCR_OVERLAY_CAPTURABLE").is_some(); // tests only
    if !capturable && SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE) == 0 {
        // Without capture exclusion the overlay would appear in the frames being read and
        // hide fields: reading the game matters more than showing anything.
        DestroyWindow(hwnd);
        return None;
    }
    Some(Layer { hwnd, canvas: None, visible: false, drawn: None })
}

impl Layer {
    unsafe fn hide(&mut self) {
        if self.visible {
            ShowWindow(self.hwnd, SW_HIDE);
            self.visible = false;
            self.drawn = None;
        }
    }

    /// Show `img` at screen position `pos`; `key` identifies the content so that an
    /// unchanged layer is not redrawn.
    unsafe fn show(&mut self, screen_dc: isize, img: &image::RgbaImage, pos: (i32, i32), key: u64) {
        if self.visible && self.drawn == Some(key) {
            return;
        }
        let (w, h) = (img.width() as i32, img.height() as i32);
        if self.canvas.as_ref().map(|c| (c.w, c.h)) != Some((w, h)) {
            self.canvas = Canvas::new(screen_dc, w, h);
        }
        if let Some(c) = self.canvas.as_mut() {
            prof::time(prof::Part::Blit, || c.blit(img));
            let (p, size, src) = (Point { x: pos.0, y: pos.1 }, Size { cx: w, cy: h }, Point { x: 0, y: 0 });
            let blend = BlendFunction { op: 0, flags: 0, alpha: 255, format: 1 };
            prof::time(prof::Part::Update, || UpdateLayeredWindow(self.hwnd, screen_dc, &p, &size, c.dc, &src, 0, &blend, ULW_ALPHA));
            if !self.visible {
                ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
                self.visible = true;
            }
            self.drawn = Some(key);
        }
    }
}

fn hash_of<T: std::hash::Hash>(v: &T) -> u64 {
    use std::hash::Hasher;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    v.hash(&mut h);
    h.finish()
}

fn run(state: Arc<Mutex<Shown>>, events: Arc<Mutex<Vec<String>>>, wake: Arc<Wake>, stop: Arc<AtomicBool>, fonts: Fonts) {
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
        let (Some(mut card), Some(mut ingame), Some(mut marks), Some(mut tip)) = (
            create_layer(inst, &class, "ba-scanner", false),
            create_layer(inst, &class, "ba-scanner-ingame", true),
            create_layer(inst, &class, "ba-scanner-marks", true),
            create_layer(inst, &class, "ba-scanner-tip", true),
        ) else {
            eprintln!("overlay: this Windows version cannot hide the overlay from screen capture (or the window could not be created); running without it");
            return;
        };
        let screen_dc = GetDC(0);
        let mut rect = None;
        let mut tick: u32 = 0;
        let mut card_hits: Vec<Hit> = Vec::new();
        // a drag of the card by its header: where in the card it was picked up, and where it is now
        let mut drag: Option<(i32, i32)> = None;
        let mut drag_at: Option<(i32, i32)> = None;
        let (mut card_track, mut ingame_track, mut marks_track) = (Tracker::<Card>::new(), Tracker::<Option<PanelState>>::new(), Tracker::<Vec<Mark>>::new());
        let (mut card_gen, mut ingame_gen, mut marks_gen, mut dock_now) = (0u64, 0u64, 0u64, Dock::default());
        let mut card_memo: Memo<(u64, u32), (image::RgbaImage, Vec<Hit>)> = Memo::new();
        let mut ingame_memo: Memo<(u64, u32), image::RgbaImage> = Memo::new();
        let mut marks_memo: Memo<(u64, i32, i32), image::RgbaImage> = Memo::new();
        let mut tip_memo: Memo<(String, i32), image::RgbaImage> = Memo::new();
        let mut hits_gen = 0u64;
        prof::enable();
        let mut prof_since = Instant::now();
        while !stop.load(Ordering::Relaxed) {
            let tick_start = Instant::now();
            prof::report(&mut prof_since, Duration::from_secs(5));
            let mut msg: Msg = std::mem::zeroed();
            while PeekMessageW(&mut msg, 0, 0, 0, PM_REMOVE) != 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            if tick % 8 == 0 {
                rect = prof::time(prof::Part::Geometry, game_geometry); // follow the window if it moves
            }
            // clicks on the card's buttons
            let game_present_now = rect.is_some();
            let clicks: Vec<(isize, i32, i32)> = CLICKS.lock().map(|mut c| std::mem::take(&mut *c)).unwrap_or_default();
            for (h, x, y) in clicks {
                if h == card.hwnd {
                    if let Some(hit) = card_hits.iter().find(|hit| hit.contains(x, y) && hit.tip.is_none()) {
                        if hit.id == DRAG_HANDLE {
                            if left_button_down() && game_present_now {
                                drag = Some((x, y));
                            }
                        } else if let Ok(mut e) = events.lock() {
                            e.push(hit.id.clone());
                        }
                    }
                }
            }
            // The card is always there while the add-on runs: against the game when it is found,
            // else waiting at the top right of the screen (so it can still be used, and closed).
            let game_present = rect.is_some();
            let (client, outer, work, game) = rect.unwrap_or_else(|| {
                let w = primary_work();
                ((w.2 - 2, w.1, w.2, w.1 + 200), (w.2 - 2, w.1, w.2, w.1 + 200), w, 0)
            });
            if game_present && prof::time(prof::Part::Covered, || covered(game, [card.hwnd, ingame.hwnd, marks.hwnd, tip.hwnd], outer)) {
                card.hide();
                ingame.hide();
                marks.hide();
                tip.hide();
                tick = tick.wrapping_add(1);
                prof::add(prof::Part::Busy, tick_start.elapsed());
                wake.wait(Duration::from_millis(90));
                continue;
            }
            let (cw, ch) = (client.2 - client.0, client.3 - client.1);
            // what changed since the last tick: nothing is drawn again unless it did
            if let Ok(g) = state.lock() {
                card_gen = card_track.update(&g.card);
                ingame_gen = ingame_track.update(&g.ingame);
                marks_gen = marks_track.update(&g.marks);
                dock_now = g.dock;
            }
            let mut card_rect: Option<((i32, i32), (i32, i32))> = None;
            match card_track.current() {
                Some(Card::Panel(p)) => {
                    let width = panel_width(cw) as u32;
                    let ((img, hits), render_gen) = card_memo.get((card_gen, width), || prof::time(prof::Part::RenderCard, || render_panel_hits(width, p, &fonts)));
                    let size = (img.width() as i32, img.height() as i32);
                    // against the game on the docked side; with no room, inside it rather than nowhere
                    let docked = if game_present {
                        dock_origin(client, outer, work, size, dock_now).unwrap_or_else(|| ingame_origin(client, size))
                    } else {
                        // no game: wait at the top right of the screen
                        ((work.2 - size.0 - 12).max(work.0), work.1 + 12)
                    };
                    let at = match drag {
                        Some((gx, gy)) if left_button_down() => {
                            let (cx, cy) = cursor();
                            let p = (cx - gx, cy - gy);
                            drag_at = Some(p);
                            p
                        }
                        Some(_) => {
                            // let go: snap to the nearest side and report it
                            drag = None;
                            match drag_at.take() {
                                Some(p) => {
                                    let d = dock_from_drop(client, p, size);
                                    if let Ok(mut g) = state.lock() {
                                        g.dock = d;
                                    }
                                    if let Ok(mut e) = events.lock() {
                                        e.push(d.encode());
                                    }
                                    dock_origin(client, outer, work, size, d).unwrap_or(docked)
                                }
                                None => docked,
                            }
                        }
                        None => docked,
                    };
                    if hits_gen != render_gen {
                        card_hits.clone_from(hits);
                        hits_gen = render_gen;
                    }
                    card_rect = Some((at, size));
                    card.show(screen_dc, img, at, hash_of(&(render_gen, at)));
                }
                _ => card.hide(),
            }
            // a tooltip while the pointer rests on a row that has one
            let hovered = card_rect.filter(|_| drag.is_none()).and_then(|(at, size)| {
                let (cx, cy) = cursor();
                let inside = cx >= at.0 && cx < at.0 + size.0 && cy >= at.1 && cy < at.1 + size.1;
                inside.then(|| card_hits.iter().find(|h| h.contains(cx - at.0, cy - at.1)).and_then(|h| h.tip.clone())).flatten().map(|t| (t, (cx, cy)))
            });
            match hovered {
                Some((text, (cx, cy))) => {
                    let scale = panel_width(cw) as f32 / PANEL_BASE_W;
                    let (img, tip_gen) = tip_memo.get((text.clone(), panel_width(cw)), || render_tooltip(scale, &text, &fonts));
                    let at = ((cx + 14).min(work.2 - img.width() as i32).max(work.0), (cy + 22).min(work.3 - img.height() as i32).max(work.1));
                    tip.show(screen_dc, img, at, hash_of(&(tip_gen, at)));
                }
                None => tip.hide(),
            }
            if game_present {
                // a card inside the game
                match ingame_track.current() {
                    Some(Some(p)) => {
                        let width = ingame_panel_width(cw) as u32;
                        let (img, render_gen) = ingame_memo.get((ingame_gen, width), || prof::time(prof::Part::RenderIngame, || render_panel(width, p, &fonts)));
                        let at = ingame_origin(client, (img.width() as i32, img.height() as i32));
                        ingame.show(screen_dc, img, at, hash_of(&(render_gen, at)));
                    }
                    _ => ingame.hide(),
                }
                // the icons over the portraits: the layer covers the game's client area
                match marks_track.current() {
                    Some(m) if !m.is_empty() => {
                        let (img, render_gen) = marks_memo.get((marks_gen, cw, ch), || prof::time(prof::Part::RenderMarks, || render_marks(cw as u32, ch as u32, m, &fonts)));
                        marks.show(screen_dc, img, (client.0, client.1), hash_of(&(render_gen, client.0, client.1)));
                    }
                    _ => marks.hide(),
                }
            } else {
                ingame.hide();
                marks.hide();
            }
            tick = tick.wrapping_add(1);
            prof::add(prof::Part::Busy, tick_start.elapsed());
            // a drag needs the card to follow the cursor smoothly
            wake.wait(Duration::from_millis(if drag.is_some() { 16 } else { 90 }));
        }
        drop(card);
        drop(ingame);
        drop(marks);
        drop(tip);
        ReleaseDC(0, screen_dc);
    }
}

impl Drop for Layer {
    fn drop(&mut self) {
        unsafe {
            DestroyWindow(self.hwnd);
        }
    }
}
