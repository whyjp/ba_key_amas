//! The output port: where the assistant's cards and icons go. The assistant only knows
//! this trait; the Windows overlay implements it, and tests use a recording fake.
use baocr_overlay::{Dock, Mark, Overlay, PanelState};

pub trait Surface {
    /// The add-on's main card, beside the game window (with the settings buttons).
    fn main_card(&self, card: PanelState);
    /// A card inside the game window; None removes it.
    fn ingame_card(&self, card: Option<PanelState>);
    /// Icons over the game's portraits (positions in the 1280x960 frame).
    fn marks(&self, marks: Vec<Mark>);
    /// Ids of the buttons clicked since the last call.
    fn take_events(&self) -> Vec<String>;
    /// Where the main card sits against the game window.
    fn set_dock(&self, dock: Dock);
    /// Keep the player's mouse and keyboard off the game (only the scan's own input gets through;
    /// ESC raises the stop flag). Returns whether the block is in force.
    fn lock_input(&self, on: bool) -> bool;
}

impl Surface for Overlay {
    fn main_card(&self, card: PanelState) {
        self.panel(card);
    }
    fn ingame_card(&self, card: Option<PanelState>) {
        Overlay::ingame_card(self, card);
    }
    fn marks(&self, marks: Vec<Mark>) {
        Overlay::marks(self, marks);
    }
    fn take_events(&self) -> Vec<String> {
        Overlay::take_events(self)
    }
    fn set_dock(&self, dock: Dock) {
        Overlay::set_dock(self, dock);
    }
    fn lock_input(&self, on: bool) -> bool {
        self.block_input(on)
    }
}
