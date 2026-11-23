//! Scan progress banner drawn over the game window.
//!
//! The banner is a separate top-most, click-through, never-activating layered
//! window, so it cannot take focus or swallow the scanner's clicks. It is also
//! excluded from screen capture (when the OS supports it) so it never appears in
//! the frames being read. On non-Windows targets it is a no-op.
mod panel;
mod render;
#[cfg(windows)]
mod win;

pub use panel::{panel_origin, panel_width, render_panel, Chip, PanelState, Row, Tone};

pub use render::{banner_layout, banner_origin, Ltrb, load_fonts, render, BannerState, Fonts, Phase, BASE_H, BASE_W};

use std::sync::{Arc, Mutex};

/// What the overlay window shows.
#[derive(Debug, Clone, PartialEq)]
pub enum Shown {
    /// Scan progress, over/above the game window.
    Banner(BannerState),
    /// Guide card beside the game window.
    Panel(PanelState),
    Hidden,
}

/// Handle to the running banner. Dropping it closes the window.
pub struct Overlay {
    state: Arc<Mutex<Shown>>,
    #[cfg(windows)]
    worker: Option<win::Worker>,
}

impl Overlay {
    /// Show the banner over the game window ("Blue Archive"). Never fails the
    /// caller: if the banner cannot be shown, scanning just runs without it.
    pub fn start() -> Overlay {
        Overlay::start_with(Shown::Banner(BannerState::default()))
    }

    /// Start with nothing shown (the guide panel appears when `panel` is called).
    pub fn start_hidden() -> Overlay {
        Overlay::start_with(Shown::Hidden)
    }

    fn start_with(first: Shown) -> Overlay {
        let state = Arc::new(Mutex::new(first));
        #[cfg(windows)]
        let worker = win::Worker::start(state.clone());
        Overlay {
            state,
            #[cfg(windows)]
            worker,
        }
    }

    fn show(&self, s: Shown) {
        if let Ok(mut g) = self.state.lock() {
            *g = s;
        }
    }

    /// Replace the shown banner state.
    pub fn set(&self, s: BannerState) {
        self.show(Shown::Banner(s));
    }

    /// Show the guide card beside the game window.
    pub fn panel(&self, p: PanelState) {
        self.show(Shown::Panel(p));
    }

    pub fn hide(&self) {
        self.show(Shown::Hidden);
    }

    pub fn counting(&self) {
        self.counting_n(0);
    }

    /// Counting the roster; `n` students found so far.
    pub fn counting_n(&self, n: usize) {
        self.set(BannerState { phase: Phase::Counting, done: n, total: None, note: String::new() });
    }

    /// `total` is None while it is not known.
    pub fn scanning(&self, done: usize, total: Option<usize>) {
        self.set(BannerState { phase: Phase::Scanning, done, total, note: String::new() });
    }

    pub fn finished(&self, done: usize, total: Option<usize>) {
        self.set(BannerState { phase: Phase::Done, done, total, note: String::new() });
    }

    pub fn failed(&self, done: usize, total: Option<usize>, note: &str) {
        self.set(BannerState { phase: Phase::Failed, done, total, note: note.to_string() });
    }

    /// Keep the final message visible for `secs`, then close.
    pub fn close_after(self, secs: u64) {
        std::thread::sleep(std::time::Duration::from_secs(secs));
        drop(self);
    }
}

impl Drop for Overlay {
    fn drop(&mut self) {
        #[cfg(windows)]
        if let Some(w) = self.worker.take() {
            w.stop();
        }
    }
}
