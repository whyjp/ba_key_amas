//! Input hints: the player's own clicks and wheel turns inside the game window, used only as
//! a hint that the screen is about to change, so the add-on looks sooner instead of waiting for
//! movement plus two matching frames.
//!
//! Nothing here intercepts, delays or forwards input. The hook callback (Windows part) only
//! timestamps wheel and left-button events that fall inside the game's client rectangle and
//! drops everything else on the spot; mouse movement, keys and injected events are never looked
//! at. Nothing is logged or stored beyond the few events waiting for the next `drain`.
//!
//! `HintScheduler` is the pure part: events in, requests out, with the timing policy.
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Mutex;
use std::time::Duration;

/// Milliseconds on any monotonic clock the caller chooses (the hook uses time since its start).
pub type Ms = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HintKind {
    /// Left button released.
    Click,
    /// Left button pressed (a drag or a hold may follow).
    ButtonDown,
    /// Wheel turned.
    Wheel,
}

/// One input event inside the game's client area. `x`, `y` are screen pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HintEvent {
    pub at: Ms,
    pub kind: HintKind,
    pub x: i32,
    pub y: i32,
}

/// Why a look at the game is asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    /// A click: the game starts to react about now.
    Click,
    /// The wheel (or a drag) has been quiet long enough for the list to have stopped.
    ScrollStopped,
    /// A click on a known button: what is on the card is already wrong, take it down now.
    Preempt(usize),
}

/// What the loop should do at `due`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Request {
    pub due: Ms,
    pub reason: Reason,
    /// The view is known to be still at `due` (the scroll stopped, the click's animation is over),
    /// so one frame is enough: do not wait for a second matching one.
    pub trust_single_frame: bool,
}

/// A rectangle of the 1280x960 client layout that, when clicked, has a known consequence.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HotZone {
    pub rect: [i32; 4],
}

#[derive(Debug, Clone, Copy)]
pub struct Policy {
    /// Look this long after a click (the game has started to answer, an instant cut is already done).
    pub click_first_look: Duration,
    /// Look again this long after a click, for answers that animate; this one is trusted as still.
    pub click_trusted_look: Duration,
    /// A scroll has stopped when the wheel was quiet for this long.
    pub wheel_quiet: Duration,
    /// Clicks closer together than this are one.
    pub click_debounce: Duration,
}

impl Default for Policy {
    fn default() -> Self {
        // Guesses to be tuned on the live game (see docs/speed-research.md): none of them was measured with input.
        Policy { click_first_look: Duration::from_millis(80), click_trusted_look: Duration::from_millis(450), wheel_quiet: Duration::from_millis(160), click_debounce: Duration::from_millis(40) }
    }
}

pub struct HintScheduler {
    policy: Policy,
    /// the client rectangle in screen pixels and the zones in client (1280x960) pixels
    client: [i32; 4],
    zones: Vec<HotZone>,
    pending: Vec<Request>,
    last_wheel: Option<Ms>,
    last_click: Option<Ms>,
    held_since: Option<Ms>,
}

impl HintScheduler {
    pub fn new(policy: Policy) -> Self {
        HintScheduler { policy, client: [0, 0, 1280, 960], zones: Vec::new(), pending: Vec::new(), last_wheel: None, last_click: None, held_since: None }
    }

    pub fn set_client(&mut self, client: [i32; 4]) {
        self.client = client;
    }

    pub fn set_zones(&mut self, zones: Vec<HotZone>) {
        self.zones = zones;
    }

    fn zone_at(&self, x: i32, y: i32) -> Option<usize> {
        let [l, t, r, b] = self.client;
        let (w, h) = ((r - l).max(1) as f64, (b - t).max(1) as f64);
        let (cx, cy) = (((x - l) as f64 * 1280.0 / w) as i32, ((y - t) as f64 * 960.0 / h) as i32);
        self.zones.iter().position(|z| cx >= z.rect[0] && cx < z.rect[2] && cy >= z.rect[1] && cy < z.rect[3])
    }

