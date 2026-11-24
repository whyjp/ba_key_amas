//! Reproduces (on purpose) the old `activate()` focus trick and measures whether it leaves Alt
//! logically down in the input queue it attached to. It only touches the current foreground
//! window's queue and its own test window; any stuck Alt it finds is released at once.
//!
//!     cargo run --release -p baocr-hcu --example focus_repro [iterations]
use std::time::Duration;

#[link(name = "user32")]
extern "system" {
    fn CreateWindowExW(ex: u32, class: *const u16, title: *const u16, style: u32, x: i32, y: i32, w: i32, h: i32, parent: isize, menu: isize, inst: isize, param: *const u8) -> isize;
    fn PeekMessageW(msg: *mut [u8; 48], hwnd: isize, min: u32, max: u32, remove: u32) -> i32;
    fn DispatchMessageW(msg: *const [u8; 48]) -> isize;
    fn GetForegroundWindow() -> isize;
    fn SetForegroundWindow(h: isize) -> i32;
    fn GetWindowThreadProcessId(h: isize, pid: *mut u32) -> u32;
    fn AttachThreadInput(from: u32, to: u32, attach: i32) -> i32;
    fn keybd_event(vk: u8, scan: u8, flags: u32, extra: usize);
    fn GetKeyState(vk: i32) -> i16;
    fn GetAsyncKeyState(vk: i32) -> i16;
    fn DestroyWindow(h: isize) -> i32;
}
#[link(name = "kernel32")]
extern "system" {
    fn GetCurrentThreadId() -> u32;
}

const VK_MENU: u8 = 0x12;
const UP: u32 = 2;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn alt_down_in_queue_of(thread: u32, me: u32) -> (bool, bool) {
    unsafe {
        AttachThreadInput(me, thread, 1);
        std::thread::sleep(Duration::from_millis(15));
        let queue = GetKeyState(VK_MENU as i32) as u16 & 0x8000 != 0;
        let asynch = GetAsyncKeyState(VK_MENU as i32) as u16 & 0x8000 != 0;
        if queue {
            keybd_event(VK_MENU, 0, UP, 0);
            std::thread::sleep(Duration::from_millis(15));
        }
        AttachThreadInput(me, thread, 0);
        (queue, asynch)
    }
}

fn main() {
    let n: usize = std::env::args().nth(1).and_then(|a| a.parse().ok()).unwrap_or(20);
    let (tx, rx) = std::sync::mpsc::channel();
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = stop.clone();
    let pump = std::thread::spawn(move || unsafe {
        let hwnd = CreateWindowExW(0, wide("STATIC").as_ptr(), wide("focus_repro test window").as_ptr(), 0x1000_0000 | 0x8000_0000, 100, 100, 320, 120, 0, 0, 0, std::ptr::null());
        tx.send((hwnd, GetCurrentThreadId())).unwrap();
        let mut msg = [0u8; 48];
        while !flag.load(std::sync::atomic::Ordering::Relaxed) {
            while PeekMessageW(&mut msg, 0, 0, 0, 1) != 0 {
                DispatchMessageW(&msg);
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        DestroyWindow(hwnd);
    });
    let (hwnd, own_thread) = rx.recv().unwrap();
    let me = unsafe { GetCurrentThreadId() };
    unsafe {
        SetForegroundWindow(hwnd);
    }
    std::thread::sleep(Duration::from_millis(300));
    let fg = unsafe { GetForegroundWindow() };
    let target = unsafe { GetWindowThreadProcessId(fg, std::ptr::null_mut()) };
    println!("foreground queue under test: {} ({})", target, if target == own_thread { "our own test window" } else { "ANOTHER program's window" });

    for (name, detach_delay_ms) in [("old trick, detach at once", 0u64), ("old trick, detach after 40 ms", 40)] {
        let (mut stuck_queue, mut stuck_async) = (0, 0);
        for _ in 0..n {
            unsafe {
                AttachThreadInput(me, target, 1);
                keybd_event(VK_MENU, 0, 0, 0);
                keybd_event(VK_MENU, 0, UP, 0);
                SetForegroundWindow(hwnd);
                if detach_delay_ms > 0 {
                    std::thread::sleep(Duration::from_millis(detach_delay_ms));
                }
                AttachThreadInput(me, target, 0);
            }
            std::thread::sleep(Duration::from_millis(40));
            let (q, a) = alt_down_in_queue_of(target, me);
            stuck_queue += q as usize;
            stuck_async += a as usize;
        }
        println!("{name}: Alt stuck in the attached queue {stuck_queue}/{n}, system-wide async state {stuck_async}/{n}");
    }
    let mut plain = 0;
    for _ in 0..n {
        unsafe {
            SetForegroundWindow(hwnd);
        }
        std::thread::sleep(Duration::from_millis(40));
        plain += alt_down_in_queue_of(target, me).0 as usize;
    }
    println!("plain SetForegroundWindow only (the new activate): Alt stuck {plain}/{n}");
    unsafe {
        keybd_event(VK_MENU, 0, UP, 0);
    }
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    pump.join().ok();
}
