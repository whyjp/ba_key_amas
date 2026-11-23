//! Which encounter is the player on, in a raid-like mode?
//!
//! The raid screens name the encounter in different places: the boss list names the boss and
//! its armor, the 방 정보 popup names the difficulty, level and the boss's attack and armor, the
//! 제약해제결전 stage screen names boss, level, attack and armor. The formation screen that follows
//! names none of it (it shows party tabs and slots). `RaidTracker` remembers the encounter from
//! the screens that name it, like `MissionTracker` does for the mission popup.
//!
//! Everything here is raw screen text. Turning the armor and attack words into the database's
//! codes is the wiki's job (`Wiki::decode_labels`), so this crate needs no database.
use crate::scene::{norm, RaidMode, Scene};
use baocr_core::ocr_api::TextBox;
use once_cell::sync::Lazy;
use regex::Regex;

/// The difficulty names as the game writes them, in the order of the database's `idx`.
pub const DIFFICULTIES: [&str; 8] = ["Normal", "Hard", "VeryHard", "Hardcore", "Extreme", "Insane", "Torment", "Lunatic"];
/// The terrain tabs of the mock raid's boss list.
const TERRAIN_WORDS: [&str; 3] = ["시가지", "야외", "실내"];
const ARMOR_LINE: &str = "방어타입";
const ARMOR_WORD: &str = "장갑";
const RECOMMENDED: &str = "추천";
const UNRESTRICTED_NAME: &str = "제약해제결전";
const BEST_RECORD: &str = "최고기록";

/// The position of a difficulty word in the game's order (the database's `idx`), in any letter case.
pub fn difficulty_index(text: &str) -> Option<usize> {
    let n = norm(text).to_lowercase();
    DIFFICULTIES.iter().position(|d| d.to_lowercase() == n)
}

/// What the screens say about the encounter. Every field is optional because no screen says all of them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RaidRef {
    pub mode: RaidMode,
    /// The boss as the screen writes it (OCR text, not matched to anything yet).
    pub boss: String,
    /// Position in `DIFFICULTIES`. The boss list shows every difficulty and selects none: only 방 정보 names one.
    pub difficulty: Option<usize>,
    /// The boss's armor words as shown, one per armor ("특수장갑/경장갑" is two).
    pub armors: Vec<String>,
    /// The boss's attack-type word as shown.
    pub attack: Option<String>,
    /// The enemy level the screen recommends.
    pub level: Option<u32>,
    /// The terrain tabs read on the boss list, left to right. A boss offers two. The highlighted one is a
    /// colour, not text: which of them is chosen is not read.
    pub terrains: Vec<String>,
}

impl RaidRef {
    fn named(mode: RaidMode, boss: &str) -> RaidRef {
        RaidRef { mode, boss: boss.trim().to_string(), difficulty: None, armors: Vec::new(), attack: None, level: None, terrains: Vec::new() }
    }

    /// "모의 총력전 시로&쿠로 Normal".
    pub fn display(&self) -> String {
        match self.difficulty {
            Some(i) => format!("{} {} {}", self.mode.label(), self.boss, DIFFICULTIES[i]),
            None => format!("{} {}", self.mode.label(), self.boss),
        }
    }

    fn same_boss(&self, other: &str) -> bool {
        norm(&self.boss) == norm(other)
    }
}

fn center_x(b: &TextBox) -> f32 {
    (b.x0 + b.x1) / 2.0
}

fn hangul_len(t: &str) -> usize {
    t.chars().filter(|c| ('\u{AC00}'..='\u{D7A3}').contains(c)).count()
}

/// The nearest box under `anchor` (its top within `reach` of the anchor's bottom) and left-aligned with it
/// within `slack`, skipping boxes that are not a name.
fn below<'a>(boxes: &'a [TextBox], anchor: &TextBox, reach: f32, slack: f32) -> Option<&'a TextBox> {
    boxes
        .iter()
        .filter(|b| !std::ptr::eq(*b, anchor) && b.y0 > anchor.y0 + 4.0 && b.y0 - anchor.y1 < reach && (b.x0 - anchor.x0).abs() < slack && norm(&b.text).chars().count() >= 2)
        .min_by(|a, b| a.y0.total_cmp(&b.y0))
}

/// One difficulty row of a boss list: the row's difficulty word and the boss name written under it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DifficultyRow {
    pub index: usize,
    pub boss: String,
}

