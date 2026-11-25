//! The tray icon: the add-on's way to be closed whatever state its windows are in.
//!
//! It lives on its own thread with its own hidden window, apart from the overlay's windows and
//! the main loop, so a drawing error or a stuck loop cannot take it away. Choosing 종료 raises
//! the stop flag (a running scan stops), reports `quit` through the overlay's events, and, if the
//! program has not ended by itself shortly after, ends it.
use crate::input_block::signal_escape;
use std::ffi::c_void;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Time the program is given to end by itself before the tray ends it.
const QUIT_GRACE: Duration = Duration::from_secs(4);
const WM_TRAY: u32 = 0x8001; // WM_APP + 1
const WM_RBUTTONUP: isize = 0x0205;
const WM_CLOSE: u32 = 0x0010;
const WM_NULL: u32 = 0x0000;
const NIM_ADD: u32 = 0;
const NIM_DELETE: u32 = 2;
const NIF_MESSAGE: u32 = 1;
const NIF_ICON: u32 = 2;
const NIF_TIP: u32 = 4;
const MF_STRING: u32 = 0;
const MF_GRAYED: u32 = 1;
const MF_SEPARATOR: u32 = 0x800;
const TPM_RIGHTBUTTON: u32 = 2;
const TPM_RETURNCMD: u32 = 0x100;
const ID_QUIT: usize = 2;

