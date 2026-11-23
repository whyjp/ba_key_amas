//! Where the card's rectangles go, as geometry on plain numbers (tested without a window).
//!
//! The player prefers an anchor (beside the game, or inside one of its top corners). Beside the
//! game only works when the monitor has room for the card there; otherwise the card is shown
//! inside the game instead, and goes back when the room appears. The open card and the folded
//! tab are two rectangles of one anchor: the tab sits at the card's edge nearest the anchor and
//! the card unfolds away from it.
use crate::dock::{Anchor, Corner, Side};
use crate::render::Ltrb;

/// The game window as the placement sees it: client area, whole window, and the work area of its monitor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Frame {
    pub client: Ltrb,
    pub outer: Ltrb,
    pub work: Ltrb,
}

/// Which way the card grows out of its tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unfold {
    Left,
    Right,
}

/// An anchor's two rectangles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Resolved {
    pub anchor: Anchor,
    /// The open card (its height is cut when the game is shorter than the card).
    pub open: Ltrb,
    pub tab: Ltrb,
    pub unfold: Unfold,
}

/// Inside the game the card starts this far down the client area, below the game's own header
/// bar (currency, menu), like the card the add-on already draws inside the game.
const INSIDE_TOP: f32 = 0.075;

fn size_of(r: Ltrb) -> (i32, i32) {
    (r.2 - r.0, r.3 - r.1)
}

fn gap(client: Ltrb) -> i32 {
    (((client.3 - client.1) as f32 * 0.012).round() as i32).max(6)
}

fn inside_margin(client: Ltrb) -> i32 {
    (((client.2 - client.0) as f32 * 0.012).round() as i32).max(4)
}

/// The rectangle of a card of `size` at `anchor`; None when the anchor is beside the game and
/// the monitor has no room there. Beside the game its top is level with the game's client top
/// (pushed up if it would leave the monitor). Inside, it is kept in the client area: pushed up
/// as far as the client's top margin, then cut short.
pub fn slot(f: Frame, anchor: Anchor, size: (i32, i32)) -> Option<Ltrb> {
    let (pw, ph) = size;
    let (client, outer, work) = (f.client, f.outer, f.work);
    match anchor {
        Anchor::Outside(side) => {
            let g = gap(client);
            let y = client.1.clamp(work.1, (work.3 - ph).max(work.1));
            let x = match side {
                Side::Right => (outer.2 + g + pw <= work.2).then_some(outer.2 + g),
                Side::Left => (outer.0 - g - pw >= work.0).then_some(outer.0 - g - pw),
            }?;
            Some((x, y, x + pw, y + ph))
        }
        Anchor::Inside(corner) => {
            let m = inside_margin(client);
            let (cw, ch) = size_of(client);
            let w = pw.min(cw - 2 * m).max(1);
            let x = match corner {
                Corner::TopLeft => client.0 + m,
                Corner::TopRight => client.2 - m - w,
            };
            let top = client.1 + (ch as f32 * INSIDE_TOP).round() as i32;
            let bottom = (client.3 - m).min(work.3);
            let y = if top + ph <= bottom { top } else { (bottom - ph).max(client.1 + m).min(top) };
            let h = ph.min(bottom - y).max(1);
            Some((x, y, x + w, y + h))
        }
    }
}

/// The anchor the card is really shown at: the preferred one, else (beside the game, no room)
/// the other side, else the top corner inside the game on the preferred side. An inside
/// preference stays inside. `outside_w` is the card's width beside the game.
pub fn effective_anchor(f: Frame, outside_w: i32, preferred: Anchor) -> Anchor {
    let Anchor::Outside(side) = preferred else { return preferred };
    let other = if side == Side::Left { Side::Right } else { Side::Left };
    [side, other]
        .into_iter()
        .find(|s| slot(f, Anchor::Outside(*s), (outside_w, 1)).is_some())
        .map(Anchor::Outside)
        .unwrap_or(Anchor::Inside(if side == Side::Left { Corner::TopLeft } else { Corner::TopRight }))
}

/// The anchors a card can be dropped on: both corners inside the game, and each side that has room.
pub fn available(f: Frame, outside_w: i32) -> Vec<Anchor> {
    let mut v = Vec::with_capacity(4);
    for side in [Side::Left, Side::Right] {
        if slot(f, Anchor::Outside(side), (outside_w, 1)).is_some() {
            v.push(Anchor::Outside(side));
        }
    }
    v.push(Anchor::Inside(Corner::TopLeft));
    v.push(Anchor::Inside(Corner::TopRight));
    v
}