/// The rows on a 총력전 or 모의 총력전 list. A difficulty word with no boss name under it is not a row
/// (the "Normal" in the best-score strip at the bottom).
pub fn difficulty_rows(boxes: &[TextBox]) -> Vec<DifficultyRow> {
    boxes
        .iter()
        .filter_map(|d| {
            let index = difficulty_index(&d.text)?;
            let boss = below(boxes, d, 40.0, 45.0).filter(|b| difficulty_index(&b.text).is_none() && !norm(&b.text).contains("점수"))?;
            Some(DifficultyRow { index, boss: boss.text.trim().to_string() })
        })
        .collect()
}

/// The armor words of one text ("특수장갑/경장갑").
fn armor_words(text: &str) -> Vec<String> {
    text.split('/').map(|w| w.trim().to_string()).filter(|w| !w.is_empty()).collect()
}

/// The left panel of the 모의 총력전 boss list: "방어타입:<armor>" and the boss name under it.
pub fn mock_boss_from_list(boxes: &[TextBox]) -> Option<RaidRef> {
    let line = boxes.iter().find(|b| norm(&b.text).starts_with(ARMOR_LINE))?;
    let armors = armor_words(norm(&line.text).strip_prefix(ARMOR_LINE)?.trim_start_matches([':', '：']));
    let boss = below(boxes, line, 60.0, 50.0)?;
    let mut tabs: Vec<&TextBox> = boxes.iter().filter(|b| b.y0 > 60.0 && b.y1 < 130.0 && b.x0 > 880.0 && TERRAIN_WORDS.contains(&norm(&b.text).as_str())).collect();
    tabs.sort_by(|a, b| a.x0.total_cmp(&b.x0));
    let terrains = tabs.iter().map(|b| norm(&b.text)).collect();
    Some(RaidRef { armors, terrains, ..RaidRef::named(RaidMode::MockTotalAssault, &boss.text) })
}

/// The boss of the 총력전 list: the same boss on every row.
pub fn boss_from_raid_select(boxes: &[TextBox]) -> Option<RaidRef> {
    let first = difficulty_rows(boxes).into_iter().min_by_key(|r| r.index)?;
    Some(RaidRef::named(RaidMode::TotalAssault, &first.boss))
}

/// The last run of digits in an OCR'd level ("Lv.25", "L4.17" for "Lv.17", "Lv,68").
fn level_of(text: &str) -> Option<u32> {
    static DIGITS: Lazy<Regex> = Lazy::new(|| Regex::new(r"\d+").unwrap());
    DIGITS.find_iter(text).last()?.as_str().parse().ok()
}

/// The recommended level: the "Lv" box beside the word 추천.
fn recommended_level(boxes: &[TextBox]) -> Option<u32> {
    let anchor = boxes.iter().find(|b| norm(&b.text) == RECOMMENDED)?;
    boxes
        .iter()
        .filter(|b| !std::ptr::eq(*b, anchor) && b.y0 >= anchor.y0 - 5.0 && b.y0 < anchor.y1 + 60.0 && (center_x(b) - center_x(anchor)).abs() < 80.0 && b.text.trim_start().starts_with(['L', 'l']))
        .find_map(|b| level_of(&b.text))
}

/// The attack and armor words of the pills in the row of an armor word: the armor box holds "장갑", the
/// attack word is the nearest word of two or more letters to its left in the same row.
fn pills(boxes: &[TextBox], inside: impl Fn(&TextBox) -> bool) -> (Option<String>, Vec<String>) {
    let Some(armor) = boxes.iter().filter(|b| inside(b) && norm(&b.text).contains(ARMOR_WORD)).max_by(|a, b| a.x0.total_cmp(&b.x0)) else { return (None, Vec::new()) };
    let attack = boxes
        .iter()
        .filter(|b| inside(b) && !std::ptr::eq(*b, armor) && (b.y0 - armor.y0).abs() < 12.0 && b.x1 <= armor.x0 + 5.0 && hangul_len(&b.text) >= 2 && norm(&b.text).chars().all(|c| ('\u{AC00}'..='\u{D7A3}').contains(&c)))
        .max_by(|a, b| a.x1.total_cmp(&b.x1))
        .map(|b| norm(&b.text));
    (attack, armor_words(&norm(&armor.text)))
}

