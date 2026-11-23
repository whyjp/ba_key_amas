//! What the player chose: which widgets show, and where.
use crate::widget::{Kind, WidgetId};
use baocr_overlay::{Anchor, Dock, Presentation, Side};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Placement {
    /// On the card beside the game window (the add-on's main UI).
    Outside,
    /// Inside the game window.
    InGame,
    Off,
}

impl Placement {
    pub fn label(self, kind: Kind) -> &'static str {
        match (self, kind) {
            (Placement::Outside, _) => "게임 밖",
            (Placement::InGame, Kind::Card) => "게임 안",
            (Placement::InGame, Kind::Marks) => "켜짐",
            (Placement::Off, _) => "끔",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Master switch for everything drawn inside the game. Off hides it all there; the
    /// card beside the game stays.
    pub ingame_visible: bool,
    pub placements: BTreeMap<WidgetId, Placement>,
    /// The card's side of the game window (true: right). Moved by dragging the card's header.
    pub dock_right: bool,
    /// Where the card is anchored when it is not simply beside the game: "il" / "ir" (inside the
    /// game's top left / top right corner), or "l" / "r". Overrides `dock_right` when present
    /// (older files lack it); an unknown code is ignored.
    pub dock_anchor: Option<String>,
    /// "pinned" (always open) or "bookmark" (a folded tab that opens on hover). None: the
    /// default for where the card is (open beside the game, a tab inside it).
    pub presentation: Option<String>,
    /// The roster scan also reads the stat popup and the level-up and mystic tabs (slower).
    pub scan_deep: bool,
    /// The student-list section of the card is unfolded.
    pub student_open: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            ingame_visible: true,
            placements: BTreeMap::from([
                (WidgetId::MissionInfo, Placement::Outside),
                (WidgetId::RaidInfo, Placement::Outside),
                (WidgetId::Formation, Placement::Outside),
                (WidgetId::PortraitIcons, Placement::InGame),
                (WidgetId::SlotStats, Placement::InGame),
            ]),
            dock_right: true,
            dock_anchor: None,
            presentation: None,
            scan_deep: false,
            student_open: true,
        }
    }
}

impl Settings {
    pub fn dock(&self) -> Dock {
        let side = Anchor::Outside(if self.dock_right { Side::Right } else { Side::Left });
        Dock {
            anchor: self.dock_anchor.as_deref().and_then(Anchor::from_code).unwrap_or(side),
            presentation: self.presentation.as_deref().and_then(Presentation::from_code),
        }
    }

    pub fn set_anchor(&mut self, anchor: Anchor) {
        if let Anchor::Outside(side) = anchor {
            self.dock_right = side == Side::Right;
        }
        self.dock_anchor = Some(anchor.code().into());
    }

    pub fn set_presentation(&mut self, p: Presentation) {
        self.presentation = Some(p.code().into());
    }

    pub fn set_dock(&mut self, dock: Dock) {
        self.set_anchor(dock.anchor);
        if let Some(p) = dock.presentation {
            self.set_presentation(p);
        }
    }

    /// Where the widget is configured to go, falling back to the default for one the file lacks.
    pub fn placement(&self, id: WidgetId) -> Placement {
        self.placements.get(&id).copied().or_else(|| Settings::default().placements.get(&id).copied()).unwrap_or(Placement::Off)
    }

    /// Where it really goes now: the master switch removes everything inside the game.
    pub fn effective(&self, id: WidgetId) -> Placement {
        match self.placement(id) {
            Placement::InGame if !self.ingame_visible => Placement::Off,
            p => p,
        }
    }

    /// Next placement for a click on the widget's button. Cards go beside the game, then
    /// inside it, then off; icons over portraits are only on or off.
    pub fn cycle(&mut self, id: WidgetId, kind: Kind) {
        let next = match (self.placement(id), kind) {
            (Placement::Outside, _) => Placement::InGame,
            (Placement::InGame, _) => Placement::Off,
            (Placement::Off, Kind::Card) => Placement::Outside,
            (Placement::Off, Kind::Marks) => Placement::InGame,
        };
        self.placements.insert(id, next);
    }

    /// The settings file: `%APPDATA%\baocr\overlay.json`, else beside the program.
    pub fn default_path() -> PathBuf {
        std::env::var_os("APPDATA").map(|d| PathBuf::from(d).join("baocr")).unwrap_or_else(|| PathBuf::from(".")).join("overlay.json")
    }