/// The anchor a thing of `size` dropped with the pointer at `drop` means: the available one
/// whose rectangle's middle is nearest. (Used for the open card and for the tab alike; `size`
/// is the dragged thing's.)
pub fn snap(f: Frame, size: (i32, i32), drop: (i32, i32), available: &[Anchor]) -> Anchor {
    let dist = |a: &Anchor| {
        slot(f, *a, size).map_or(i64::MAX, |r| {
            let (dx, dy) = (((r.0 + r.2) / 2 - drop.0) as i64, ((r.1 + r.3) / 2 - drop.1) as i64);
            dx * dx + dy * dy
        })
    };
    available.iter().copied().min_by_key(dist).unwrap_or_default()
}

/// Which way a card anchored at `anchor` grows out of its tab: away from the edge it hangs on.
pub fn unfold_of(anchor: Anchor) -> Unfold {
    match anchor {
        Anchor::Outside(Side::Right) | Anchor::Inside(Corner::TopLeft) => Unfold::Right,
        Anchor::Outside(Side::Left) | Anchor::Inside(Corner::TopRight) => Unfold::Left,
    }
}

/// Both rectangles of the card at `anchor` (an effective anchor; one beside the game that has no
/// room is treated as the matching inside corner).
pub fn resolve(f: Frame, anchor: Anchor, card: (i32, i32), tab: (i32, i32)) -> Resolved {
    let (anchor, open) = match slot(f, anchor, card) {
        Some(r) => (anchor, r),
        None => {
            let c = if matches!(anchor, Anchor::Outside(Side::Left)) { Corner::TopLeft } else { Corner::TopRight };
            let a = Anchor::Inside(c);
            (a, slot(f, a, card).expect("inside always has a slot"))
        }
    };
    let unfold = unfold_of(anchor);
    let (ow, oh) = size_of(open);
    let (tw, th) = (tab.0.min(ow), tab.1.min(oh));
    let tab = match unfold {
        Unfold::Right => (open.0, open.1, open.0 + tw, open.1 + th),
        Unfold::Left => (open.2 - tw, open.1, open.2, open.1 + th),
    };
    Resolved { anchor, open, tab, unfold }
}

/// The part of the card that has come out at slide progress `p` (0 folded, 1 open): the screen
/// rectangle and the first column of the open card's image shown in it. The card slides out of
/// its edge, so the columns shown are the ones that have crossed it: the card's right end when
/// it grows to the right, its left end when it grows to the left. The height grows with it.
pub fn strip(r: &Resolved, p: f32) -> (Ltrb, i32) {
    let p = p.clamp(0.0, 1.0);
    let ((ow, oh), (tw, th)) = (size_of(r.open), size_of(r.tab));
    let vw = tw + (((ow - tw) as f32) * p).round() as i32;
    let vh = th + (((oh - th) as f32) * p).round() as i32;
    match r.unfold {
        Unfold::Right => ((r.open.0, r.open.1, r.open.0 + vw, r.open.1 + vh), ow - vw),
        Unfold::Left => ((r.open.2 - vw, r.open.1, r.open.2, r.open.1 + vh), 0),
    }
}

/// How far the pointer must travel between press and release for it to be a move; a press that
/// stays put (a click on the tab) must not change where the card lives.
pub const DRAG_SLOP: i32 = 6;

/// The anchor a finished drag chooses, or None when it chose nothing: the pointer hardly moved
/// (a click), or there was nothing to snap to (the game went away mid-drag). In both cases the
/// stored preference is left alone.
pub fn dropped(f: Frame, size: (i32, i32), pressed_at: (i32, i32), released_at: (i32, i32), available: &[Anchor]) -> Option<Anchor> {
    let moved = (released_at.0 - pressed_at.0).abs().max((released_at.1 - pressed_at.1).abs()) >= DRAG_SLOP;
    (moved && !available.is_empty()).then(|| snap(f, size, released_at, available))
}

pub fn contains(r: Ltrb, p: (i32, i32)) -> bool {
    p.0 >= r.0 && p.0 < r.2 && p.1 >= r.1 && p.1 < r.3
}

#[cfg(test)]
mod tests {
    use super::*;

    // a QHD monitor with the 4:3 game windowed in the middle of it
    const QHD_WORK: Ltrb = (0, 0, 2560, 1400);
    fn qhd_window() -> Frame {
        Frame { client: (640, 100, 1920, 1060), outer: (632, 70, 1928, 1068), work: QHD_WORK }
    }
    // a 1080p monitor with the game filling it
    fn full_1080() -> Frame {
        Frame { client: (0, 31, 1920, 1040), outer: (0, 0, 1920, 1040), work: (0, 0, 1920, 1040) }
    }
    const CARD: (i32, i32) = (360, 500);
    const TAB: (i32, i32) = (28, 72);

