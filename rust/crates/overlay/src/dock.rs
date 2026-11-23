//! Where the add-on's card sits against the game window: on its right or left side, at
//! some height. The player moves it by dragging its header; on release it snaps back to the
//! nearest side. All of this is geometry on plain numbers, so it is tested without a window.
use crate::render::Ltrb;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
}

/// The card's place: a side of the game window and a height down from the top of the
/// game's client area, in thousandths of its height (so it follows any window size).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dock {
    pub side: Side,
    pub y_permille: i32,
}

impl Default for Dock {
    fn default() -> Self {
        Dock { side: Side::Right, y_permille: 0 }
    }
}

impl Dock {
    /// "dock:r:120": the form a finished drag is reported in.
    pub fn encode(self) -> String {
        format!("dock:{}:{}", if self.side == Side::Left { 'l' } else { 'r' }, self.y_permille)
    }

    pub fn decode(event: &str) -> Option<Dock> {
        let rest = event.strip_prefix("dock:")?;
        let (side, y) = rest.split_once(':')?;
        let side = match side {
            "l" => Side::Left,
            "r" => Side::Right,
            _ => return None,
        };
        Some(Dock { side, y_permille: y.parse::<i32>().ok()?.clamp(0, 1000) })
    }
}

/// Where a card of `size` goes in screen coordinates: against the preferred side of the
/// game window, else against the other side. None when neither side has room on the monitor.
pub fn dock_origin(client: Ltrb, outer: Ltrb, work: Ltrb, size: (i32, i32), dock: Dock) -> Option<(i32, i32)> {
    let (pw, ph) = size;
    let ch = client.3 - client.1;
    let gap = ((ch as f32 * 0.012).round() as i32).max(6);
    let wanted_y = client.1 + (ch as i64 * dock.y_permille as i64 / 1000) as i32;
    let y = wanted_y.clamp(work.1, (work.3 - ph).max(work.1));
    let right = (outer.2 + gap + pw <= work.2).then_some((outer.2 + gap, y));
    let left = (outer.0 - gap - pw >= work.0).then_some((outer.0 - gap - pw, y));
    match dock.side {
        Side::Right => right.or(left),
        Side::Left => left.or(right),
    }
}

/// The dock a card dropped with its top-left corner at `at` means: the side whose half of
/// the game window the card's middle is over, and the height it was dropped at.
pub fn dock_from_drop(client: Ltrb, at: (i32, i32), size: (i32, i32)) -> Dock {
    let (cw, ch) = (client.2 - client.0, (client.3 - client.1).max(1));
    let middle = at.0 + size.0 / 2;
    let side = if middle < client.0 + cw / 2 { Side::Left } else { Side::Right };
    let y_permille = (((at.1 - client.1) as i64 * 1000) / ch as i64).clamp(0, 1000) as i32;
    Dock { side, y_permille }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORK: Ltrb = (0, 0, 1920, 1040);
    const OUTER: Ltrb = (592, 70, 1068, 458);
    const CLIENT: Ltrb = (600, 100, 1060, 450);

    #[test]
    fn the_card_sits_against_the_chosen_side_at_the_chosen_height() {
        let size = (300, 400);
        let (x, y) = dock_origin(CLIENT, OUTER, WORK, size, Dock::default()).unwrap();
        assert!(x >= OUTER.2 && x - OUTER.2 < 20, "against the right edge: {x}");
        assert_eq!(y, CLIENT.1, "aligned with the top of the game");
        let (x, y) = dock_origin(CLIENT, OUTER, WORK, size, Dock { side: Side::Left, y_permille: 500 }).unwrap();
        assert!(x + size.0 <= OUTER.0 && OUTER.0 - (x + size.0) < 20, "against the left edge: {x}");
        assert_eq!(y, CLIENT.1 + 175, "half way down the game");
    }

    #[test]
    fn a_full_side_falls_back_to_the_other_and_no_room_gives_none() {
        let size = (300, 400);
        // the game at the right edge of the monitor: the preferred right side is full
        let (x, _) = dock_origin((1500, 100, 1900, 400), (1492, 70, 1908, 408), WORK, size, Dock::default()).unwrap();
        assert!(x + size.0 <= 1492, "moved to the left: {x}");
        assert_eq!(dock_origin((0, 31, 1920, 1040), (0, 0, 1920, 1040), WORK, size, Dock::default()), None);
        // never below the work area
        let (_, y) = dock_origin((600, 900, 1060, 1030), (592, 870, 1068, 1038), WORK, size, Dock { side: Side::Right, y_permille: 1000 }).unwrap();
        assert!(y + size.1 <= WORK.3, "{y}");
    }

    #[test]
    fn a_drop_snaps_to_the_side_its_middle_is_over_and_keeps_the_height() {
        let size = (300, 200);
        // dropped with its middle left of the window's middle: the left side
        let d = dock_from_drop(CLIENT, (500, 275), size);
        assert_eq!((d.side, d.y_permille), (Side::Left, 500));
        // dropped far right, above the window: right, height clamped to the top
        let d = dock_from_drop(CLIENT, (1100, 20), size);
        assert_eq!((d.side, d.y_permille), (Side::Right, 0));
        let d = dock_from_drop(CLIENT, (900, 5000), size);
        assert_eq!(d.y_permille, 1000);
    }

    #[test]
    fn a_dock_survives_being_sent_as_an_event() {
        for d in [Dock::default(), Dock { side: Side::Left, y_permille: 733 }] {
            assert_eq!(Dock::decode(&d.encode()), Some(d));
        }
        assert_eq!(Dock::decode("dock:x:5"), None);
        assert_eq!(Dock::decode("menu"), None);
        assert_eq!(Dock::decode("dock:r:99999").map(|d| d.y_permille), Some(1000));
    }
}