/// What the "방 정보" popup of a mock-raid room names: difficulty, boss, level and the boss's attack and armor.
pub fn room_from_info(boxes: &[TextBox]) -> Option<RaidRef> {
    let in_popup = |b: &TextBox| b.x0 > 130.0 && b.x1 < 1130.0 && b.y0 > 290.0 && b.y1 < 470.0;
    let difficulty = boxes.iter().filter(|b| in_popup(b) && b.x0 < 330.0).find_map(|b| difficulty_index(&b.text).map(|i| (i, b)))?;
    let boss = below(boxes, difficulty.1, 40.0, 40.0)?;
    let (attack, armors) = pills(boxes, in_popup);
    Some(RaidRef { difficulty: Some(difficulty.0), armors, attack, level: recommended_level(boxes), ..RaidRef::named(RaidMode::MockTotalAssault, &boss.text) })
}

/// The boss named under the small "제약해제결전" label on the stage screen and on the select screen's
/// "최고 기록" line.
fn unrestricted_boss(boxes: &[TextBox], stage: bool) -> Option<&TextBox> {
    if stage {
        let label = boxes.iter().find(|b| b.y0 > 200.0 && norm(&b.text) == UNRESTRICTED_NAME)?;
        return boxes.iter().filter(|b| !std::ptr::eq(*b, label) && b.y0 > label.y0 + 4.0 && b.y0 - label.y1 < 40.0 && (center_x(b) - center_x(label)).abs() < 200.0 && hangul_len(&b.text) >= 2).min_by(|a, b| a.y0.total_cmp(&b.y0));
    }
    let record = boxes.iter().find(|b| norm(&b.text).contains(BEST_RECORD))?;
    boxes.iter().filter(|b| b.y1 <= record.y0 + 10.0 && record.y0 - b.y1 < 40.0 && (b.x0 - record.x0).abs() < 40.0 && hangul_len(&b.text) >= 2).max_by(|a, b| a.y1.total_cmp(&b.y1))
}

/// The boss alone, from the 제약해제결전 select screen.
pub fn boss_from_unrestricted_select(boxes: &[TextBox]) -> Option<RaidRef> {
    unrestricted_boss(boxes, false).map(|b| RaidRef::named(RaidMode::UnrestrictedRaid, &b.text))
}

/// Boss, level, attack and armor of the 제약해제결전 stage screen. The terrain is an icon, and the
/// stage chosen among 01, 02, ... is a highlight: neither is text, so neither is read.
pub fn unrestricted_stage(boxes: &[TextBox]) -> Option<RaidRef> {
    let boss = unrestricted_boss(boxes, true)?;
    let (attack, armors) = pills(boxes, |b| b.x0 > 800.0 && b.y0 > 380.0 && b.y1 < 460.0);
    Some(RaidRef { armors, attack, level: recommended_level(boxes), ..RaidRef::named(RaidMode::UnrestrictedRaid, &boss.text) })
}

/// Remembers the raid encounter across the screens that do not name it.
#[derive(Debug, Default)]
pub struct RaidTracker {
    raid: Option<RaidRef>,
}

impl RaidTracker {
    pub fn raid(&self) -> Option<&RaidRef> {
        self.raid.as_ref()
    }

    /// Feed the screen just recognised (with its OCR text) and get the encounter now in play.
    pub fn update(&mut self, scene: Scene, boxes: &[TextBox]) -> Option<&RaidRef> {
        match scene {
            Scene::RaidSelect => self.set(boss_from_raid_select(boxes)),
            Scene::MockRaidBosses => self.set(mock_boss_from_list(boxes)),
            Scene::UnrestrictedRaidSelect => self.set(boss_from_unrestricted_select(boxes)),
            Scene::UnrestrictedRaidStage => self.set(unrestricted_stage(boxes)),
            // the popup opens over the boss list: it adds what the list does not show to the same boss
            Scene::RaidRoomInfo => {
                if let Some(room) = room_from_info(boxes) {
                    let merged = match self.raid.take() {
                        Some(list) if list.mode == room.mode && list.same_boss(&room.boss) => RaidRef { terrains: list.terrains, ..room },
                        _ => room,
                    };
                    self.raid = Some(merged);
                }
            }
            // a formation of another mode than the remembered encounter is not that encounter's
            Scene::RaidFormation(mode) => {
                if self.raid.as_ref().is_some_and(|r| r.mode != mode) {
                    self.raid = None;
                }
            }
            // the same boss list, other bosses' lists and the popups of the screens above keep what is known
            Scene::UnrestrictedRaidStages | Scene::QuickFormation | Scene::Battle | Scene::Unknown => {}
            // anywhere else the player is no longer in a raid encounter (a campaign mission, the menu, another mode)
            Scene::Lobby
            | Scene::LobbyIdle
            | Scene::WorkHub
            | Scene::MissionList
            | Scene::DungeonSelect
            | Scene::DungeonList
            | Scene::MissionInfo
            | Scene::EnemyInfo
            | Scene::MissionMap
            | Scene::Squad
            | Scene::StudentList
            | Scene::StudentProfile
            | Scene::TimeAttackSelect
            | Scene::TimeAttackNextSeason
            | Scene::EliminateSelect
            | Scene::EliminateClosed
            | Scene::SpoilerWarning => self.raid = None,
        }
        self.raid.as_ref()
    }

