//! Which game screen is this, and which mission is the player on?
//!
//! Screens are told apart by the text OCR reads on them (header title and a few
//! anchor words), never by position alone, so the same code works at any window
//! size. The mission screens each show different things: only the "임무 정보"
//! popup (and the enemy popup) name the mission, while the hex map and the squad
//! formation that follow show no name. `MissionTracker` remembers the mission
//! from the popup so the later screens know it.
use baocr_core::ocr_api::TextBox;
use once_cell::sync::Lazy;
use regex::Regex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scene {
    /// Main menu.
    Lobby,
    /// The main menu after a while without input: the game hides every menu, art only. A click brings them back.
    LobbyIdle,
    /// 업무: the hub that leads to 임무, 현상수배, 총력전, ...
    WorkHub,
    /// 임무: the list of one area's missions (Normal / Hard tabs).
    MissionList,
    /// 현상수배 / 특별의뢰 / 학원교류회: choosing the place (고가도로, 거점 방어, 트리니티, ...).
    DungeonSelect,
    /// The stage list of one place ("스테이지 목록").
    DungeonList,
    /// "임무 정보" popup of one mission.
    MissionInfo,
    /// "적 정보 / 상성 정보" popup of one mission.
    EnemyInfo,
    /// The hex map shown after 임무 시작 (header "임무", no mission name).
    MissionMap,
    /// 편성: squad formation.
    Squad,
    /// The 빠른 편성 popup: the grid of students to put into the squad.
    QuickFormation,
    /// Student list and student profile, told apart by pixels, not text.
    StudentList,
    StudentProfile,
    /// The live combat screen, told apart by pixels (see `baocr_pixels::looks_like_battle`).
    Battle,
    Unknown,
}

impl Scene {
    /// Name shown to the player.
    pub fn label(self) -> &'static str {
        match self {
            Scene::Lobby => "메인 메뉴",
            Scene::LobbyIdle => "메인 메뉴 (대기)",
            Scene::WorkHub => "업무",
            Scene::MissionList => "임무 선택",
            Scene::DungeonSelect => "의뢰 선택",
            Scene::DungeonList => "스테이지 목록",
            Scene::MissionInfo => "임무 정보",
            Scene::EnemyInfo => "적 정보",
            Scene::MissionMap => "임무 지도",
            Scene::Squad => "편성",
            Scene::QuickFormation => "빠른 편성",
            Scene::StudentList => "학생 목록",
            Scene::StudentProfile => "학생 정보",
            Scene::Battle => "전투 중",
            Scene::Unknown => "",
        }
    }
}

/// OCR text without spaces and the stray marks it adds ("임무?" for a header with a help icon).
fn norm(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace() && !"?？·•|｜".contains(*c)).collect()
}

fn has(boxes: &[TextBox], word: &str) -> bool {
    boxes.iter().any(|b| norm(&b.text).contains(word))
}

/// The header title at the top left (right of the back arrow).
fn header(boxes: &[TextBox]) -> String {
    boxes.iter().filter(|b| b.y1 < 60.0 && b.x0 > 80.0 && b.x0 < 300.0).map(|b| norm(&b.text)).next().unwrap_or_default()
}

/// A popup's title: centred near the top of the popup, around y 200..260.
fn popup_title_is(boxes: &[TextBox], word: &str) -> bool {
    boxes.iter().any(|b| norm(&b.text) == word && b.y0 > 170.0 && b.y1 < 300.0 && (b.x0 + b.x1) / 2.0 > 450.0 && (b.x0 + b.x1) / 2.0 < 830.0)
}

/// Headers of the screens that hold place-based missions.
const DUNGEON_HEADERS: [&str; 3] = ["현상수배", "특별의뢰", "학원교류회"];

/// The main menu's bottom bar names at least four of its eight places.
const LOBBY_LABELS: [&str; 8] = ["카페", "스케줄", "학생", "편성", "소설", "제조", "상점", "모집"];

pub fn is_lobby(boxes: &[TextBox]) -> bool {
    let found = LOBBY_LABELS.iter().filter(|l| boxes.iter().any(|b| b.y0 > 880.0 && b.y1 < 958.0 && b.text.trim() == **l)).count();
    found >= 4
}