    /// A missing or unreadable file gives the defaults.
    pub fn load(path: &Path) -> Settings {
        std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(self).unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_card_cycles_beside_inside_off_and_icons_toggle() {
        let mut s = Settings::default();
        assert_eq!(s.placement(WidgetId::Formation), Placement::Outside);
        s.cycle(WidgetId::Formation, Kind::Card);
        assert_eq!(s.placement(WidgetId::Formation), Placement::InGame);
        s.cycle(WidgetId::Formation, Kind::Card);
        assert_eq!(s.placement(WidgetId::Formation), Placement::Off);
        s.cycle(WidgetId::Formation, Kind::Card);
        assert_eq!(s.placement(WidgetId::Formation), Placement::Outside);
        s.cycle(WidgetId::PortraitIcons, Kind::Marks);
        assert_eq!(s.placement(WidgetId::PortraitIcons), Placement::Off);
        s.cycle(WidgetId::PortraitIcons, Kind::Marks);
        assert_eq!(s.placement(WidgetId::PortraitIcons), Placement::InGame);
    }

    #[test]
    fn the_master_switch_hides_what_is_inside_the_game_only() {
        let mut s = Settings::default();
        s.cycle(WidgetId::Formation, Kind::Card); // inside the game
        s.ingame_visible = false;
        assert_eq!(s.effective(WidgetId::Formation), Placement::Off);
        assert_eq!(s.effective(WidgetId::PortraitIcons), Placement::Off);
        assert_eq!(s.effective(WidgetId::MissionInfo), Placement::Outside, "the card beside the game is not affected");
        assert_eq!(s.placement(WidgetId::Formation), Placement::InGame, "the choice itself is kept");
    }

    #[test]
    fn settings_survive_a_round_trip_and_bad_files_give_defaults() {
        let dir = std::env::temp_dir().join(format!("baocr-settings-{}", std::process::id()));
        let path = dir.join("overlay.json");
        assert_eq!(Settings::load(&path), Settings::default(), "no file");
        let mut s = Settings::default();
        s.ingame_visible = false;
        s.cycle(WidgetId::SlotStats, Kind::Marks);
        s.save(&path).unwrap();
        assert_eq!(Settings::load(&path), s);
        std::fs::write(&path, "not json").unwrap();
        assert_eq!(Settings::load(&path), Settings::default());
        // a widget a file does not mention keeps its default, and so does a dock an old file lacks
        std::fs::write(&path, r#"{"ingame_visible":true,"placements":{}}"#).unwrap();
        assert_eq!(Settings::load(&path).placement(WidgetId::MissionInfo), Placement::Outside);
        assert_eq!(Settings::load(&path).dock(), Dock::default());
        // an older file that still carries a dock height loads, the height is ignored
        std::fs::write(&path, r#"{"dock_right":false,"dock_y_permille":300}"#).unwrap();
        assert_eq!(Settings::load(&path).dock(), Dock { anchor: Anchor::Outside(Side::Left), presentation: None }, "a file with only the dock still loads");
        // the first version's file (no anchor, no presentation) keeps its side and gets the defaults
        std::fs::write(&path, r#"{"ingame_visible":true,"placements":{},"dock_right":true,"scan_deep":false,"student_open":true}"#).unwrap();
        assert_eq!(Settings::load(&path).dock(), Dock::default());
        // the new choices are kept, and a code nobody knows falls back to the side
        let mut s = Settings::default();
        s.set_anchor(Anchor::Inside(baocr_overlay::Corner::TopRight));
        s.set_presentation(Presentation::Pinned);
        s.save(&path).unwrap();
        assert_eq!(Settings::load(&path).dock(), Dock { anchor: Anchor::Inside(baocr_overlay::Corner::TopRight), presentation: Some(Presentation::Pinned) });
        std::fs::write(&path, r#"{"dock_right":false,"dock_anchor":"upside-down","presentation":"floating"}"#).unwrap();
        assert_eq!(Settings::load(&path).dock(), Dock { anchor: Anchor::Outside(Side::Left), presentation: None });
        // dropping beside the game again takes the card back out, and sets the side for a later return
        let mut s = Settings::default();
        s.set_anchor(Anchor::Inside(baocr_overlay::Corner::TopLeft));
        s.set_anchor(Anchor::Outside(Side::Left));
        assert_eq!(s.dock().anchor, Anchor::Outside(Side::Left));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
