//! Blocking the player's own mouse and keyboard while the scan drives the game.
//!
//! The scan clicks through HCU, which injects its input; the player's hands must stay off.
//! Two low-level hooks see every input event first. Events marked as injected pass (they are
//! the scan's own); the rest are swallowed, except that ESC is the way out: it is swallowed too
//! and raises a flag that stops the scan and gives the control back. The hooks run on their
//! own thread with a tight message loop, so the scan's input is never held up.
//!
//! Safety: the block is only on while a scan runs, it ends when the scan ends, ESC ends the
//! scan, and it lifts by itself after `MAX_BLOCK` whatever happens.
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// The longest the block may last (a full detail scan takes about twenty minutes).
const MAX_BLOCK: Duration = Duration::from_secs(40 * 60);
const VK_ESCAPE: u32 = 0x1B;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Verdict {
    /// Let the event through.
    Pass,
    /// Swallow it: the game never sees it.
    Swallow,
    /// Swallow it and signal that the player pressed ESC.
    Escape,
}

/// What to do with a keyboard event. `injected`: it was sent by a program, not typed.
pub(crate) fn key_verdict(blocking: bool, injected: bool, vk: u32, down: bool) -> Verdict {
    if !blocking || injected {
        return Verdict::Pass;
    }
    if vk == VK_ESCAPE && down {
        return Verdict::Escape;
    }
    Verdict::Swallow
}

/// What to do with a mouse event (movement, buttons, wheel).
pub(crate) fn mouse_verdict(blocking: bool, injected: bool) -> Verdict {
    if blocking && !injected {
        Verdict::Swallow
    } else {
        Verdict::Pass
    }
}

static BLOCKING: AtomicBool = AtomicBool::new(false);
static STARTED: Mutex<Option<Instant>> = Mutex::new(None);
static ESCAPE_TARGET: Mutex<Option<Arc<AtomicBool>>> = Mutex::new(None);

/// Is the block in force right now (on, and not past its time limit)?
fn blocking_now() -> bool {
    BLOCKING.load(Ordering::Relaxed) && STARTED.lock().ok().and_then(|s| *s).is_some_and(|t| t.elapsed() < MAX_BLOCK)
}

/// The flag that is raised when the player presses ESC during the block.
pub fn set_escape_target(flag: Arc<AtomicBool>) {
    if let Ok(mut t) = ESCAPE_TARGET.lock() {
        *t = Some(flag);
    }
}

pub(crate) fn signal_escape() {
    if let Ok(t) = ESCAPE_TARGET.lock() {
        if let Some(flag) = t.as_ref() {
            flag.store(true, Ordering::SeqCst);
        }
    }
}

#[cfg(windows)]
pub use imp::InputBlock;

#[cfg(windows)]
mod imp {
    use super::*;

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
    struct KbdHook {
        vk: u32,
        scan: u32,
        flags: u32,
        time: u32,
        extra: usize,
    }
    #[repr(C)]
    struct MouseHook {
        pt: Point,
        data: u32,
        flags: u32,
        time: u32,
        extra: usize,
    }

    type HookProc = unsafe extern "system" fn(i32, usize, isize) -> isize;

    #[link(name = "user32")]
    extern "system" {
        fn SetWindowsHookExW(id: i32, proc_: HookProc, module: isize, thread: u32) -> isize;
        fn UnhookWindowsHookEx(hook: isize) -> i32;
        fn CallNextHookEx(hook: isize, code: i32, w: usize, l: isize) -> isize;
        fn GetMessageW(msg: *mut Msg, hwnd: isize, min: u32, max: u32) -> i32;
        fn PostThreadMessageW(thread: u32, msg: u32, w: usize, l: isize) -> i32;
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentThreadId() -> u32;
        fn GetModuleHandleW(name: *const u16) -> isize;
    }

    const WH_KEYBOARD_LL: i32 = 13;
    const WH_MOUSE_LL: i32 = 14;
    const WM_KEYDOWN: usize = 0x0100;
    const WM_SYSKEYDOWN: usize = 0x0104;
    const WM_QUIT: u32 = 0x0012;
    const LLKHF_INJECTED: u32 = 0x10;
    const LLMHF_INJECTED: u32 = 0x01;