    #[test]
    fn a_roomy_qhd_window_keeps_the_card_beside_the_game_level_with_its_top() {
        let f = qhd_window();
        let a = effective_anchor(f, 360, Anchor::Outside(Side::Right));
        assert_eq!(a, Anchor::Outside(Side::Right));
        let r = resolve(f, a, CARD, TAB);
        assert!(r.open.0 >= f.outer.2 && r.open.0 - f.outer.2 < 20, "against the right edge: {:?}", r.open);
        assert_eq!(r.open.1, f.client.1, "level with the top of the game");
        assert_eq!(size_of(r.open), CARD);
        assert_eq!(r.unfold, Unfold::Right);
        assert_eq!(r.tab.0, r.open.0, "the tab hangs on the edge nearest the game");
        assert_eq!(size_of(r.tab), TAB);
        let l = resolve(f, Anchor::Outside(Side::Left), CARD, TAB);
        assert!(l.open.2 <= f.outer.0 && f.outer.0 - l.open.2 < 20);
        assert_eq!((l.unfold, l.tab.2), (Unfold::Left, l.open.2));
    }

    #[test]
    fn a_game_filling_the_monitor_puts_the_card_inside_and_the_preference_is_kept_for_later() {
        let f = full_1080();
        for (pref, corner) in [(Side::Right, Corner::TopRight), (Side::Left, Corner::TopLeft)] {
            assert_eq!(effective_anchor(f, 360, Anchor::Outside(pref)), Anchor::Inside(corner));
        }
        // the same preference is outside again when the window shrinks
        assert_eq!(effective_anchor(qhd_window(), 360, Anchor::Outside(Side::Left)), Anchor::Outside(Side::Left));
        let r = resolve(f, Anchor::Inside(Corner::TopRight), (320, 400), TAB);
        assert!(r.open.2 < f.client.2 && f.client.2 - r.open.2 <= 24, "a small margin from the right: {:?}", r.open);
        assert!(r.open.1 > f.client.1, "below the game's own header bar");
        assert_eq!(r.unfold, Unfold::Left, "a right-anchored card grows to the left");
        assert_eq!(r.tab.2, r.open.2);
        let l = resolve(f, Anchor::Inside(Corner::TopLeft), (320, 400), TAB);
        assert!(l.open.0 > f.client.0 && l.open.0 - f.client.0 <= 24);
        assert_eq!((l.unfold, l.tab.0), (Unfold::Right, l.open.0));
    }

    #[test]
    fn the_other_side_is_used_before_going_inside() {
        // a window at the monitor's right edge: no room right, room left
        let f = Frame { client: (1500, 100, 1900, 400), outer: (1492, 70, 1908, 408), work: (0, 0, 1920, 1040) };
        assert_eq!(effective_anchor(f, 300, Anchor::Outside(Side::Right)), Anchor::Outside(Side::Left));
        assert_eq!(available(f, 300), vec![Anchor::Outside(Side::Left), Anchor::Inside(Corner::TopLeft), Anchor::Inside(Corner::TopRight)]);
        // an inside preference stays inside even with room outside
        assert_eq!(effective_anchor(qhd_window(), 360, Anchor::Inside(Corner::TopLeft)), Anchor::Inside(Corner::TopLeft));
    }

    #[test]
    fn a_card_never_leaves_the_monitor_or_the_game_and_is_cut_when_the_game_is_short() {
        // beside the game: pushed up so it stays on the monitor
        let f = Frame { client: (600, 900, 1060, 1030), outer: (592, 870, 1068, 1038), work: (0, 0, 1920, 1040) };
        let r = resolve(f, Anchor::Outside(Side::Right), (300, 400), TAB);
        assert!(r.open.3 <= f.work.3, "{:?}", r.open);
        // inside a short game window: kept in the client area, the height cut
        let short = Frame { client: (100, 100, 740, 380), outer: (92, 70, 748, 388), work: (0, 0, 1920, 1040) };
        let r = resolve(short, Anchor::Inside(Corner::TopRight), (320, 700), TAB);
        assert!(r.open.0 >= short.client.0 && r.open.2 <= short.client.2 && r.open.1 >= short.client.1 && r.open.3 <= short.client.3, "{:?}", r.open);
        assert!(size_of(r.open).1 < 700 && size_of(r.open).1 > 150, "cut, not collapsed: {:?}", r.open);
        // a card wider than the game is narrowed to it
        let narrow = Frame { client: (100, 100, 300, 400), outer: (92, 70, 308, 408), work: (0, 0, 1920, 1040) };
        assert!(size_of(slot(narrow, Anchor::Inside(Corner::TopLeft), (320, 200)).unwrap()).0 <= 200);
    }

