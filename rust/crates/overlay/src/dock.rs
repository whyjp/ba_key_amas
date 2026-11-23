//! Where the add-on's card sits against the game window: against its right or left side, with
//! its top level with the top of the game. The player moves it by dragging its header sideways;
//! on release it snaps to the nearest side. All of this is geometry on plain numbers, so it is
//! tested without a window.
use crate::render::Ltrb;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
}

/// The card's place: a side of the game window. (Its height is not a choice: the card always
/// starts at the top of the game's client area.)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dock {
    pub side: Side,
}

impl Default for Dock {
    fn default() -> Self {
        Dock { side: Side::Right }
    }
}

impl Dock {
    /// "dock:r": the form a finished drag is reported in.
    pub fn encode(self) -> String {
        format!("dock:{}", if self.side == Side::Left { 'l' } else { 'r' })
    }

    pub fn decode(event: &str) -> Option<Dock> {
        match event.strip_prefix("dock:")? {
            "l" => Some(Dock { side: Side::Left }),
            "r" => Some(Dock { side: Side::Right }),
            _ => None,
        }
    }
}

/// Where a card of `size` goes in screen coordinates: against the preferred side of the game
/// window, else against the other side, its top level with the game's client area. None when
/// neither side has room on the monitor.
pub fn dock_origin(client: Ltrb, outer: Ltrb, work: Ltrb, size: (i32, i32), dock: Dock) -> Option<(i32, i32)> {
    let (pw, ph) = size;
    let gap = (((client.3 - client.1) as f32 * 0.012).round() as i32).max(6);
    let y = client.1.clamp(work.1, (work.3 - ph).max(work.1));
    let right = (outer.2 + gap + pw <= work.2).then_some((outer.2 + gap, y));
    let left = (outer.0 - gap - pw >= work.0).then_some((outer.0 - gap - pw, y));
    match dock.side {
        Side::Right => right.or(left),
        Side::Left => left.or(right),
    }
}

/// The dock a card dropped with its top-left corner at `at` means: the side whose half of the
/// game window the card's middle is over.
pub fn dock_from_drop(client: Ltrb, at: (i32, i32), size: (i32, i32)) -> Dock {
    let cw = client.2 - client.0;
    let middle = at.0 + size.0 / 2;
    Dock { side: if middle < client.0 + cw / 2 { Side::Left } else { Side::Right } }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORK: Ltrb = (0, 0, 1920, 1040);
    const OUTER: Ltrb = (592, 70, 1068, 458);
    const CLIENT: Ltrb = (600, 100, 1060, 450);

    #[test]
    fn the_card_sits_against_the_chosen_side_with_its_top_level_with_the_game() {
        let size = (300, 400);
        let (x, y) = dock_origin(CLIENT, OUTER, WORK, size, Dock::default()).unwrap();
        assert!(x >= OUTER.2 && x - OUTER.2 < 20, "against the right edge: {x}");
        assert_eq!(y, CLIENT.1, "level with the top of the game");
        let (x, y) = dock_origin(CLIENT, OUTER, WORK, size, Dock { side: Side::Left }).unwrap();
        assert!(x + size.0 <= OUTER.0 && OUTER.0 - (x + size.0) < 20, "against the left edge: {x}");
        assert_eq!(y, CLIENT.1);
    }

    #[test]
    fn a_full_side_falls_back_to_the_other_and_no_room_gives_none() {
        let size = (300, 400);
        let (x, _) = dock_origin((1500, 100, 1900, 400), (1492, 70, 1908, 408), WORK, size, Dock::default()).unwrap();
        assert!(x + size.0 <= 1492, "moved to the left: {x}");
        assert_eq!(dock_origin((0, 31, 1920, 1040), (0, 0, 1920, 1040), WORK, size, Dock::default()), None);
        // a card taller than the room below the game's top is pushed up, never off the monitor
        let (_, y) = dock_origin((600, 900, 1060, 1030), (592, 870, 1068, 1038), WORK, size, Dock::default()).unwrap();
        assert!(y + size.1 <= WORK.3, "{y}");
    }

    #[test]
    fn a_drop_snaps_to_the_side_its_middle_is_over_whatever_the_height() {
        let size = (300, 200);
        assert_eq!(dock_from_drop(CLIENT, (500, 275), size).side, Side::Left);
        assert_eq!(dock_from_drop(CLIENT, (1100, 20), size).side, Side::Right);
        assert_eq!(dock_from_drop(CLIENT, (900, 5000), size).side, Side::Right, "dropping low does not move it down");
    }

    #[test]
    fn a_dock_survives_being_sent_as_an_event() {
        for d in [Dock::default(), Dock { side: Side::Left }] {
            assert_eq!(Dock::decode(&d.encode()), Some(d));
        }
        assert_eq!(Dock::decode("dock:x"), None);
        assert_eq!(Dock::decode("menu"), None);
    }
}