/// Classify a frame from its OCR text. Student list / profile are decided by the
/// caller from pixels and passed in `pixel_scene`.
pub fn classify(boxes: &[TextBox], pixel_scene: Option<Scene>) -> Scene {
    if let Some(s) = pixel_scene {
        return s;
    }
    if has(boxes, "적정보") && has(boxes, "상성정보") {
        return Scene::EnemyInfo;
    }
    if popup_title_is(boxes, "빠른편성") {
        return Scene::QuickFormation;
    }
    if popup_title_is(boxes, "임무정보") {
        return Scene::MissionInfo;
    }
    match header(boxes).as_str() {
        "임무" if has(boxes, "투입부대") || has(boxes, "임무개시") => Scene::MissionMap,
        "임무" if has(boxes, "Normal") && has(boxes, "Hard") => Scene::MissionList,
        "편성" => Scene::Squad,
        "업무" => Scene::WorkHub,
        h if DUNGEON_HEADERS.contains(&h) && has(boxes, "스테이지목록") => Scene::DungeonList,
        h if DUNGEON_HEADERS.contains(&h) && has(boxes, "Select") => Scene::DungeonSelect,
        _ if is_lobby(boxes) => Scene::Lobby,
        _ => Scene::Unknown,
    }
}

/// A mission as the game shows it. Campaign missions carry an area and stage number
/// ("19-3 | 설원 지구 제2 지역"). 현상수배, 특별의뢰 and 학원교류회 missions show only a
/// stage number and a stage name ("03 | 고가도로 C"); the place they belong to
/// (고가도로, 거점 방어, 트리니티, ...) was shown on the list screen before.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissionRef {
    /// Campaign area and stage number.
    pub number: Option<(u32, u32)>,
    /// Stage number within a place (현상수배, 특별의뢰, 학원교류회).
    pub stage_no: Option<u32>,
    /// The place of such a mission, from the list screen, when it was seen.
    pub place: Option<String>,
    pub name: String,
}

impl MissionRef {
    pub fn named(name: &str) -> MissionRef {
        MissionRef { number: None, stage_no: None, place: None, name: name.to_string() }
    }

    /// "19-3 설원 지구 제2 지역", "고가도로 03 고가도로 C", or just the name.
    pub fn display(&self) -> String {
        match (self.number, self.stage_no, &self.place) {
            (Some((a, s)), _, _) => format!("{a}-{s} {}", self.name),
            (None, Some(n), Some(place)) => format!("{place} {n:02} {}", self.name),
            (None, Some(n), None) => format!("{n:02} {}", self.name),
            _ => self.name.clone(),
        }
    }

    fn same_stage(&self, other: &MissionRef) -> bool {
        self.number == other.number && self.stage_no == other.stage_no
    }
}

static NUMBERED: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\s*(\d{1,2})\s*[-–—]\s*(\d{1,2})\s*[|｜lI]?\s*(.+?)\s*$").unwrap());
static STAGE_LINE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\s*(\d{1,2})\s*[|｜lI]?\s*(\D.+?)\s*$").unwrap());

/// The mission named on the "임무 정보" popup (its title line, around y 300..410).
pub fn mission_from_info(boxes: &[TextBox]) -> Option<MissionRef> {
    boxes.iter().filter(|b| b.y0 > 300.0 && b.y1 < 410.0 && b.x0 < 560.0).find_map(|b| {
        if let Some(c) = NUMBERED.captures(&b.text) {
            let name = c[3].trim().to_string();
            let number = (c[1].parse().ok()?, c[2].parse().ok()?);
            return (name.chars().count() >= 2).then_some(MissionRef { number: Some(number), stage_no: None, place: None, name });
        }
        let c = STAGE_LINE.captures(&b.text)?;
        let name = c[2].trim().to_string();
        (name.chars().count() >= 2).then(|| MissionRef { number: None, stage_no: c[1].parse().ok(), place: None, name })
    })
}

/// The mission named on the enemy popup (its title, around y 200..300).
pub fn mission_from_enemy_popup(boxes: &[TextBox]) -> Option<MissionRef> {
    boxes
        .iter()
        .filter(|b| b.y0 > 190.0 && b.y1 < 300.0 && (b.x0 + b.x1) / 2.0 > 450.0 && (b.x0 + b.x1) / 2.0 < 830.0)
        .map(|b| b.text.trim().to_string())
        .find(|t| t.chars().count() >= 2 && !t.contains('×'))
        .map(|name| MissionRef::named(&name))
}

/// The place named on a stage list's left card ("현상수배 - 고가도로", "거점 방어", "트리니티").
pub fn place_from_list(boxes: &[TextBox]) -> Option<String> {
    let mut card: Vec<&TextBox> = boxes.iter().filter(|b| b.x0 < 400.0 && b.y0 > 240.0 && b.y0 < 300.0).collect();
    card.sort_by(|a, b| a.y0.total_cmp(&b.y0));
    card.first().map(|b| b.text.trim().to_string()).filter(|t| t.chars().count() >= 2)
}

