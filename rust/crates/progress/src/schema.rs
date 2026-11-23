//! The progress snapshot the scan writes to disk. A value that could not be read is `null`, and the reason is
//! in `warnings`; nothing is guessed. The running event's stages (`events.open`) and the always-open past
//! events (`events.past`) are separate records and are never merged.
use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProgressSnapshot {
    pub schema_version: u32,
    /// RFC 3339 time the scan started.
    pub taken_at: String,
    /// `"4:3"` or `"16:9"`, the screen layout the values were read in.
    pub layout: String,
    pub account: Account,
    pub schedule: Schedule,
    pub missions: Missions,
    pub story: Story,
    pub events: Events,
    pub momotalk: Momotalk,
    pub warnings: Vec<String>,
}

impl ProgressSnapshot {
    pub fn new(taken_at: impl Into<String>, layout: &str) -> Self {
        ProgressSnapshot {
            schema_version: SCHEMA_VERSION,
            taken_at: taken_at.into(),
            layout: layout.to_string(),
            account: Account::default(),
            schedule: Schedule::default(),
            missions: Missions::default(),
            story: Story::default(),
            events: Events::default(),
            momotalk: Momotalk::default(),
            warnings: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Account {
    pub nickname: Option<String>,
    pub level: Option<u32>,
    pub circle: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Schedule {
    pub regions: Vec<Region>,
    pub total_rank: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Region {
    pub name: String,
    pub rank: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Missions {
    pub areas: Vec<MissionArea>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mode {
    Normal,
    Hard,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MissionArea {
    pub area: Option<u32>,
    pub mode: Option<Mode>,
    pub missions: Vec<Mission>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mission {
    /// Stage label such as `19-2`.
    pub label: String,
    /// Lit stars, 0 to 3.
    pub stars: Option<u8>,
    /// At least one star.
    pub cleared: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Story {
    pub main: Vec<StoryVolume>,
    pub mini: Vec<StoryVolume>,
    pub group: Vec<StoryVolume>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StoryVolume {
    pub volume: String,
    pub chapters: Vec<Chapter>,
    pub cleared: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Chapter {
    /// Chapter label such as `4장`.
    pub chapter: String,
    pub title: Option<String>,
    pub cleared: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Events {
    /// The event that is running now, read while it is open. `None` when no event screen was scanned.
    pub open: Option<OpenEvent>,
    /// The always-open past events, in the order the list shows them.
    pub past: Vec<PastEvent>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Period {
    /// Local server time, `YYYY-MM-DD HH:MM`.
    pub start: String,
    pub end: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StageKind {
    Quest,
    Story,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stage {
    pub number: u32,
    pub name: String,
    /// Lit stars of a quest stage; always `None` for a story stage.
    pub stars: Option<u8>,
    pub kind: StageKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpenEvent {
    /// The event's name is a picture; `None` unless the OCR read it. The period is the identity.
    pub name: Option<String>,
    pub period: Period,
    pub stages: Vec<Stage>,
    /// Always true for this record.
    pub scanned_while_open: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PastEvent {
    /// Position in the past-event list, 0 for the oldest.
    pub index: u32,
    pub cleared: Option<bool>,
    /// Always false for this record: the list is read while the event is over.
    pub scanned_while_open: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Momotalk {
    pub students: Vec<MomoStudent>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MomoStudent {
    pub name: String,
    /// Bond rank (the number in the heart).
    pub bond: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unread_values_serialise_as_null() {
        let s = ProgressSnapshot::new("2026-11-24T12:00:00+09:00", "16:9");
        let v = serde_json::to_value(&s).unwrap();
        assert!(v["account"]["nickname"].is_null());
        assert!(v["schedule"]["total_rank"].is_null());
        assert!(v["events"]["open"].is_null());
        assert_eq!(v["layout"], "16:9");
        let back: ProgressSnapshot = serde_json::from_value(v).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn open_and_past_events_are_distinct_records() {
        let mut s = ProgressSnapshot::new("t", "16:9");
        s.events.open = Some(OpenEvent { name: None, period: Period { start: "a".into(), end: "b".into() }, stages: vec![], scanned_while_open: true });
        s.events.past.push(PastEvent { index: 0, cleared: Some(true), scanned_while_open: false });
        let v = serde_json::to_value(&s).unwrap();
        assert_eq!(v["events"]["open"]["scanned_while_open"], true);
        assert_eq!(v["events"]["past"][0]["scanned_while_open"], false);
        assert!(v["events"]["past"][0].get("period").is_none());
    }
}