    /// Take one event in. Events outside the client rectangle are ignored.
    pub fn feed(&mut self, e: HintEvent) {
        if !inside(self.client, e.x, e.y) {
            return;
        }
        let ms = |d: Duration| d.as_millis() as Ms;
        match e.kind {
            HintKind::Wheel => {
                self.last_wheel = Some(e.at);
                // one scroll-stopped request per burst: the previous one moves with the wheel
                self.pending.retain(|r| r.reason != Reason::ScrollStopped);
                self.pending.push(Request { due: e.at + ms(self.policy.wheel_quiet), reason: Reason::ScrollStopped, trust_single_frame: true });
            }
            HintKind::ButtonDown => self.held_since = Some(e.at),
            HintKind::Click => {
                // a drag (button held a while) is a scroll in the grid: it stops when released
                let dragged = self.held_since.take().is_some_and(|d| e.at.saturating_sub(d) > 250);
                if dragged {
                    self.pending.retain(|r| r.reason != Reason::ScrollStopped);
                    self.pending.push(Request { due: e.at + ms(self.policy.wheel_quiet), reason: Reason::ScrollStopped, trust_single_frame: true });
                    return;
                }
                if self.last_click.is_some_and(|c| e.at.saturating_sub(c) < ms(self.policy.click_debounce)) {
                    return;
                }
                self.last_click = Some(e.at);
                if let Some(z) = self.zone_at(e.x, e.y) {
                    self.pending.push(Request { due: e.at, reason: Reason::Preempt(z), trust_single_frame: false });
                }
                self.pending.push(Request { due: e.at + ms(self.policy.click_first_look), reason: Reason::Click, trust_single_frame: false });
                self.pending.push(Request { due: e.at + ms(self.policy.click_trusted_look), reason: Reason::Click, trust_single_frame: true });
            }
        }
    }

    /// Requests that are due at `now`, oldest first. They are removed.
    pub fn due(&mut self, now: Ms) -> Vec<Request> {
        let mut out: Vec<Request> = self.pending.iter().copied().filter(|r| r.due <= now).collect();
        self.pending.retain(|r| r.due > now);
        out.sort_by_key(|r| r.due);
        out
    }

    /// When the next request falls due (so the loop can cap its wait).
    pub fn next_due(&self) -> Option<Ms> {
        self.pending.iter().map(|r| r.due).min()
    }

    /// The wheel was turned within the quiet time: the grid is probably moving.
    pub fn scrolling(&self, now: Ms) -> bool {
        self.last_wheel.is_some_and(|w| now.saturating_sub(w) < self.policy.wheel_quiet.as_millis() as Ms) || self.held_since.is_some()
    }
}

/// Is the screen point inside `rect` (left, top, right, bottom)?
pub fn inside(rect: [i32; 4], x: i32, y: i32) -> bool {
    x >= rect[0] && x < rect[2] && y >= rect[1] && y < rect[3]
}

// The client rectangle the hook callback tests against, as plain atomics: the callback takes no lock.
static CLIENT: [AtomicI32; 4] = [AtomicI32::new(0), AtomicI32::new(0), AtomicI32::new(0), AtomicI32::new(0)];
static SINK: Mutex<Option<std::sync::mpsc::SyncSender<HintEvent>>> = Mutex::new(None);

fn client_now() -> [i32; 4] {
    [CLIENT[0].load(Ordering::Relaxed), CLIENT[1].load(Ordering::Relaxed), CLIENT[2].load(Ordering::Relaxed), CLIENT[3].load(Ordering::Relaxed)]
}

/// What the hook callback does with one event: drop it unless it is a wheel turn or a left button
/// change, not injected, and inside the client area.
fn enqueue(at: Ms, kind: HintKind, x: i32, y: i32) {
    if !inside(client_now(), x, y) {
        return;
    }
    if let Ok(sink) = SINK.try_lock() {
        if let Some(tx) = sink.as_ref() {
            let _ = tx.try_send(HintEvent { at, kind, x, y }); // a full queue drops the hint, never waits
        }
    }
}

#[cfg(windows)]
pub use imp::InputHints;

#[cfg(windows)]
mod imp {
    use super::*;
    use std::sync::mpsc::Receiver;
    use std::time::Instant;

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

    const WH_MOUSE_LL: i32 = 14;
    const WM_LBUTTONDOWN: usize = 0x0201;
    const WM_LBUTTONUP: usize = 0x0202;
    const WM_MOUSEWHEEL: usize = 0x020A;
    const WM_QUIT: u32 = 0x0012;
    const LLMHF_INJECTED: u32 = 0x01;

    static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();