    /// A list or stage screen names the boss afresh; one whose boss could not be read keeps what was known.
    fn set(&mut self, read: Option<RaidRef>) {
        if read.is_some() {
            self.raid = read;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::classify;

    fn boxes(name: &str) -> Vec<TextBox> {
        let path = format!("{}/../../../fixtures/screens/{name}.boxes.txt", env!("CARGO_MANIFEST_DIR"));
        std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{path}: {e}"))
            .lines()
            .filter_map(|l| {
                let (pos, text) = l.split_once('\t')?;
                let v: Vec<f32> = pos.split(',').filter_map(|n| n.parse().ok()).collect();
                (v.len() == 4).then(|| TextBox { x0: v[0], y0: v[1], x1: v[2], y1: v[3], text: text.trim_end_matches('\r').to_string(), score: 1.0 })
            })
            .collect()
    }

    /// The boss and armor the 14 bosses of the mock list show, in the order the list holds them
    /// (the fifteenth screen is the first again). `raid-mock-m00..m14`.
    const MOCK_BOSSES: [(&str, &[&str]); 15] = [
        ("시로&쿠로", &["특수장갑"]),
        ("예로니무스", &["경장갑"]),
        ("KAITEN FX Mk.0", &["경장갑"]),
        ("페로로지라", &["특수장갑"]),
        ("호드", &["중장갑"]),
        ("고즈", &["특수장갑"]),
        ("그레고리오", &["경장갑"]),
        ("호버크래프트", &["중장갑"]),
        ("쿠로카게", &["탄력장갑"]),
        ("게부라", &["경장갑"]),
        ("예소드", &["탄력장갑"]),
        ("드럼통게", &["특수장갑", "경장갑"]),
        ("비나", &["중장갑"]),
        ("헤세드", &["중장갑"]),
        ("시로&쿠로", &["특수장갑"]),
    ];

    #[test]
    fn every_boss_on_the_mock_list_is_read_with_its_armor_and_the_dual_armor_stays_two() {
        for (i, (boss, armors)) in MOCK_BOSSES.iter().enumerate() {
            let r = mock_boss_from_list(&boxes(&format!("raid-mock-m{i:02}"))).unwrap_or_else(|| panic!("m{i:02}"));
            assert_eq!((r.boss.as_str(), r.armors.iter().map(String::as_str).collect::<Vec<_>>()), (*boss, armors.to_vec()), "m{i:02}");
            assert_eq!((r.mode, r.difficulty), (RaidMode::MockTotalAssault, None), "the list selects no difficulty");
        }
    }

    #[test]
    fn the_two_terrain_tabs_of_a_boss_are_listed_left_to_right_without_saying_which_is_chosen() {
        let terrains = |f: &str| mock_boss_from_list(&boxes(f)).unwrap().terrains;
        assert_eq!(terrains("raid-mock-m00"), ["시가지", "실내"], "the house icon between them was read as a stray mark");
        assert_eq!(terrains("raid-mock-m01"), ["실내", "시가지"]);
        assert_eq!(terrains("raid-mock-m12"), ["야외", "시가지"]);
    }

    #[test]
    fn difficulty_rows_are_numbered_in_the_games_order_and_the_best_score_strip_is_no_row() {
        let rows = difficulty_rows(&boxes("raid-mock-m00"));
        assert_eq!(rows.iter().map(|r| r.index).collect::<Vec<_>>(), (0..=6).collect::<Vec<_>>(), "Normal .. Torment");
        assert!(rows.iter().all(|r| r.boss == "시로&쿠로"), "{rows:?}");
        assert_eq!(difficulty_index("Hardcore"), Some(3), "the game's casing differs from the database's HardCore");
        assert_eq!(difficulty_index("VERYHARD"), Some(2));
        assert_eq!(difficulty_index("Lunatic"), Some(7));
        assert_eq!(difficulty_index("Normally"), None);
        let select = difficulty_rows(&boxes("raid-select"));
        assert_eq!(select.iter().map(|r| r.index).collect::<Vec<_>>(), vec![0, 1, 2, 3, 4], "the locked rows' notes are not rows");
        assert_eq!(boss_from_raid_select(&boxes("raid-select")).unwrap().boss, "예로니무스");
    }

    #[test]
    fn the_room_popup_names_difficulty_boss_level_attack_and_armor() {
        let r = room_from_info(&boxes("raid-mock-room-info")).unwrap();
        assert_eq!((r.mode, r.boss.as_str(), r.difficulty, r.level), (RaidMode::MockTotalAssault, "시로&쿠로", Some(0), Some(17)));
        assert_eq!((r.attack.as_deref(), r.armors.as_slice()), (Some("일반"), ["특수장갑".to_string()].as_slice()));
    }

    #[test]
    fn the_stage_screen_names_boss_level_attack_and_armor_and_the_select_screen_the_boss() {
        let r = unrestricted_stage(&boxes("unrestricted-raid-stage01")).unwrap();
        assert_eq!((r.mode, r.boss.as_str(), r.level, r.attack.as_deref()), (RaidMode::UnrestrictedRaid, "세트의분노", Some(25), Some("일반")));
        assert_eq!(r.armors, ["경장갑"]);
        assert_eq!(boss_from_unrestricted_select(&boxes("unrestricted-raid-select")).unwrap().boss, "세트의분노");
    }

    #[test]
    fn the_formation_knows_the_encounter_the_list_and_popup_named() {
        let mut t = RaidTracker::default();
        let feed = |t: &mut RaidTracker, file: &str| {
            let b = boxes(file);
            let scene = classify(&b, None);
            t.update(scene, &b).cloned()
        };
        let list = feed(&mut t, "raid-mock-m00").unwrap();
        assert_eq!((list.boss.as_str(), list.difficulty, list.level), ("시로&쿠로", None, None));
        let room = feed(&mut t, "raid-mock-room-info").unwrap();
        assert_eq!((room.difficulty, room.level, room.attack.as_deref()), (Some(0), Some(17), Some("일반")));
        assert_eq!(room.terrains, list.terrains, "the popup keeps the tabs of the list under it");
        let formation = feed(&mut t, "raid-formation-empty").unwrap();
        assert_eq!(formation, room, "the formation names nothing and keeps the room");
        assert_eq!(t.update(Scene::QuickFormation, &boxes("quick-formation")).unwrap(), &room);
        // another boss on the list replaces it, and a campaign screen or the menu forgets it
        assert_eq!(feed(&mut t, "raid-mock-m01").unwrap().boss, "예로니무스");
        assert!(feed(&mut t, "mission-info").is_none());
        assert!(feed(&mut t, "raid-mock-m01").is_some() && feed(&mut t, "work-hub-full").is_none());
    }

    #[test]
    fn a_formation_of_another_mode_does_not_inherit_the_encounter() {
        let mut t = RaidTracker::default();
        t.update(Scene::UnrestrictedRaidStage, &boxes("unrestricted-raid-stage01"));
        assert!(t.update(Scene::RaidFormation(RaidMode::UnrestrictedRaid), &boxes("unrestricted-raid-formation")).is_some());
        assert!(t.update(Scene::RaidFormation(RaidMode::MockTotalAssault), &boxes("raid-formation-empty")).is_none());
    }

    #[test]
    fn a_room_popup_over_another_boss_replaces_the_list_boss() {
        let mut t = RaidTracker::default();
        t.update(Scene::MockRaidBosses, &boxes("raid-mock-m01"));
        let r = t.update(Scene::RaidRoomInfo, &boxes("raid-mock-room-info")).unwrap();
        assert_eq!(r.boss, "시로&쿠로");
        assert!(r.terrains.is_empty());
    }
}