/// The students named on the 편성 screen: the four strikers' labels and the specials' labels
/// (as OCR read them, to be matched to students by the caller), each with whether it was
/// read in the specials' row.
pub fn squad_names(boxes: &[TextBox]) -> Vec<(String, bool)> {
    let has_hangul = |t: &str| t.chars().any(|c| ('\u{AC00}'..='\u{D7A3}').contains(&c));
    boxes
        .iter()
        .filter(|b| {
            let strikers = b.y0 >= 690.0 && b.y0 <= 745.0 && b.x0 >= 200.0 && b.x1 <= 1100.0;
            let specials = b.y0 >= 880.0 && b.y0 <= 935.0 && b.x0 >= 470.0 && b.x1 <= 1000.0;
            (strikers || specials) && has_hangul(&b.text)
        })
        .map(|b| (b.text.trim().to_string(), b.y0 >= 880.0))
        .collect()
}

/// Remembers the mission across the screens that do not name it.
#[derive(Debug, Default)]
pub struct MissionTracker {
    mission: Option<MissionRef>,
    /// The place read on the last stage list (for missions that do not name it).
    place: Option<String>,
}

impl MissionTracker {
    pub fn mission(&self) -> Option<&MissionRef> {
        self.mission.as_ref()
    }

    /// Feed the screen just recognised (with its OCR text) and get the mission now in play.
    pub fn update(&mut self, scene: Scene, boxes: &[TextBox]) -> Option<&MissionRef> {
        match scene {
            Scene::MissionInfo => {
                if let Some(mut m) = mission_from_info(boxes) {
                    if m.stage_no.is_some() {
                        m.place = self.place.clone();
                    }
                    // An alert dialog over the popup can hide the end of the name: the same
                    // mission seen again with a shorter name keeps the full name already read.
                    let cut_off_view = self.mission.as_ref().is_some_and(|cur| cur.same_stage(&m) && cur.name.chars().count() > m.name.chars().count());
                    if !cut_off_view {
                        self.mission = Some(m);
                    }
                }
            }
            // the enemy popup opens on top of the info popup: it can only add a name, never replace a numbered mission
            Scene::EnemyInfo => {
                if let Some(m) = mission_from_enemy_popup(boxes) {
                    let same = self.mission.as_ref().is_some_and(|cur| norm(&cur.name) == norm(&m.name));
                    if !same {
                        self.mission = Some(m);
                    }
                }
            }
            // the stage list names the place its stages belong to
            Scene::DungeonList => {
                self.mission = None;
                self.place = place_from_list(boxes).or_else(|| self.place.take());
            }
            // back at a place from which another mission is picked (or none): forget
            Scene::DungeonSelect | Scene::MissionList | Scene::WorkHub | Scene::Lobby | Scene::LobbyIdle | Scene::StudentList | Scene::StudentProfile => {
                self.mission = None;
                self.place = None;
            }
            Scene::MissionMap | Scene::Squad | Scene::QuickFormation | Scene::Battle | Scene::Unknown => {}
        }
        self.mission.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn boxes(name: &str) -> Vec<TextBox> {
        let path = format!("{}/../../../fixtures/screens/{name}.boxes.txt", env!("CARGO_MANIFEST_DIR"));
        std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{path}: {e}"))
            .lines()
            .filter_map(|l| {
                let (pos, text) = l.split_once('\t')?;
                let v: Vec<f32> = pos.split(',').filter_map(|n| n.parse().ok()).collect();
                (v.len() == 4).then(|| TextBox { x0: v[0], y0: v[1], x1: v[2], y1: v[3], text: text.to_string(), score: 1.0 })
            })
            .collect()
    }

    #[test]
    fn every_recorded_screen_is_recognised() {
        for (file, want) in [
            ("lobby", Scene::Lobby),
            ("work-hub", Scene::WorkHub),
            ("mission-list", Scene::MissionList),
            ("mission-info", Scene::MissionInfo),
            ("enemy-info", Scene::EnemyInfo),
            ("mission-map", Scene::MissionMap),
            ("squad", Scene::Squad),
            ("quick-formation", Scene::QuickFormation),
            ("dungeon-select-bounty", Scene::DungeonSelect),
            ("dungeon-select-week", Scene::DungeonSelect),
            ("dungeon-select-school", Scene::DungeonSelect),
            ("dungeon-list-bounty", Scene::DungeonList),
            ("dungeon-list-week", Scene::DungeonList),
            ("dungeon-list-school", Scene::DungeonList),
            ("dungeon-info-bounty", Scene::MissionInfo),
            ("dungeon-info-week", Scene::MissionInfo),
        ] {
            assert_eq!(classify(&boxes(file), None), want, "{file}");
        }
    }

    #[test]
    fn pixel_scenes_win_over_text() {
        assert_eq!(classify(&boxes("lobby"), Some(Scene::StudentList)), Scene::StudentList);
    }

    #[test]
    fn the_mission_is_read_from_the_info_popup_and_the_enemy_popup() {
        let m = mission_from_info(&boxes("mission-info")).unwrap();
        assert_eq!(m, MissionRef { number: Some((19, 3)), stage_no: None, place: None, name: "설원 지구 제2 지역".into() });
        assert_eq!(m.display(), "19-3 설원 지구 제2 지역");
        let e = mission_from_enemy_popup(&boxes("enemy-info")).unwrap();
        assert_eq!(e.name, "설원 지구 제2 지역");
    }

    #[test]
    fn the_mission_survives_the_screens_that_do_not_name_it_and_is_forgotten_on_leaving() {
        let mut t = MissionTracker::default();
        assert!(t.update(Scene::MissionList, &boxes("mission-list")).is_none());
        assert_eq!(t.update(Scene::MissionInfo, &boxes("mission-info")).unwrap().number, Some((19, 3)));
        // the enemy popup repeats the name without a number: the numbered mission stays
        assert_eq!(t.update(Scene::EnemyInfo, &boxes("enemy-info")).unwrap().number, Some((19, 3)));
        assert_eq!(t.update(Scene::MissionMap, &boxes("mission-map")).unwrap().name, "설원 지구 제2 지역");
        assert_eq!(t.update(Scene::Squad, &boxes("squad")).unwrap().name, "설원 지구 제2 지역");
        assert!(t.update(Scene::Unknown, &[]).is_some(), "an unrecognised frame (a transition) keeps the memory");
        assert!(t.update(Scene::MissionList, &boxes("mission-list")).is_none());
    }

    #[test]
    fn a_name_cut_off_by_an_alert_dialog_does_not_replace_the_full_name() {
        let mut t = MissionTracker::default();
        t.update(Scene::MissionInfo, &boxes("mission-info"));
        let mut hidden = boxes("mission-info");
        for b in hidden.iter_mut().filter(|b| b.text.starts_with("19-3")) {
            b.text = "19-3|설원 지구".into();
        }
        assert_eq!(t.update(Scene::MissionInfo, &hidden).unwrap().name, "설원 지구 제2 지역");
        // another mission replaces it
        for b in hidden.iter_mut().filter(|b| b.text.starts_with("19-3")) {
            b.text = "19-4|호쿠로쿠 중앙선".into();
        }
        assert_eq!(t.update(Scene::MissionInfo, &hidden).unwrap().number, Some((19, 4)));
    }

    #[test]
    fn a_place_based_mission_takes_its_place_from_the_list_screen() {
        for (list, info, place, stage, name) in [
            ("dungeon-list-bounty", "dungeon-info-bounty", "현상수배-고가도로", 3, "고가도로 C"),
            ("dungeon-list-week", "dungeon-info-week", "거점방어", 7, "폐허의 군수공장G"),
        ] {
            let mut t = MissionTracker::default();
            t.update(Scene::DungeonSelect, &boxes("dungeon-select-bounty"));
            t.update(Scene::DungeonList, &boxes(list));
            let m = t.update(Scene::MissionInfo, &boxes(info)).unwrap();
            assert_eq!((m.place.as_deref(), m.stage_no, m.number), (Some(place), Some(stage), None), "{info}");
            assert_eq!(m.name.replace(' ', ""), name.replace(' ', ""));
            // leaving for the place selection forgets both
            assert!(t.update(Scene::DungeonSelect, &boxes("dungeon-select-bounty")).is_none());
        }
        assert_eq!(place_from_list(&boxes("dungeon-list-school")).as_deref(), Some("트리니티"));
    }

    #[test]
    fn the_squad_screen_names_its_strikers_and_specials() {
        let names = squad_names(&boxes("squad"));
        let texts: Vec<&str> = names.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(texts, vec!["호시노(무장)", "하나코(수영복)", "시로코*테러", "사오리(드레스)", "아코", "키사키"], "{names:?}");
        assert_eq!(names.iter().map(|(_, special)| *special).collect::<Vec<_>>(), vec![false, false, false, false, true, true]);
    }

    #[test]
    fn the_mission_is_kept_while_the_quick_formation_popup_is_open() {
        let mut t = MissionTracker::default();
        t.update(Scene::MissionInfo, &boxes("mission-info"));
        assert!(t.update(Scene::QuickFormation, &boxes("quick-formation")).is_some());
    }

    #[test]
    fn a_name_seen_only_on_the_enemy_popup_is_kept_without_a_number() {
        let mut t = MissionTracker::default();
        let m = t.update(Scene::EnemyInfo, &boxes("enemy-info")).unwrap();
        assert_eq!(m.number, None);
    }
}
