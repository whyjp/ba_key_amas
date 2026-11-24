//! The bookmark's open/closed behaviour over time, as a state machine on a clock the caller
//! supplies (milliseconds), so it is tested with a fake one. The card opens once the pointer has
//! rested on the tab or the card for a moment, stays open while the pointer is on it, closes a
//! little after the pointer leaves (so it can travel), and slides in between.

/// How long the pointer must rest on a folded tab before it opens (a pass-by does not open it).
pub const OPEN_DWELL_MS: u64 = 100;
/// How long the card stays open after the pointer left it.
pub const CLOSE_DELAY_MS: u64 = 350;
/// The slide, folded to open or back.
pub const SLIDE_MS: u64 = 120;

/// What is true of the pointer and the card right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Intent {
    /// The pointer is not on the tab or the card.
    Away,
    /// The pointer is on the tab or the card.
    Over,
    /// Keep it open whatever the pointer does (a drag of the open card).
    Hold,
    /// Fold it now and keep it folded whatever the pointer does (a drag of the tab, a scan).
    Shut,
}

#[derive(Debug, Clone, Default)]
pub struct Fold {
    want_open: bool,
    /// Linear slide position, 0 folded .. 1 open.
    pos: f32,
    last_ms: Option<u64>,
    over_since: Option<u64>,
    away_since: Option<u64>,
}

fn ease_out(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}

impl Fold {
    /// Advance to `now_ms` with the pointer in the state `intent`. Returns the eased slide
    /// progress: 0 folded, 1 open.
    pub fn step(&mut self, now_ms: u64, intent: Intent) -> f32 {
        let dt = self.last_ms.map_or(0, |l| now_ms.saturating_sub(l));
        self.last_ms = Some(now_ms);
        match intent {
            Intent::Hold => {
                self.want_open = true;
                self.over_since = None;
                self.away_since = None;
            }
            Intent::Shut => {
                self.want_open = false;
                self.over_since = None;
                self.away_since = None;
            }
            Intent::Over => {
                self.away_since = None;
                if !self.want_open {
                    // already sliding out: coming back catches it at once
                    let since = *self.over_since.get_or_insert(now_ms);
                    if self.pos > 0.0 || now_ms.saturating_sub(since) >= OPEN_DWELL_MS {
                        self.want_open = true;
                        self.over_since = None;
                    }
                }
            }
            Intent::Away => {
                self.over_since = None;
                if self.want_open {
                    let since = *self.away_since.get_or_insert(now_ms);
                    if now_ms.saturating_sub(since) >= CLOSE_DELAY_MS {
                        self.want_open = false;
                        self.away_since = None;
                    }
                }
            }
        }
        let step = dt as f32 / SLIDE_MS as f32;
        self.pos = if self.want_open { (self.pos + step).min(1.0) } else { (self.pos - step).max(0.0) };
        ease_out(self.pos)
    }

    /// Open at once, no slide (a pinned card; the card's state after being covered and shown again is `fold`).
    pub fn set_open(&mut self) {
        *self = Fold { want_open: true, pos: 1.0, ..Fold::default() };
    }

    /// Folded at once, no slide (the window was covered or hidden: nothing is animated unseen).
    pub fn reset(&mut self) {
        *self = Fold::default();
    }

    /// True while a slide is in progress (the loop then ticks faster).
    pub fn sliding(&self) -> bool {
        (self.want_open && self.pos < 1.0) || (!self.want_open && self.pos > 0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Feed `intent` every 10 ms from `from` to `to`; returns the last progress.
    fn run(f: &mut Fold, from: u64, to: u64, intent: Intent) -> f32 {
        let mut p = 0.0;
        let mut t = from;
        while t <= to {
            p = f.step(t, intent);
            t += 10;
        }
        p
    }

    #[test]
    fn resting_on_the_tab_opens_it_after_a_dwell_and_a_slide() {
        let mut f = Fold::default();
        assert_eq!(f.step(0, Intent::Away), 0.0);
        assert_eq!(run(&mut f, 0, 50, Intent::Over), 0.0, "not yet: the pointer only just arrived");
        assert!(!f.want_open);
        let mid = run(&mut f, 60, 150, Intent::Over);
        assert!(mid > 0.0 && mid < 1.0, "sliding: {mid}");
        assert!(f.sliding());
        assert_eq!(run(&mut f, 160, 400, Intent::Over), 1.0);
        assert!(!f.sliding());
    }

    #[test]
    fn a_pass_by_does_not_open_it() {
        let mut f = Fold::default();
        run(&mut f, 0, 60, Intent::Over);
        assert_eq!(run(&mut f, 70, 400, Intent::Away), 0.0);
        assert!(!f.want_open);
    }

    #[test]
    fn leaving_closes_it_after_the_delay_and_coming_back_in_time_keeps_it() {
        let mut f = Fold::default();
        run(&mut f, 0, 500, Intent::Over);
        assert_eq!(f.step(500, Intent::Over), 1.0);
        // gone for less than the delay: still open
        assert_eq!(run(&mut f, 510, 800, Intent::Away), 1.0);
        assert_eq!(run(&mut f, 810, 900, Intent::Over), 1.0);
        // gone for good: open through the delay, then it slides shut
        assert_eq!(run(&mut f, 910, 1200, Intent::Away), 1.0);
        let mid = run(&mut f, 1210, 1290, Intent::Away);
        assert!(mid < 1.0, "closing: {mid}");
        assert_eq!(run(&mut f, 1300, 1600, Intent::Away), 0.0);
    }

    #[test]
    fn coming_back_while_it_slides_shut_catches_it_at_once() {
        let mut f = Fold::default();
        run(&mut f, 0, 500, Intent::Over);
        run(&mut f, 510, 900, Intent::Away);
        f.step(910, Intent::Away);
        let p = f.step(950, Intent::Over);
        assert!(p > 0.0 && f.want_open, "no new dwell once it is moving");
    }

    #[test]
    fn hold_keeps_it_open_and_shut_keeps_it_folded_whatever_the_pointer_does() {
        let mut f = Fold::default();
        assert_eq!(run(&mut f, 0, 300, Intent::Hold), 1.0, "a drag of the card opens it without a dwell");
        assert_eq!(run(&mut f, 310, 2000, Intent::Hold), 1.0);
        assert_eq!(run(&mut f, 2010, 2500, Intent::Shut), 0.0);
        assert_eq!(run(&mut f, 2510, 2590, Intent::Over), 0.0, "after Shut the pointer has to rest again before it opens");
        assert_eq!(run(&mut f, 2600, 3000, Intent::Over), 1.0);
        let mut g = Fold::default();
        assert_eq!(run(&mut g, 0, 1000, Intent::Shut), 0.0, "dragging a folded tab does not unfold it");
    }

    #[test]
    fn covered_windows_and_pinned_cards_skip_the_animation() {
        let mut f = Fold::default();
        f.set_open();
        assert_eq!(f.step(0, Intent::Over), 1.0);
        assert!(!f.sliding());
        f.reset();
        assert_eq!(f.step(10, Intent::Away), 0.0);
        assert!(!f.sliding() && !f.want_open);
    }

    #[test]
    fn a_long_gap_between_ticks_finishes_the_slide_instead_of_overshooting() {
        let mut f = Fold::default();
        f.step(0, Intent::Hold);
        assert_eq!(f.step(10_000, Intent::Hold), 1.0);
        assert_eq!(f.step(20_000, Intent::Shut), 0.0);
    }
}
