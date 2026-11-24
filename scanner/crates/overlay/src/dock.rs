//! The vocabulary of where the card lives: against a side of the game window or inside one of
//! its top corners, and whether it is always open or folded into a bookmark tab. Plain data and
//! the short codes they travel as (events, the settings file); the geometry is in `placement`.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Corner {
    TopLeft,
    TopRight,
}

/// Where the card is anchored: beside the game window (its top level with the game's client
/// top) or inside the game window at one of its top corners.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    Outside(Side),
    Inside(Corner),
}

impl Default for Anchor {
    fn default() -> Self {
        Anchor::Outside(Side::Right)
    }
}

impl Anchor {
    /// The code a drag reports and the settings file keeps: "l" / "r" beside the game (the
    /// original "dock:l" and "dock:r"), "il" / "ir" inside its top left / top right corner.
    pub fn code(self) -> &'static str {
        match self {
            Anchor::Outside(Side::Left) => "l",
            Anchor::Outside(Side::Right) => "r",
            Anchor::Inside(Corner::TopLeft) => "il",
            Anchor::Inside(Corner::TopRight) => "ir",
        }
    }

    pub fn from_code(code: &str) -> Option<Anchor> {
        match code {
            "l" => Some(Anchor::Outside(Side::Left)),
            "r" => Some(Anchor::Outside(Side::Right)),
            "il" => Some(Anchor::Inside(Corner::TopLeft)),
            "ir" => Some(Anchor::Inside(Corner::TopRight)),
            _ => None,
        }
    }

    /// "dock:r": the form a finished drag is reported in.
    pub fn encode(self) -> String {
        format!("dock:{}", self.code())
    }

    pub fn decode(event: &str) -> Option<Anchor> {
        Anchor::from_code(event.strip_prefix("dock:")?)
    }

    pub fn is_inside(self) -> bool {
        matches!(self, Anchor::Inside(_))
    }
}

/// How the card shows itself: always open, or folded into a small tab that opens while the
/// pointer is on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Presentation {
    Pinned,
    Bookmark,
}

impl Presentation {
    pub fn code(self) -> &'static str {
        match self {
            Presentation::Pinned => "pinned",
            Presentation::Bookmark => "bookmark",
        }
    }

    pub fn from_code(code: &str) -> Option<Presentation> {
        match code {
            "pinned" => Some(Presentation::Pinned),
            "bookmark" => Some(Presentation::Bookmark),
            _ => None,
        }
    }

    /// "present:bookmark": reported when the player presses the pin button.
    pub fn encode(self) -> String {
        format!("present:{}", self.code())
    }

    pub fn decode(event: &str) -> Option<Presentation> {
        Presentation::from_code(event.strip_prefix("present:")?)
    }

    pub fn toggled(self) -> Presentation {
        match self {
            Presentation::Pinned => Presentation::Bookmark,
            Presentation::Bookmark => Presentation::Pinned,
        }
    }

    /// What the card does when the player has not chosen: open beside the game (nothing is
    /// covered), a tab inside it (an open card would cover the game).
    pub fn default_for(anchor: Anchor) -> Presentation {
        if anchor.is_inside() {
            Presentation::Bookmark
        } else {
            Presentation::Pinned
        }
    }
}

/// What the player prefers. The anchor is a preference: with no room beside the game the card is
/// shown inside it, and goes back when the room is there again. `presentation` None means
/// "the default for wherever the card really is".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dock {
    pub anchor: Anchor,
    pub presentation: Option<Presentation>,
}

impl Default for Dock {
    fn default() -> Self {
        Dock { anchor: Anchor::Outside(Side::Right), presentation: None }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_anchor_survives_being_sent_as_an_event_and_the_old_codes_still_work() {
        for a in [Anchor::Outside(Side::Left), Anchor::Outside(Side::Right), Anchor::Inside(Corner::TopLeft), Anchor::Inside(Corner::TopRight)] {
            assert_eq!(Anchor::decode(&a.encode()), Some(a));
        }
        assert_eq!(Anchor::decode("dock:l"), Some(Anchor::Outside(Side::Left)), "the first version's event");
        assert_eq!(Anchor::decode("dock:r"), Some(Anchor::Outside(Side::Right)));
        assert_eq!(Anchor::decode("dock:x"), None);
        assert_eq!(Anchor::decode("menu"), None);
        assert_eq!(Presentation::decode("present:bookmark"), Some(Presentation::Bookmark));
        assert_eq!(Presentation::decode(&Presentation::Pinned.encode()), Some(Presentation::Pinned));
        assert_eq!(Presentation::decode("present:x"), None);
    }

    #[test]
    fn the_default_presentation_follows_where_the_card_really_is() {
        assert_eq!(Presentation::default_for(Anchor::Outside(Side::Right)), Presentation::Pinned);
        assert_eq!(Presentation::default_for(Anchor::Inside(Corner::TopRight)), Presentation::Bookmark);
        assert_eq!(Presentation::Pinned.toggled(), Presentation::Bookmark);
    }
}
