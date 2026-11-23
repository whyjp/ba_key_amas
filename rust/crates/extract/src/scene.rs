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
    /// 업무: the hub that leads to 임무, 현상수배, 총력전, ...
    WorkHub,
    /// 임무: the list of one area's missions (Normal / Hard tabs).
    MissionList,
    /// "임무 정보" popup of one mission.
    MissionInfo,
    /// "적 정보 / 상성 정보" popup of one mission.
    EnemyInfo,
    /// The hex map shown after 임무 시작 (header "임무", no mission name).
    MissionMap,
    /// 편성: squad formation.
    Squad,
    /// Student list and student profile, told apart by pixels, not text.
    StudentList,
    StudentProfile,
    Unknown,
}

impl Scene {
    /// Name shown to the player.
    pub fn label(self) -> &'static str {
        match self {
            Scene::Lobby => "메인 메뉴",
            Scene::WorkHub => "업무",
            Scene::MissionList => "임무 선택",
            Scene::MissionInfo => "임무 정보",
            Scene::EnemyInfo => "적 정보",
            Scene::MissionMap => "임무 지도",
            Scene::Squad => "편성",
            Scene::StudentList => "학생 목록",
            Scene::StudentProfile => "학생 정보",
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
    boxes.iter().any(|b| norm(&b.text) == word && b.y0 > 190.0 && b.y1 < 270.0 && (b.x0 + b.x1) / 2.0 > 450.0 && (b.x0 + b.x1) / 2.0 < 830.0)
}

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
    if popup_title_is(boxes, "임무정보") {
        return Scene::MissionInfo;
    }
    match header(boxes).as_str() {
        "임무" if has(boxes, "투입부대") || has(boxes, "임무개시") => Scene::MissionMap,
        "임무" if has(boxes, "Normal") && has(boxes, "Hard") => Scene::MissionList,
        "편성" => Scene::Squad,
        "업무" => Scene::WorkHub,
        _ if is_lobby(boxes) => Scene::Lobby,
        _ => Scene::Unknown,
    }
}

/// A campaign mission as the game shows it: "19-3 | 설원 지구 제2 지역".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissionRef {
    /// Area and stage number; None when only the name was seen (the enemy popup).
    pub number: Option<(u32, u32)>,
    pub name: String,
}

impl MissionRef {
    /// "19-3 설원 지구 제2 지역", or just the name.
    pub fn display(&self) -> String {
        match self.number {
            Some((a, s)) => format!("{a}-{s} {}", self.name),
            None => self.name.clone(),
        }
    }
}

static NUMBERED: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\s*(\d{1,2})\s*[-–—]\s*(\d{1,2})\s*[|｜lI]?\s*(.+?)\s*$").unwrap());

/// The mission named on the "임무 정보" popup (its title line, around y 330..400).
pub fn mission_from_info(boxes: &[TextBox]) -> Option<MissionRef> {
    boxes.iter().filter(|b| b.y0 > 320.0 && b.y1 < 410.0 && b.x0 < 560.0).find_map(|b| {
        let c = NUMBERED.captures(&b.text)?;
        let name = c[3].trim().to_string();
        let number = (c[1].parse().ok()?, c[2].parse().ok()?);
        (name.chars().count() >= 2).then_some(MissionRef { number: Some(number), name })
    })
}

/// The mission named on the enemy popup (its title, around y 200..260).
pub fn mission_from_enemy_popup(boxes: &[TextBox]) -> Option<MissionRef> {
    boxes
        .iter()
        .filter(|b| b.y0 > 190.0 && b.y1 < 270.0 && (b.x0 + b.x1) / 2.0 > 450.0 && (b.x0 + b.x1) / 2.0 < 830.0)
        .map(|b| b.text.trim().to_string())
        .find(|t| t.chars().count() >= 2 && !t.contains('×'))
        .map(|name| MissionRef { number: None, name })
}

/// Remembers the mission across the screens that do not name it.
#[derive(Debug, Default)]
pub struct MissionTracker {
    mission: Option<MissionRef>,
}

impl MissionTracker {
    pub fn mission(&self) -> Option<&MissionRef> {
        self.mission.as_ref()
    }

    /// Feed the screen just recognised (with its OCR text) and get the mission now in play.
    pub fn update(&mut self, scene: Scene, boxes: &[TextBox]) -> Option<&MissionRef> {
        match scene {
            Scene::MissionInfo => {
                if let Some(m) = mission_from_info(boxes) {
                    self.mission = Some(m);
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
            // back at a place from which another mission is picked (or none): forget
            Scene::MissionList | Scene::WorkHub | Scene::Lobby | Scene::StudentList | Scene::StudentProfile => self.mission = None,
            Scene::MissionMap | Scene::Squad | Scene::Unknown => {}
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
        assert_eq!(m, MissionRef { number: Some((19, 3)), name: "설원 지구 제2 지역".into() });
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
    fn a_name_seen_only_on_the_enemy_popup_is_kept_without_a_number() {
        let mut t = MissionTracker::default();
        let m = t.update(Scene::EnemyInfo, &boxes("enemy-info")).unwrap();
        assert_eq!(m.number, None);
    }
}