    #[test]
    fn a_tiny_work_area_still_gives_every_anchor_a_rectangle_inside_it() {
        let f = Frame { client: (0, 0, 400, 300), outer: (0, 0, 400, 300), work: (0, 0, 400, 300) };
        assert_eq!(available(f, 230), vec![Anchor::Inside(Corner::TopLeft), Anchor::Inside(Corner::TopRight)]);
        for a in available(f, 230) {
            let r = resolve(f, a, (230, 600), TAB);
            assert!(r.open.0 >= 0 && r.open.2 <= 400 && r.open.1 >= 0 && r.open.3 <= 300, "{a:?} {:?}", r.open);
            assert!(r.tab.0 >= r.open.0 && r.tab.2 <= r.open.2 && r.tab.3 <= r.open.3, "the tab is part of the card's rectangle");
        }
    }

    #[test]
    fn a_drop_snaps_to_the_nearest_available_anchor() {
        let f = qhd_window();
        let all = available(f, 360);
        assert_eq!(all.len(), 4);
        // the card dropped with the pointer far left of the game, beside it, inside it near each edge
        assert_eq!(snap(f, CARD, (200, 300), &all), Anchor::Outside(Side::Left));
        assert_eq!(snap(f, CARD, (2400, 300), &all), Anchor::Outside(Side::Right));
        assert_eq!(snap(f, CARD, (800, 300), &all), Anchor::Inside(Corner::TopLeft));
        assert_eq!(snap(f, CARD, (1750, 300), &all), Anchor::Inside(Corner::TopRight));
        // dropping low does not move it down: only the side counts
        assert_eq!(snap(f, CARD, (800, 1300), &all), Anchor::Inside(Corner::TopLeft));
        // the tab, which is small, snaps by where it was dropped just the same
        assert_eq!(snap(f, TAB, (1880, 200), &all), Anchor::Inside(Corner::TopRight));
        assert_eq!(snap(f, TAB, (1960, 150), &all), Anchor::Outside(Side::Right));
        // without room beside the game only the inside corners are offered
        let full = full_1080();
        assert_eq!(snap(full, CARD, (1800, 100), &available(full, 360)), Anchor::Inside(Corner::TopRight));
        assert_eq!(snap(full, CARD, (100, 100), &available(full, 360)), Anchor::Inside(Corner::TopLeft));
    }

    #[test]
    fn a_click_or_a_lost_game_does_not_change_the_stored_place() {
        let f = full_1080();
        let avail = available(f, 360);
        assert_eq!(dropped(f, TAB, (1900, 100), (1902, 101), &avail), None, "a press that stays put");
        assert_eq!(dropped(f, TAB, (1900, 100), (100, 100), &avail), Some(Anchor::Inside(Corner::TopLeft)), "a real move");
        assert_eq!(dropped(f, TAB, (1900, 100), (100, 100), &[]), None, "no game, nothing to snap to");
        // a move that ends where the card already is still counts: it is a choice (beside the game -> inside)
        assert_eq!(effective_anchor(f, 360, Anchor::Outside(Side::Right)), Anchor::Inside(Corner::TopRight));
        assert_eq!(dropped(f, TAB, (1900, 100), (1800, 150), &avail), Some(Anchor::Inside(Corner::TopRight)));
    }

    #[test]
    fn the_strip_grows_from_the_tab_to_the_card_out_of_its_edge() {
        let f = full_1080();
        let r = resolve(f, Anchor::Inside(Corner::TopRight), (320, 400), TAB);
        let (folded, _) = strip(&r, 0.0);
        assert_eq!(folded, r.tab);
        let (open, x) = strip(&r, 1.0);
        assert_eq!((open, x), (r.open, 0));
        let (mid, x) = strip(&r, 0.5);
        assert_eq!(mid.2, r.open.2, "stays on its edge");
        assert!(mid.0 > r.open.0 && mid.0 < r.tab.0 && x == 0, "{mid:?}");
        // growing to the right shows the card's right end: the leading edge is the card's own
        let r = resolve(f, Anchor::Inside(Corner::TopLeft), (320, 400), TAB);
        let (mid, x) = strip(&r, 0.5);
        assert_eq!(mid.0, r.open.0);
        assert_eq!(x, 320 - (mid.2 - mid.0), "the columns that crossed the edge");
        assert!(contains(mid, (mid.0, mid.1)) && !contains(mid, (mid.2, mid.1)));
    }
}