#[repr(C)]
struct Point {
    x: i32,
    y: i32,
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
struct NotifyIconData {
    cb_size: u32,
    hwnd: isize,
    id: u32,
    flags: u32,
    callback: u32,
    icon: isize,
    tip: [u16; 128],
    state: u32,
    state_mask: u32,
    info: [u16; 256],
    timeout_or_version: u32,
    info_title: [u16; 64],
    info_flags: u32,
    guid: [u8; 16],
    balloon_icon: isize,
}

#[link(name = "user32")]
extern "system" {
    fn RegisterClassExW(c: *const WndClassExW) -> u16;
    fn CreateWindowExW(ex: u32, class: *const u16, title: *const u16, style: u32, x: i32, y: i32, w: i32, h: i32, parent: isize, menu: isize, inst: isize, param: *const c_void) -> isize;
    fn DefWindowProcW(h: isize, m: u32, w: usize, l: isize) -> isize;
    fn DestroyWindow(h: isize) -> i32;
    fn GetMessageW(m: *mut Msg, h: isize, min: u32, max: u32) -> i32;
    fn TranslateMessage(m: *const Msg) -> i32;
    fn DispatchMessageW(m: *const Msg) -> isize;
    fn PostMessageW(h: isize, m: u32, w: usize, l: isize) -> i32;
    fn PostQuitMessage(code: i32);
    fn CreatePopupMenu() -> isize;
    fn AppendMenuW(menu: isize, flags: u32, id: usize, text: *const u16) -> i32;
    fn TrackPopupMenu(menu: isize, flags: u32, x: i32, y: i32, reserved: i32, hwnd: isize, rect: *const c_void) -> i32;
    fn DestroyMenu(menu: isize) -> i32;
    fn SetForegroundWindow(h: isize) -> i32;
    fn GetCursorPos(p: *mut Point) -> i32;
    fn CreateIcon(inst: isize, w: i32, h: i32, planes: u8, bits: u8, and_bits: *const u8, xor_bits: *const u8) -> isize;
}
#[link(name = "shell32")]
extern "system" {
    fn Shell_NotifyIconW(message: u32, data: *const NotifyIconData) -> i32;
}
#[link(name = "kernel32")]
extern "system" {
    fn GetModuleHandleW(name: *const u16) -> isize;
}

static EVENTS: Mutex<Option<Arc<Mutex<Vec<String>>>>> = Mutex::new(None);

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// The tray icon: a navy square with a white B (drawn here, no icon file needed).
unsafe fn make_icon(inst: isize) -> isize {
    use crate::render::{draw_text, fill_rrect, load_fonts, NAVY, WHITE};
    const N: u32 = 32;
    let mut img = image::RgbaImage::from_pixel(N, N, image::Rgba([0, 0, 0, 0]));
    fill_rrect(&mut img, 1.0, 1.0, 30.0, 30.0, 6.0, |_, _| (NAVY, 1.0));
    if let Some(f) = load_fonts() {
        draw_text(&mut img, &f.bold, 22.0, 9.0, 3.5, WHITE, 1.0, "B");
    }
    let mut xor = Vec::with_capacity((N * N * 4) as usize);
    for p in img.pixels() {
        xor.extend_from_slice(&[p[2], p[1], p[0], p[3]]); // BGRA
    }
    let and = vec![0u8; (N * N / 8) as usize];
    CreateIcon(inst, N as i32, N as i32, 1, 32, and.as_ptr(), xor.as_ptr())
}

/// Ask the program to end: stop a running scan, report it, and end the program if it does not.
fn quit() {
    signal_escape();
    if let Ok(g) = EVENTS.lock() {
        if let Some(events) = g.as_ref() {
            if let Ok(mut e) = events.lock() {
                e.push("quit".to_string());
            }
        }
    }
    std::thread::spawn(|| {
        std::thread::sleep(QUIT_GRACE);
        std::process::exit(0);
    });
}

unsafe extern "system" fn wnd_proc(h: isize, m: u32, w: usize, l: isize) -> isize {
    if m == WM_TRAY && l == WM_RBUTTONUP {
        let menu = CreatePopupMenu();
        AppendMenuW(menu, MF_STRING | MF_GRAYED, 1, wide("ba-scanner 애드온 · 실행 중").as_ptr());
        AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
        AppendMenuW(menu, MF_STRING, ID_QUIT, wide("종료").as_ptr());
        let mut p = Point { x: 0, y: 0 };
        GetCursorPos(&mut p);
        SetForegroundWindow(h); // without this the menu does not close when the player clicks elsewhere
        let chosen = TrackPopupMenu(menu, TPM_RIGHTBUTTON | TPM_RETURNCMD, p.x, p.y, 0, h, std::ptr::null());
        PostMessageW(h, WM_NULL, 0, 0);
        DestroyMenu(menu);
        if chosen as usize == ID_QUIT {
            quit();
        }
        return 0;
    }
    if m == WM_CLOSE {
        DestroyWindow(h);
        return 0;
    }
    if m == 0x0002 {
        PostQuitMessage(0); // WM_DESTROY
        return 0;
    }
    DefWindowProcW(h, m, w, l)
}

/// The tray icon, shown until this is dropped.
pub struct Tray {
    hwnd: isize,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl Tray {
    /// Add the icon. `events` receives "quit" when the player chooses 종료. None if the icon
    /// could not be created (the add-on still runs).
    pub fn start(events: Arc<Mutex<Vec<String>>>) -> Option<Tray> {
        if let Ok(mut g) = EVENTS.lock() {
            *g = Some(events);
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let handle = std::thread::spawn(move || unsafe {
            let inst = GetModuleHandleW(std::ptr::null());
            let class = wide("BaScannerTray");
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
            RegisterClassExW(&wc);
            let title = wide("ba-scanner tray");
            let hwnd = CreateWindowExW(0, class.as_ptr(), title.as_ptr(), 0, 0, 0, 0, 0, 0, 0, inst, std::ptr::null());
            if hwnd == 0 {
                let _ = tx.send(0);
                return;
            }
            let mut data: NotifyIconData = std::mem::zeroed();
            data.cb_size = std::mem::size_of::<NotifyIconData>() as u32;
            data.hwnd = hwnd;
            data.id = 1;
            data.flags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
            data.callback = WM_TRAY;
            data.icon = make_icon(inst);
            for (i, c) in "ba-scanner 애드온".encode_utf16().take(127).enumerate() {
                data.tip[i] = c;
            }
            let added = Shell_NotifyIconW(NIM_ADD, &data) != 0;
            let _ = tx.send(if added { hwnd } else { 0 });
            if added {
                let mut msg: Msg = std::mem::zeroed();
                while GetMessageW(&mut msg, 0, 0, 0) > 0 {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
                Shell_NotifyIconW(NIM_DELETE, &data);
            } else {
                DestroyWindow(hwnd);
            }
        });
        match rx.recv_timeout(Duration::from_secs(3)) {
            Ok(hwnd) if hwnd != 0 => Some(Tray { hwnd, handle: Some(handle) }),
            _ => None,
        }
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        unsafe {
            PostMessageW(self.hwnd, WM_CLOSE, 0, 0);
        }
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}
