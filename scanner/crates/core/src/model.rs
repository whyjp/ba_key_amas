//! The schema contract. Field names and shapes match the Go program's JSON
//! (`student.json`, `students.json`, `index.json`) so existing outputs load
//! unchanged. `Option<T>` is `*T` in Go: None = "not proven by pixels".
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub type Int = i32;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Stats {
    pub max_hp: Option<Int>,
    pub attack: Option<Int>,
    pub defense: Option<Int>,
    pub healing: Option<Int>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Exp {
    pub current: Option<Int>,
    pub required: Option<Int>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Loadout {
    pub tactic_role: String,
    pub position: String,
    pub attack_type: String,
    pub armor_type: String,
    pub weapon_class: String,
}

/// 4=S .. 0=D
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Terrain {
    pub urban: Option<Int>,
    pub outdoor: Option<Int>,
    pub indoor: Option<Int>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Badges {
    pub basic_info_tab: Option<bool>,
    pub level_up_tab: Option<bool>,
    pub mystic_release_tab: Option<bool>,
    pub skill_growth: Option<bool>,
    pub weapon_growth: Option<bool>,
    pub equipment_growth: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Locks {
    pub ability_unlock_locked: Option<bool>,
    pub simsang_bloom_locked: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SkillLocked {
    pub ex: bool,
    pub basic: bool,
    pub passive: bool,
    pub sub: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct NameParts {
    pub base_name: String,
    pub outfit: String,
    pub outfit_key: String,
    pub is_outfit_variant: bool,
    pub is_distinct_character: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub distinct_suffix: String,
}

/// status: exact | corrected | ambiguous | unlisted | empty
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct NameMatch {
    pub status: String,
    pub cleaned: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub candidate: String,
    pub distance: Int,
    pub margin: Int,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Reference {
    pub source: String,
    pub id: Int,
    pub school: String,
    pub club: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub school_year: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub age: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub birthday: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub height: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub full_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub hobby: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub designer: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub illustrator: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub voice: String,
    pub base_star_grade: Int,
    pub weapon_type: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub ambiguous_name: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FixedMeta {
    /// "dictionary" or "pixels+dictionary"
    pub source: String,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub dictionary_id: Int,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub ambiguous: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub equipment_slots: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub potential_material: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skill_materials: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub weapon_terrain: String,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub weapon_terrain_bonus: Int,
    pub terrain_base: Terrain,
}

fn is_zero(v: &Int) -> bool {
    *v == 0
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Equipment {
    pub slot: Int,
    pub slot_kind: String,
    pub equipped: Option<bool>,
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub item_name: Option<String>,
    pub tier: Option<Int>,
    pub level: Option<Int>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_match_error: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct StatDetail {
    pub main: Stats,
    pub accuracy: Option<Int>,
    pub evasion: Option<Int>,
    pub critical: Option<Int>,
    pub critical_damage_pct: Option<Int>,
    pub defense_pierce: Option<Int>,
    pub stability: Option<Int>,
    pub range: Option<Int>,
    pub cost_recovery: Option<Int>,
    pub cc_power: Option<Int>,
    pub cc_resist: Option<Int>,
    pub type_bonus_kind: String,
    pub type_bonus_pct: Option<Int>,
    pub max_cost_bonus: Option<Int>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Mystic {
    pub stars: Option<Int>,
    pub next_stars: Option<Int>,
    pub unlock_skill_text: String,
    pub max_bond_rank_text: String,
    pub max_bond_rank: Option<Int>,
    pub next_max_bond_rank: Option<Int>,
    pub next_max_hp: Option<Int>,
    pub next_attack: Option<Int>,
    pub next_healing: Option<Int>,
    pub status_text: String,
    pub max_growth_reached: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Deep {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stat_detail: Option<StatDetail>,
    /// 레벨 업 tab: growth stats without gear/bond.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_stats: Option<Stats>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mystic: Option<Mystic>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Student {
    pub name: String,
    pub level: Option<Int>,
    pub rarity_stars: Option<Int>,
    pub ex_skill_level: Option<Int>,
    pub normal_skill_level: Option<Int>,
    pub passive_skill_level: Option<Int>,
    pub sub_skill_level: Option<Int>,
    pub bond_rank: Option<Int>,
    pub unique_weapon_stars: Option<Int>,
    pub unique_weapon_level: Option<Int>,
    pub combat_role: String,
    pub is_pinned: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name_match: Option<NameMatch>,
    pub identity: NameParts,
    pub stats: Stats,
    pub exp: Exp,
    pub loadout: Loadout,
    pub terrain: Terrain,
    pub badges: Badges,
    pub locks: Locks,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference: Option<Reference>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deep: Option<Deep>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fixed_meta: Option<FixedMeta>,
    pub skill_locked: SkillLocked,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    #[serde(default)]
    pub equipment: Vec<Equipment>,
    pub source: String,
    #[serde(default)]
    pub raw_ocr: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Account {
    pub ap: String,
    pub credits: Option<Int>,
    pub pyroxene: Option<Int>,
    pub raw_credits: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Capture {
    pub aspect_ratio: String,
    pub frame_region: [f64; 4],
    pub window_mode: String,
    pub note: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RosterEntry {
    pub id: String,
    pub page: Int,
    pub column: Int,
    pub status: String,
    pub student: Student,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub detail_path: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RosterReport {
    pub schema_version: Int,
    pub complete: bool,
    pub stop_reason: String,
    pub pages_scanned: Int,
    pub entries: Vec<RosterEntry>,
    pub unresolved: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account: Option<Account>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capture: Option<Capture>,
}

/// One student in out/index.json: captured -> extracted | failed.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct IndexEntry {
    pub id: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    pub attempts: Int,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub error: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub frames: Vec<String>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub warnings: Int,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A student.json written by the Go program must load and survive a round trip.
    #[test]
    fn loads_go_student_json() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../fixtures/go-output/details/001/student.json");
        let text = std::fs::read_to_string(dir).expect("fixture student.json");
        let s: Student = serde_json::from_str(&text).expect("Go student.json must deserialize");
        assert!(!s.name.is_empty());
        let again: Student = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(s, again);
    }
}