    /// Only timestamps and enqueues; always hands the event on untouched.
    unsafe extern "system" fn mouse_proc(code: i32, w: usize, l: isize) -> isize {
        if code >= 0 && l != 0 && matches!(w, WM_LBUTTONDOWN | WM_LBUTTONUP | WM_MOUSEWHEEL) {
            let m = &*(l as *const MouseHook);
            if m.flags & LLMHF_INJECTED == 0 {
                let kind = match w {
                    WM_LBUTTONDOWN => HintKind::ButtonDown,
                    WM_LBUTTONUP => HintKind::Click,
                    _ => HintKind::Wheel,
                };
                let at = START.get().map_or(0, |s| s.elapsed().as_millis() as Ms);
                enqueue(at, kind, m.pt.x, m.pt.y);
            }
        }
        CallNextHookEx(0, code, w, l)
    }

    /// The passive hook. Dropping it removes the hook. Experimental and off unless started.
    pub struct InputHints {
        thread: u32,
        handle: Option<std::thread::JoinHandle<()>>,
        rx: Receiver<HintEvent>,
        start: Instant,
    }

    impl InputHints {
        /// Install the hook on its own thread. None when Windows refuses (nothing changes then).
        pub fn start() -> Option<InputHints> {
            let start = *START.get_or_init(Instant::now);
            let (tx, rx) = std::sync::mpsc::sync_channel(256);
            if let Ok(mut s) = SINK.lock() {
                *s = Some(tx);
            }
            let (ready_tx, ready_rx) = std::sync::mpsc::channel();
            let handle = std::thread::spawn(move || unsafe {
                let module = GetModuleHandleW(std::ptr::null::<u16>());
                let hook = SetWindowsHookExW(WH_MOUSE_LL, mouse_proc, module, 0);
                let _ = ready_tx.send((GetCurrentThreadId(), hook != 0));
                if hook != 0 {
                    let mut msg: Msg = std::mem::zeroed();
                    while GetMessageW(&mut msg, 0, 0, 0) > 0 {}
                    UnhookWindowsHookEx(hook);
                }
            });
            match ready_rx.recv_timeout(Duration::from_secs(2)) {
                Ok((thread, true)) => Some(InputHints { thread, handle: Some(handle), rx, start }),
                _ => {
                    if let Ok(mut s) = SINK.lock() {
                        *s = None;
                    }
                    None
                }
            }
        }

        /// The game's client rectangle in screen pixels; events outside it are dropped by the hook itself.
        pub fn set_client(&self, rect: [i32; 4]) {
            for (slot, v) in CLIENT.iter().zip(rect) {
                slot.store(v, Ordering::Relaxed);
            }
        }

        /// Milliseconds on the clock the events are stamped with.
        pub fn now(&self) -> Ms {
            self.start.elapsed().as_millis() as Ms
        }

        /// The events since the last call.
        pub fn drain(&self) -> Vec<HintEvent> {
            self.rx.try_iter().collect()
        }
    }

