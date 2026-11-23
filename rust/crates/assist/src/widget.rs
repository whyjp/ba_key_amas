//! The seam between what is known and what is shown.
//!
//! A `Widget` takes the `Context` (the observation plus what the wiki says about it)
//! and produces an `Output`: rows for a card, or icons over the game's own portraits.
//! Where an output ends up (beside the game, inside it, nowhere) is not the widget's
//! business: that is the settings' and the assistant's.
use crate::observe::Observation;
use baocr_extract::scene::Scene;
use baocr_overlay::{Mark, Row};
use baocr_wikidb::{MissionBrief, StudentBrief, TypeTable};
use serde::{Deserialize, Serialize};

/// Every widget has one stable id, used in the settings file and in button events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WidgetId {
    MissionInfo,
    Formation,
    PortraitIcons,
    SlotStats,
    FurnitureStudents,
}

impl WidgetId {
    pub const ALL: [WidgetId; 5] = [WidgetId::MissionInfo, WidgetId::Formation, WidgetId::PortraitIcons, WidgetId::SlotStats, WidgetId::FurnitureStudents];

    /// The id as it appears in button events ("place:formation").
    pub fn key(self) -> &'static str {
        match self {
            WidgetId::MissionInfo => "mission_info",
            WidgetId::Formation => "formation",
            WidgetId::PortraitIcons => "portrait_icons",
            WidgetId::SlotStats => "slot_stats",
            WidgetId::FurnitureStudents => "furniture_students",
        }
    }

    pub fn from_key(key: &str) -> Option<WidgetId> {
        WidgetId::ALL.into_iter().find(|w| w.key() == key)
    }
}

/// What a widget draws on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Rows of a card: beside the game or inside it.
    Card,
    /// Icons placed over the game's own portraits: always inside the game.
    Marks,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Output {
    Card { title: String, rows: Vec<Row> },
    Marks(Vec<Mark>),
}

/// How the mission screen the player picked resolves in the wiki.
#[derive(Debug, Clone, PartialEq)]
pub enum Lookup {
    /// No mission is in play, or the screen has nothing to do with one.
    None,
    /// The stage number was seen but not the place it belongs to.
    PlaceUnknown,
    NotFound,
    Found { brief: MissionBrief, alternatives: usize },
}

pub struct Context<'a> {
    pub obs: &'a Observation,
    pub lookup: Lookup,
    pub students: &'a [StudentBrief],
    pub types: &'a TypeTable,
}

impl Context<'_> {
    /// The mission's terrain code (Street | Outdoor | Indoor), when it is known.
    pub fn terrain(&self) -> Option<&str> {
        match &self.lookup {
            Lookup::Found { brief, .. } => brief.terrain.as_ref().map(|t| t.code.as_str()),
            _ => None,
        }
    }

    pub fn brief(&self) -> Option<&MissionBrief> {
        match &self.lookup {
            Lookup::Found { brief, .. } => Some(brief),
            _ => None,
        }
    }

    /// The squad's students, strikers and specials in the order seen.
    pub fn team(&self) -> Vec<StudentBrief> {
        self.obs.squad.iter().filter_map(|id| self.students.iter().find(|s| s.id == *id).cloned()).collect()
    }
}

/// Screens that belong to a picked mission.
pub fn is_mission_screen(scene: Scene) -> bool {
    matches!(scene, Scene::MissionInfo | Scene::EnemyInfo | Scene::MissionMap | Scene::Squad | Scene::QuickFormation)
}

pub trait Widget {
    fn id(&self) -> WidgetId;
    /// Its name in the settings menu.
    fn title(&self) -> &'static str;
    fn kind(&self) -> Kind;
    /// What to draw now, or None when there is nothing to show on this screen.
    fn render(&self, cx: &Context) -> Option<Output>;
}
