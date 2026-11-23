//! Scan progress banner drawn over the game window.
//!
//! The banner is a separate top-most, click-through, never-activating layered
//! window, so it cannot take focus or swallow the scanner's clicks. It is also
//! excluded from screen capture (when the OS supports it) so it never appears in
//! the frames being read. On non-Windows targets it is a no-op.
mod cache;
mod dock;
mod input_block;
mod marks;
mod panel;
mod render;
#[cfg(windows)]
mod tray;
mod scan;
#[cfg(windows)]
mod win;

pub use marks::{render_marks, Icon, Mark};
pub use dock::{dock_from_drop, dock_origin, Dock, Side};
pub use input_block::set_escape_target;
pub use panel::{ingame_origin, ingame_panel_width, panel_origin, panel_width, render_panel, render_panel_hits, render_tooltip, Chip, Hit, PanelState, Row, Tone, PANEL_BASE_W};

pub use render::{load_fonts, BannerState, Fonts, Ltrb, Phase};
pub use scan::{scan_card, scan_rows};

use std::sync::{Arc, Mutex};

/// The card window: scan progress, or the guide card beside the game window.
#[derive(Debug, Clone, PartialEq)]
pub enum Card {
    Panel(PanelState),
    Hidden,
}

/// Everything the overlay shows: the card and the icons over the game's own portraits
/// (two separate windows, so each updates on its own).
#[derive(Debug, Clone, PartialEq)]
pub struct Shown {
    pub card: Card,
    /// A card drawn inside the game (click-through), in addition to the one beside it.
    pub ingame: Option<PanelState>,
    pub marks: Vec<Mark>,
    /// Where the card sits against the game window (the player can drag it).
    pub dock: Dock,
}

/// Handle to the running banner. Dropping it closes the window.
pub struct Overlay {
    state: Arc<Mutex<Shown>>,
    /// Ids of the card's buttons that were clicked, oldest first.
    events: Arc<Mutex<Vec<String>>>,
    #[cfg(windows)]
    worker: Option<win::Worker>,
    /// The hooks that keep the player's hands off the game while a scan runs.
    #[cfg(windows)]
    block: Mutex<Option<input_block::InputBlock>>,
    /// The tray icon, with its own 종료: independent of every window above.
    #[cfg(windows)]
    _tray: Option<tray::Tray>,
}

impl Overlay {
    /// Show the banner over the game window ("Blue Archive"). Never fails the
    /// caller: if the banner cannot be shown, scanning just runs without it.
    pub fn start() -> Overlay {
        Overlay::start_with(Card::Hidden)
    }

    /// Start with nothing shown (the guide panel appears when `panel` is called).
    pub fn start_hidden() -> Overlay {
        Overlay::start_with(Card::Hidden)
    }

    fn start_with(first: Card) -> Overlay {
        let state = Arc::new(Mutex::new(Shown { card: first, ingame: None, marks: Vec::new(), dock: Dock::default() }));
        let events = Arc::new(Mutex::new(Vec::new()));
        #[cfg(windows)]
        let worker = win::Worker::start(state.clone(), events.clone());
        #[cfg(windows)]
        let tray = tray::Tray::start(events.clone());
        Overlay {
            state,
            events,
            #[cfg(windows)]
            worker,
            #[cfg(windows)]
            block: Mutex::new(None),
            #[cfg(windows)]
            _tray: tray,
        }
    }

    fn show(&self, c: Card) {
        if let Ok(mut g) = self.state.lock() {
            g.card = c;
        }
    }

    /// Block (or free) the player's mouse and keyboard; only the scan's own input gets through, and
    /// ESC raises the flag given to `set_escape_target`. Returns whether the block is in force.
    pub fn block_input(&self, on: bool) -> bool {
        #[cfg(windows)]
        {
            if let Ok(mut b) = self.block.lock() {
                if on && b.is_none() {
                    *b = input_block::InputBlock::start();
                } else if !on {
                    *b = None;
                }
                return b.is_some();
            }
        }
        let _ = on;
        false
    }

    /// Where the card sits against the game window.
    pub fn set_dock(&self, dock: Dock) {
        if let Ok(mut g) = self.state.lock() {
            g.dock = dock;
        }
    }

    /// A card inside the game's window (None removes it).
    pub fn ingame_card(&self, p: Option<PanelState>) {
        if let Ok(mut g) = self.state.lock() {
            g.ingame = p;
        }
    }

    /// The ids of the buttons clicked since the last call.
    pub fn take_events(&self) -> Vec<String> {
        self.events.lock().map(|mut e| std::mem::take(&mut *e)).unwrap_or_default()
    }

    /// Icons over the game's portraits (positions in the 1280x960 frame); empty clears them.
    pub fn marks(&self, marks: Vec<Mark>) {
        if let Ok(mut g) = self.state.lock() {
            g.marks = marks;
        }
    }

    /// Replace the shown banner state.
    pub fn set(&self, s: BannerState) {
        self.show(Card::Panel(scan_card(&s, false)));
    }

    /// Show the guide card beside the game window.
    pub fn panel(&self, p: PanelState) {
        self.show(Card::Panel(p));
    }

    pub fn hide(&self) {
        self.show(Card::Hidden);
    }

    pub fn counting(&self) {
        self.counting_n(0);
    }

    /// Counting the roster; `n` students found so far.
    pub fn counting_n(&self, n: usize) {
        self.set(BannerState::counting(n));
    }

    /// `total` is None while it is not known.
    pub fn scanning(&self, done: usize, total: Option<usize>) {
        self.set(BannerState::scanning(done, total));
    }

    pub fn finished(&self, done: usize, total: Option<usize>) {
        self.set(BannerState::finished(done, total));
    }

    pub fn failed(&self, done: usize, total: Option<usize>, note: &str) {
        self.set(BannerState::failed(done, total, note));
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
