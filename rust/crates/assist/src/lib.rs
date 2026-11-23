//! The on-screen assistant, built from a few replaceable parts:
//!
//! - `observe`: the input side. A frame in, an `Observation` out (screen, mission, cards, squad).
//! - `widget`, `widgets`, `matchup`: what could be shown. Each widget turns the observation into rows or icons;
//!   `matchup` is the pure part that words and colours a battle matchup.
//! - `settings`: what the player chose (on/off, beside the game or inside it).
//! - `surface`: the output side, a trait the overlay window implements.
//! - `assistant`: the coordinator that connects them.
//!
//! Each part depends only on the ones above it in this list, so a widget can be added without
//! touching the observer, and a different surface (or none, in tests) can replace the overlay.
pub mod assistant;
pub mod library;
pub mod matchup;
pub mod observe;
pub mod settings;
pub mod surface;
pub mod widget;
pub mod widgets;

pub use assistant::{Assistant, Command};
pub use library::{read_library, LibraryInfo};
pub use observe::{Observation, Observer};
pub use settings::{Placement, Settings};
pub use surface::Surface;
pub use widget::{Context, Kind, Lookup, Output, Widget, WidgetId};