    unsafe extern "system" fn keyboard_proc(code: i32, w: usize, l: isize) -> isize {
        if code >= 0 && l != 0 {
            let k = &*(l as *const KbdHook);
            match key_verdict(blocking_now(), k.flags & LLKHF_INJECTED != 0, k.vk, w == WM_KEYDOWN || w == WM_SYSKEYDOWN) {
                Verdict::Pass => {}
                Verdict::Swallow => return 1,
                Verdict::Escape => {
                    signal_escape();
                    return 1;
                }
            }
        }
        CallNextHookEx(0, code, w, l)
    }

    unsafe extern "system" fn mouse_proc(code: i32, w: usize, l: isize) -> isize {
        if code >= 0 && l != 0 {
            let m = &*(l as *const MouseHook);
            if mouse_verdict(blocking_now(), m.flags & LLMHF_INJECTED != 0) == Verdict::Swallow {
                return 1;
            }
        }
        CallNextHookEx(0, code, w, l)
    }

    /// The running block. Dropping it (or `stop`) lifts it.
    pub struct InputBlock {
        thread: u32,
        handle: Option<std::thread::JoinHandle<()>>,
    }

    impl InputBlock {
        /// Start blocking. None when the hooks could not be installed (nothing is blocked then).
        pub fn start() -> Option<InputBlock> {
            if let Ok(mut s) = STARTED.lock() {
                *s = Some(Instant::now());
            }
            BLOCKING.store(true, Ordering::SeqCst);
            let (tx, rx) = std::sync::mpsc::channel();
            let handle = std::thread::spawn(move || unsafe {
                let module = GetModuleHandleW(std::ptr::null::<u16>());
                let kb = SetWindowsHookExW(WH_KEYBOARD_LL, keyboard_proc, module, 0);
                let ms = SetWindowsHookExW(WH_MOUSE_LL, mouse_proc, module, 0);
                let ok = kb != 0 && ms != 0;
                let _ = tx.send((GetCurrentThreadId(), ok));
                if ok {
                    let mut msg: Msg = std::mem::zeroed();
                    while GetMessageW(&mut msg, 0, 0, 0) > 0 {}
                }
                if kb != 0 {
                    UnhookWindowsHookEx(kb);
                }
                if ms != 0 {
                    UnhookWindowsHookEx(ms);
                }
            });
            match rx.recv_timeout(Duration::from_secs(2)) {
                Ok((thread, true)) => Some(InputBlock { thread, handle: Some(handle) }),
                _ => {
                    BLOCKING.store(false, Ordering::SeqCst);
                    None
                }
            }
        }
    }

    impl Drop for InputBlock {
        fn drop(&mut self) {
            BLOCKING.store(false, Ordering::SeqCst);
            unsafe {
                PostThreadMessageW(self.thread, WM_QUIT, 0, 0);
            }
            if let Some(h) = self.handle.take() {
                let _ = h.join();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_players_own_input_is_blocked_and_esc_is_the_way_out() {
        // nothing is blocked while the block is off
        assert_eq!(key_verdict(false, false, 0x41, true), Verdict::Pass);
        assert_eq!(mouse_verdict(false, false), Verdict::Pass);
        // the scan's own (injected) input always passes
        assert_eq!(key_verdict(true, true, 0x41, true), Verdict::Pass);
        assert_eq!(key_verdict(true, true, VK_ESCAPE, true), Verdict::Pass, "an injected ESC is not the player's");
        assert_eq!(mouse_verdict(true, true), Verdict::Pass);
        // the player's keys and mouse are swallowed
        assert_eq!(key_verdict(true, false, 0x41, true), Verdict::Swallow);
        assert_eq!(key_verdict(true, false, 0x41, false), Verdict::Swallow);
        assert_eq!(mouse_verdict(true, false), Verdict::Swallow);
        // ESC pressed by the player raises the stop signal; its release is only swallowed
        assert_eq!(key_verdict(true, false, VK_ESCAPE, true), Verdict::Escape);
        assert_eq!(key_verdict(true, false, VK_ESCAPE, false), Verdict::Swallow);
    }

    #[test]
    fn the_block_is_off_until_started_and_the_escape_signal_reaches_its_flag() {
        assert!(!blocking_now(), "no block unless a scan started one");
        let flag = Arc::new(AtomicBool::new(false));
        set_escape_target(flag.clone());
        signal_escape();
        assert!(flag.load(Ordering::SeqCst));
    }
}