    impl Drop for InputHints {
        fn drop(&mut self) {
            if let Ok(mut s) = SINK.lock() {
                *s = None;
            }
            for slot in &CLIENT {
                slot.store(0, Ordering::Relaxed);
            }
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

    fn ev(at: Ms, kind: HintKind, x: i32, y: i32) -> HintEvent {
        HintEvent { at, kind, x, y }
    }

    fn sched() -> HintScheduler {
        let mut s = HintScheduler::new(Policy::default());
        s.set_client([100, 50, 1380, 1010]); // a 1280x960 client at (100, 50)
        s
    }

    #[test]
    fn a_click_asks_for_a_quick_look_and_a_trusted_one_later() {
        let mut s = sched();
        s.feed(ev(1000, HintKind::Click, 500, 500));
        assert_eq!(s.next_due(), Some(1080));
        assert!(s.due(1079).is_empty());
        let r = s.due(1080);
        assert_eq!(r, vec![Request { due: 1080, reason: Reason::Click, trust_single_frame: false }]);
        let r = s.due(2000);
        assert_eq!(r, vec![Request { due: 1450, reason: Reason::Click, trust_single_frame: true }]);
        assert_eq!(s.next_due(), None);
    }

    #[test]
    fn events_outside_the_game_are_ignored() {
        let mut s = sched();
        s.feed(ev(0, HintKind::Click, 50, 500));
        s.feed(ev(0, HintKind::Wheel, 1500, 500));
        s.feed(ev(0, HintKind::Click, 1380, 1010)); // the right and bottom edges are outside
        assert_eq!(s.next_due(), None);
        assert!(!s.scrolling(1));
    }

    #[test]
    fn a_burst_of_wheel_turns_is_one_scroll_that_stops_when_the_wheel_is_quiet() {
        let mut s = sched();
        for i in 0..10 {
            s.feed(ev(1000 + i * 30, HintKind::Wheel, 700, 500));
        }
        // the last turn at 1270: the request moves with it, only one is pending
        assert_eq!(s.next_due(), Some(1270 + 160));
        assert!(s.scrolling(1300));
        assert!(s.due(1429).is_empty());
        assert_eq!(s.due(1430), vec![Request { due: 1430, reason: Reason::ScrollStopped, trust_single_frame: true }]);
        assert!(!s.scrolling(1500));
    }

    #[test]
    fn clicks_closer_than_the_debounce_are_one() {
        let mut s = sched();
        s.feed(ev(1000, HintKind::Click, 500, 500));
        s.feed(ev(1020, HintKind::Click, 500, 500));
        assert_eq!(s.due(10_000).len(), 2, "one click: a quick look and a trusted one");
        s.feed(ev(2000, HintKind::Click, 500, 500));
        s.feed(ev(2100, HintKind::Click, 500, 500));
        assert_eq!(s.due(10_000).len(), 4);
    }

    #[test]
    fn a_click_on_a_known_button_takes_the_old_view_down_at_once() {
        let mut s = sched();
        s.set_zones(vec![HotZone { rect: [1000, 200, 1100, 260] }]);
        // client (1050, 230) is screen (1150, 280)
        s.feed(ev(500, HintKind::Click, 1150, 280));
        let first = s.due(500);
        assert_eq!(first, vec![Request { due: 500, reason: Reason::Preempt(0), trust_single_frame: false }]);
        s.feed(ev(900, HintKind::Click, 600, 600));
        assert!(s.due(900).iter().all(|r| !matches!(r.reason, Reason::Preempt(_))), "a click elsewhere preempts nothing");
    }

    #[test]
    fn zones_follow_the_client_when_the_window_is_scaled() {
        let mut s = HintScheduler::new(Policy::default());
        s.set_client([0, 0, 640, 480]); // half size
        s.set_zones(vec![HotZone { rect: [1000, 200, 1100, 260] }]);
        s.feed(ev(0, HintKind::Click, 525, 115)); // client (1050, 230)
        assert!(s.due(0).iter().any(|r| r.reason == Reason::Preempt(0)));
    }

    #[test]
    fn a_held_button_is_a_drag_and_its_release_ends_a_scroll_not_starts_a_click_look() {
        let mut s = sched();
        s.feed(ev(1000, HintKind::ButtonDown, 700, 500));
        assert!(s.scrolling(1100));
        s.feed(ev(1800, HintKind::Click, 700, 400));
        assert!(!s.scrolling(1801));
        assert_eq!(s.due(10_000), vec![Request { due: 1960, reason: Reason::ScrollStopped, trust_single_frame: true }]);
        // a short press is a plain click
        s.feed(ev(3000, HintKind::ButtonDown, 700, 500));
        s.feed(ev(3060, HintKind::Click, 700, 500));
        assert_eq!(s.due(10_000).len(), 2);
    }

    #[test]
    fn the_callback_side_drops_what_is_outside_before_anything_is_queued() {
        let (tx, rx) = std::sync::mpsc::sync_channel(4);
        *SINK.lock().unwrap() = Some(tx);
        for (slot, v) in CLIENT.iter().zip([0, 0, 100, 100]) {
            slot.store(v, Ordering::Relaxed);
        }
        enqueue(1, HintKind::Click, 500, 500);
        enqueue(2, HintKind::Click, 50, 50);
        for i in 0..10 {
            enqueue(3 + i, HintKind::Wheel, 10, 10); // more than the queue holds: the rest are dropped, nothing blocks
        }
        let got: Vec<HintEvent> = rx.try_iter().collect();
        assert_eq!(got.len(), 4);
        assert_eq!(got[0].at, 2);
        *SINK.lock().unwrap() = None;
        for slot in &CLIENT {
            slot.store(0, Ordering::Relaxed);
        }
    }
}
