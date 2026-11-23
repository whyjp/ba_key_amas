//! Which game screen is this, and which mission is the player on?
//!
//! Screens are told apart by the text OCR reads on them (header title and a few
//! anchor words), never by position alone, so the same code works at any window
//! size. The mission screens each show different things: only the "임무 정보"
//! popup (and the enemy popup) name the mission, while the hex map and the squad
//! formation that follow show no name. `MissionTracker` remembers the mission
//! from the popup so the later screens know it.
pub use baocr_core::layout::Layout;
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
    /// 총력전: the difficulty rows of the running season ("Battle Select").
    RaidSelect,
    /// 모의 총력전: the boss list with its difficulty rows.
    MockRaidBosses,
    /// The "방 정보" popup of one mock-raid difficulty.
    RaidRoomInfo,
    /// The squad formation of a raid-like mode. It shows no boss, no names of the students, only the party tabs.
    RaidFormation(RaidMode),
    /// 제약해제결전: boss and opening period.
    UnrestrictedRaidSelect,
    /// 제약해제결전: the bands of stages ("단계 01-24 ...").
    UnrestrictedRaidStages,
    /// 제약해제결전: one stage with the boss, HP, level, attack and armor.
    UnrestrictedRaidStage,
    /// 종합전술시험: the next test and its info.
    TimeAttackSelect,
    /// 종합전술시험: the popup "다음 시즌 정보".
    TimeAttackNextSeason,
    /// 대결전 while a season runs (no fixture yet: any 대결전 screen that is not the closed one).
    EliminateSelect,
    /// 대결전 between seasons: "키보토스는 평화로운 상태입니다".
    EliminateClosed,
    /// The warning before 제약해제결전 ("강력한 스포일러가 포함되어 있습니다") over the work hub.
    SpoilerWarning,
    /// 카페.
    Cafe,
    /// 스케줄: the places to visit ("Location Select").
    Schedule,
    /// 제조: crafting.
    Craft,
    /// 상점.
    Shop,
    /// 모집: the recruitment (gacha) screen.
    Recruit,
    /// 이벤트 일람: the list of running events.
    EventList,
    /// 소셜: the menu of 서클, 친구 and 조력자, over the main menu.
    SocialMenu,
    /// 공지: the notice popup over the main menu.
    Notice,
    /// 메뉴 모음: the menu popup (옵션, 계정, 장비, ...) over the main menu.
    MenuCollection,
    /// 옵션: the settings popup over the main menu.
    Options,
    /// 알림: a small dialog that asks for a confirmation, over whatever screen opened it.
    AlertPopup,
    /// The title screen ("TAP TO START").
    Title,
    /// 아로나의 출석부: the daily attendance sheet.
    DailyAttendance,
    /// 이벤트: the stages of a running event (Story, Quest, 개최 기간).
    EventStages,
    /// 총력전 between the end of a season and its results: "시즌 집계 중입니다" instead of the difficulty rows.
    RaidTallying,
    Unknown,
}

/// The raid-like modes that share one formation screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RaidMode {
    /// 총력전.
    TotalAssault,
    /// 모의 총력전.
    MockTotalAssault,
    /// 제약해제결전.
    UnrestrictedRaid,
}

/// Each mode's name as its screens' header shows it (spaces and marks removed).
const RAID_HEADERS: [(&str, RaidMode); 3] = [("총력전", RaidMode::TotalAssault), ("모의총력전", RaidMode::MockTotalAssault), ("제약해제결전", RaidMode::UnrestrictedRaid)];
const FORMATION_SUFFIX: &str = "편성";

impl RaidMode {
    /// The mode's name as the player knows it.
    pub fn label(self) -> &'static str {
        match self {
            RaidMode::TotalAssault => "총력전",
            RaidMode::MockTotalAssault => "모의 총력전",
            RaidMode::UnrestrictedRaid => "제약해제결전",
        }
    }
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
            Scene::RaidSelect => "총력전 선택",
            Scene::MockRaidBosses => "모의 총력전 보스",
            Scene::RaidRoomInfo => "방 정보",
            Scene::RaidFormation(RaidMode::TotalAssault) => "총력전 편성",
            Scene::RaidFormation(RaidMode::MockTotalAssault) => "모의 총력전 편성",
            Scene::RaidFormation(RaidMode::UnrestrictedRaid) => "제약해제결전 편성",
            Scene::UnrestrictedRaidSelect => "제약해제결전",
            Scene::UnrestrictedRaidStages => "제약해제결전 단계",
            Scene::UnrestrictedRaidStage => "제약해제결전 보스",
            Scene::TimeAttackSelect => "종합전술시험",
            Scene::TimeAttackNextSeason => "종합전술시험 다음 시즌",
            Scene::EliminateSelect => "대결전",
            Scene::EliminateClosed => "대결전 (휴식기)",
            Scene::SpoilerWarning => "스포일러 경고",
            Scene::Cafe => "카페",
            Scene::Schedule => "스케줄",
            Scene::Craft => "제조",
            Scene::Shop => "상점",
            Scene::Recruit => "모집",
            Scene::EventList => "이벤트 일람",
            Scene::SocialMenu => "소셜",
            Scene::Notice => "공지",
            Scene::MenuCollection => "메뉴 모음",
            Scene::Options => "옵션",
            Scene::AlertPopup => "알림",
            Scene::Title => "타이틀",
            Scene::DailyAttendance => "출석부",
            Scene::EventStages => "이벤트",
            Scene::RaidTallying => "총력전 (집계 중)",
            Scene::Unknown => "",
        }
    }
}

/// OCR text without spaces and the stray marks it adds ("임무?" for a header with a help icon).
pub(crate) fn norm(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace() && !"?？·•|｜".contains(*c)).collect()
}

fn has(boxes: &[TextBox], word: &str) -> bool {
    boxes.iter().any(|b| norm(&b.text).contains(word))
}

/// The header title at the top left (right of the back arrow).
fn header(boxes: &[TextBox]) -> String {
    boxes.iter().filter(|b| b.y1 < 60.0 && b.x0 > 80.0 && b.x0 < 300.0).map(|b| norm(&b.text)).next().unwrap_or_default()
}

/// Where a popup's title sits (y range). The popup keeps its width in both layouts but is squeezed
/// vertically in 16:9, so its title rises from about y 200..260 to about y 90..140 (the taller 메뉴 모음 title reaches y 326 in 4:3).
fn popup_title_band(layout: Layout) -> (f32, f32) {
    match layout {
        Layout::Ratio43 => (170.0, 340.0),
        Layout::Ratio169 => (60.0, 200.0),
    }
}

/// A popup's title: centred near the top of the popup.
fn popup_title_is(boxes: &[TextBox], word: &str, layout: Layout) -> bool {
    let (top, bottom) = popup_title_band(layout);
    boxes.iter().any(|b| norm(&b.text) == word && b.y0 > top && b.y1 < bottom && (b.x0 + b.x1) / 2.0 > 450.0 && (b.x0 + b.x1) / 2.0 < 830.0)
}

/// Headers of the screens that hold place-based missions.
const DUNGEON_HEADERS: [&str; 3] = ["현상수배", "특별의뢰", "학원교류회"];

/// The main menu's bottom bar names at least four of its eight places.
const LOBBY_LABELS: [&str; 8] = ["카페", "스케줄", "학생", "편성", "소설", "제조", "상점", "모집"];

pub fn is_lobby(boxes: &[TextBox]) -> bool {
    is_lobby_in(boxes, Layout::Ratio43)
}

/// The bar hugs the bottom edge of the frame, so its y follows the frame height.
pub fn is_lobby_in(boxes: &[TextBox], layout: Layout) -> bool {
    let bottom = layout.frame_height() as f32;
    let found = LOBBY_LABELS.iter().filter(|l| boxes.iter().any(|b| b.y0 > bottom - 80.0 && b.y1 < bottom - 2.0 && b.text.trim() == **l)).count();
    found >= 4
}

/// Classify a frame from its OCR text. Student list / profile are decided by the
/// caller from pixels and passed in `pixel_scene`.
pub fn classify(boxes: &[TextBox], pixel_scene: Option<Scene>) -> Scene {
    classify_in(boxes, pixel_scene, Layout::Ratio43)
}

/// [`classify`] for a frame of the given layout.
pub fn classify_in(boxes: &[TextBox], pixel_scene: Option<Scene>, layout: Layout) -> Scene {
    if let Some(s) = pixel_scene {
        return s;
    }
    // a dialog opens over any screen, whose own title may still be readable under it
    if popup_title_is(boxes, "알림", layout) {
        return Scene::AlertPopup;
    }
    if has(boxes, "적정보") && has(boxes, "상성정보") {
        return Scene::EnemyInfo;
    }
    if popup_title_is(boxes, "빠른편성", layout) {
        return Scene::QuickFormation;
    }
    if popup_title_is(boxes, "임무정보", layout) {
        return Scene::MissionInfo;
    }
    if popup_title_is(boxes, "방정보", layout) {
        return Scene::RaidRoomInfo;
    }
    if popup_title_is(boxes, "다음시즌정보", layout) {
        return Scene::TimeAttackNextSeason;
    }
    // popups over the main menu show its bottom bar too, so they are told before the menu itself
    if popup_title_is(boxes, "메뉴모음", layout) {
        return Scene::MenuCollection;
    }
    if popup_title_is(boxes, "옵션", layout) {
        return Scene::Options;
    }
    if has(boxes, "출석부") && has(boxes, "일차") {
        return Scene::DailyAttendance;
    }
    if has(boxes, "TAPTOSTART") {
        return Scene::Title;
    }
    if has(boxes, "서클") && has(boxes, "친구") && has(boxes, "조력자") {
        return Scene::SocialMenu;
    }
    if has(boxes, "진행이벤트") && has(boxes, "공략게시판") {
        return Scene::Notice;
    }
    let header = header(boxes);
    if let Some(scene) = raid_scene(&header, boxes) {
        return scene;
    }
    match header.as_str() {
        "임무" if has(boxes, "투입부대") || has(boxes, "임무개시") => Scene::MissionMap,
        "임무" if has(boxes, "Normal") && has(boxes, "Hard") => Scene::MissionList,
        "편성" => Scene::Squad,
        "카페" => Scene::Cafe,
        // the game's own word is 스케줄; OCR reads the last syllable both ways
        "스케줄" | "스케쥴" => Scene::Schedule,
        "제조" => Scene::Craft,
        "상점" => Scene::Shop,
        "모집" => Scene::Recruit,
        "이벤트일람" => Scene::EventList,
        "이벤트" if has(boxes, "Story") && has(boxes, "Quest") => Scene::EventStages,
        "업무" if has(boxes, "스포일러") => Scene::SpoilerWarning,
        "업무" => Scene::WorkHub,
        h if DUNGEON_HEADERS.contains(&h) && has(boxes, "스테이지목록") => Scene::DungeonList,
        h if DUNGEON_HEADERS.contains(&h) && has(boxes, "Select") => Scene::DungeonSelect,
        // the 16:9 work hub shows no header, only its places
        "" if has(boxes, "현상수배") && has(boxes, "특별의뢰") && has(boxes, "학원교류회") => Scene::WorkHub,
        _ if is_lobby_in(boxes, layout) => Scene::Lobby,
        _ => Scene::Unknown,
    }
}

/// The screens of the raid-like modes, told by their header and the words only they show.
/// The mode comes from the header alone: a 모의 총력전 screen also shows "총력전" as a side button.
fn raid_scene(header: &str, boxes: &[TextBox]) -> Option<Scene> {
    if let Some((_, mode)) = RAID_HEADERS.iter().find(|(name, _)| header.strip_suffix(FORMATION_SUFFIX) == Some(*name)) {
        return Some(Scene::RaidFormation(*mode));
    }
    match header {
        "총력전" if has(boxes, "BattleSelect") => Some(Scene::RaidSelect),
        "총력전" if has(boxes, "시즌집계중") => Some(Scene::RaidTallying),
        "모의총력전" if has(boxes, "보스선택") => Some(Scene::MockRaidBosses),
        "제약해제결전" if has(boxes, "단계선택") || has(boxes, "클리어부대") => Some(Scene::UnrestrictedRaidStage),
        "제약해제결전" if has(boxes, "단계클리어시") => Some(Scene::UnrestrictedRaidStages),
        "제약해제결전" if has(boxes, "플레이팁") || has(boxes, "최고기록") || has(boxes, "접속을환영합니다") => Some(Scene::UnrestrictedRaidSelect),
        "종합전술시험" if has(boxes, "다음시험") || has(boxes, "정보") => Some(Scene::TimeAttackSelect),
        "대결전" if has(boxes, "평화로운상태") => Some(Scene::EliminateClosed),
        "대결전" => Some(Scene::EliminateSelect),
        _ => None,
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

/// Where the text parsers look on one screen, in frame pixels. The game keeps the size of its UI in both
/// layouts but lays it out differently: popups are squeezed vertically, so their lines sit higher in 16:9,
/// and the squad screen's rows move by their own amounts (each measured on its recordings).
struct Spots {
    /// The mission line of the "임무 정보" popup (y range of a box).
    info_line: (f32, f32),
    /// The mission name in the enemy popup's title row.
    enemy_title: (f32, f32),
    /// The names of the strikers and of the specials on the squad screen (range of a box's top).
    strikers: (f32, f32),
    specials: (f32, f32),
}

fn spots(layout: Layout) -> Spots {
    match layout {
        Layout::Ratio43 => Spots { info_line: (300.0, 410.0), enemy_title: (190.0, 300.0), strikers: (690.0, 745.0), specials: (880.0, 935.0) },
        Layout::Ratio169 => Spots { info_line: (200.0, 275.0), enemy_title: (70.0, 150.0), strikers: (510.0, 570.0), specials: (640.0, 700.0) },
    }
}

/// The mission named on the "임무 정보" popup (its title line, around y 300..410 in 4:3).
pub fn mission_from_info(boxes: &[TextBox]) -> Option<MissionRef> {
    mission_from_info_in(boxes, Layout::Ratio43)
}

pub fn mission_from_info_in(boxes: &[TextBox], layout: Layout) -> Option<MissionRef> {
    let (top, bottom) = spots(layout).info_line;
    boxes.iter().filter(|b| b.y0 > top && b.y1 < bottom && b.x0 < 560.0).find_map(|b| {
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

/// The mission named on the enemy popup (its title, around y 200..300 in 4:3).
pub fn mission_from_enemy_popup(boxes: &[TextBox]) -> Option<MissionRef> {
    mission_from_enemy_popup_in(boxes, Layout::Ratio43)
}

pub fn mission_from_enemy_popup_in(boxes: &[TextBox], layout: Layout) -> Option<MissionRef> {
    let (top, bottom) = spots(layout).enemy_title;
    boxes
        .iter()
        .filter(|b| b.y0 > top && b.y1 < bottom && (b.x0 + b.x1) / 2.0 > 450.0 && (b.x0 + b.x1) / 2.0 < 830.0)
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
    squad_names_in(boxes, Layout::Ratio43)
}

pub fn squad_names_in(boxes: &[TextBox], layout: Layout) -> Vec<(String, bool)> {
    let has_hangul = |t: &str| t.chars().any(|c| ('\u{AC00}'..='\u{D7A3}').contains(&c));
    let Spots { strikers, specials, .. } = spots(layout);
    boxes
        .iter()
        .filter(|b| {
            let in_strikers = b.y0 >= strikers.0 && b.y0 <= strikers.1 && b.x0 >= 200.0 && b.x1 <= 1100.0;
            let in_specials = b.y0 >= specials.0 && b.y0 <= specials.1 && b.x0 >= 470.0 && b.x1 <= 1000.0;
            (in_strikers || in_specials) && has_hangul(&b.text)
        })
        .map(|b| (b.text.trim().to_string(), b.y0 >= specials.0))
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
        self.update_in(scene, boxes, Layout::Ratio43)
    }

    /// [`update`](Self::update) for a frame of the given layout.
    pub fn update_in(&mut self, scene: Scene, boxes: &[TextBox], layout: Layout) -> Option<&MissionRef> {
        match scene {
            Scene::MissionInfo => {
                if let Some(mut m) = mission_from_info_in(boxes, layout) {
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
                if let Some(m) = mission_from_enemy_popup_in(boxes, layout) {
                    let same = self.mission.as_ref().is_some_and(|cur| norm(&cur.name) == norm(&m.name));
                    if !same {
                        self.mission = Some(m);
                    }
                }
            }
            // the stage list names the place its stages belong to
            // (no 16:9 recording of a stage list yet: its card is not located there, so no place is read)
            Scene::DungeonList => {
                self.mission = None;
                let read = if layout == Layout::Ratio43 { place_from_list(boxes) } else { None };
                self.place = read.or_else(|| self.place.take());
            }
            // back at a place from which another mission is picked (or none), or in a mode that has no campaign mission: forget
            Scene::DungeonSelect
            | Scene::MissionList
            | Scene::WorkHub
            | Scene::Lobby
            | Scene::LobbyIdle
            | Scene::StudentList
            | Scene::StudentProfile
            | Scene::RaidSelect
            | Scene::MockRaidBosses
            | Scene::RaidRoomInfo
            | Scene::RaidFormation(_)
            | Scene::UnrestrictedRaidSelect
            | Scene::UnrestrictedRaidStages
            | Scene::UnrestrictedRaidStage
            | Scene::TimeAttackSelect
            | Scene::TimeAttackNextSeason
            | Scene::EliminateSelect
            | Scene::EliminateClosed
            | Scene::SpoilerWarning
            | Scene::Cafe
            | Scene::Schedule
            | Scene::Craft
            | Scene::Shop
            | Scene::Recruit
            | Scene::EventList
            | Scene::SocialMenu
            | Scene::Notice
            | Scene::MenuCollection
            | Scene::Options
            | Scene::Title
            | Scene::DailyAttendance
            | Scene::EventStages
            | Scene::RaidTallying => {
                self.mission = None;
                self.place = None;
            }
            // a dialog over a screen leaves what the screen under it showed
            Scene::MissionMap | Scene::Squad | Scene::QuickFormation | Scene::Battle | Scene::AlertPopup | Scene::Unknown => {}
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
            ("menu-formation", Scene::Squad),
            ("menu-cafe", Scene::Cafe),
            ("menu-schedule", Scene::Schedule),
            ("menu-craft", Scene::Craft),
            ("menu-shop", Scene::Shop),
            ("menu-recruit", Scene::Recruit),
            ("menu-events", Scene::EventList),
            ("menu-social", Scene::SocialMenu),
            ("menu-notice", Scene::Notice),
        ] {
            assert_eq!(classify(&boxes(file), None), want, "{file}");
        }
    }

    #[test]
    fn every_recorded_raid_screen_is_recognised() {
        let mock_bosses: Vec<String> = (0..=14).map(|i| format!("raid-mock-m{i:02}")).collect();
        let mut table: Vec<(&str, Scene)> = mock_bosses.iter().map(|f| (f.as_str(), Scene::MockRaidBosses)).collect();
        table.extend([
            ("work-hub-full", Scene::WorkHub),
            ("spoiler-warning", Scene::SpoilerWarning),
            ("raid-select", Scene::RaidSelect),
            ("raid-mock-room-info", Scene::RaidRoomInfo),
            ("raid-formation-empty", Scene::RaidFormation(RaidMode::MockTotalAssault)),
            ("unrestricted-raid-formation", Scene::RaidFormation(RaidMode::UnrestrictedRaid)),
            ("unrestricted-raid-select", Scene::UnrestrictedRaidSelect),
            ("unrestricted-raid-stages", Scene::UnrestrictedRaidStages),
            ("unrestricted-raid-stage01", Scene::UnrestrictedRaidStage),
            ("timeattack-select", Scene::TimeAttackSelect),
            ("timeattack-next-season", Scene::TimeAttackNextSeason),
            ("eliminate-closed", Scene::EliminateClosed),
        ]);
        for (file, want) in table {
            assert_eq!(classify(&boxes(file), None), want, "{file}");
        }
    }

    #[test]
    fn the_formation_mode_comes_from_the_header_and_not_from_words_elsewhere_on_the_screen() {
        let with_header = |file: &str, header: &str| {
            let mut b = boxes(file);
            b.iter_mut().find(|b| b.y0 < 50.0 && b.x0 > 80.0 && b.x0 < 300.0).unwrap().text = header.into();
            classify(&b, None)
        };
        // not recorded: the plain total assault's formation, inferred from the other two headers
        assert_eq!(with_header("raid-formation-empty", "총력전 편성"), Scene::RaidFormation(RaidMode::TotalAssault));
        assert_eq!(with_header("raid-formation-empty", "모의 총력전 편성 ?"), Scene::RaidFormation(RaidMode::MockTotalAssault));
        assert_eq!(with_header("raid-formation-empty", "편성"), Scene::Squad);
        // the mock list shows "총력전" as a side button and must not become the real total assault
        assert!(boxes("raid-mock-m00").iter().any(|b| b.text == "총력전"));
        assert_eq!(classify(&boxes("raid-mock-m00"), None), Scene::MockRaidBosses);
        // a 대결전 screen that is not the closed one (no fixture: no season was running)
        let mut open = boxes("eliminate-closed");
        open.retain(|b| !b.text.contains("평화로운"));
        assert_eq!(classify(&open, None), Scene::EliminateSelect);
    }

    #[test]
    fn raid_screens_have_a_label_for_the_card_and_formations_name_their_mode() {
        for scene in [Scene::RaidSelect, Scene::MockRaidBosses, Scene::RaidRoomInfo, Scene::UnrestrictedRaidSelect, Scene::UnrestrictedRaidStages, Scene::UnrestrictedRaidStage, Scene::TimeAttackSelect, Scene::TimeAttackNextSeason, Scene::EliminateSelect, Scene::EliminateClosed, Scene::SpoilerWarning] {
            assert!(!scene.label().is_empty(), "{scene:?}");
        }
        assert_eq!(Scene::RaidFormation(RaidMode::MockTotalAssault).label(), "모의 총력전 편성");
        assert_eq!(Scene::RaidFormation(RaidMode::UnrestrictedRaid).label(), "제약해제결전 편성");
        assert_eq!(Scene::RaidFormation(RaidMode::TotalAssault).label(), "총력전 편성");
    }

    #[test]
    fn the_campaign_mission_is_forgotten_in_a_raid_mode_and_not_brought_back() {
        let mut t = MissionTracker::default();
        t.update(Scene::MissionInfo, &boxes("mission-info"));
        assert!(t.update(Scene::RaidFormation(RaidMode::MockTotalAssault), &boxes("raid-formation-empty")).is_none());
        assert!(t.update(Scene::QuickFormation, &boxes("quick-formation")).is_none());
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

    #[test]
    fn the_menu_popups_are_recognised_over_the_main_menu() {
        assert_eq!(classify(&boxes("menu-collection"), None), Scene::MenuCollection);
        assert_eq!(classify(&boxes("menu-options-game"), None), Scene::Options);
        assert_eq!(classify(&boxes("menu-options-graphic"), None), Scene::Options);
        // a popup title is a popup title only where a popup puts it: the 옵션 button of the menu popup is not one
        let mut menu = boxes("menu-collection");
        menu.retain(|b| !b.text.contains("메뉴 모음"));
        assert_eq!(classify(&menu, None), Scene::Lobby);
    }

    #[test]
    fn every_screen_recorded_in_16_9_is_recognised() {
        for (file, want) in [
            ("w169-lobby", Scene::Lobby),
            ("w169-work-hub", Scene::WorkHub),
            ("w169-mission-list", Scene::MissionList),
            ("w169-mission-info", Scene::MissionInfo),
            ("w169-enemy-info", Scene::EnemyInfo),
            ("w169-squad", Scene::Squad),
            ("w169-quick-formation", Scene::QuickFormation),
            ("w169-mock-bosses", Scene::MockRaidBosses),
            ("w169-mock-room-info", Scene::RaidRoomInfo),
            ("w169-raid-formation", Scene::RaidFormation(RaidMode::MockTotalAssault)),
            ("w169-raid-quick-formation", Scene::QuickFormation),
            ("w169-raid-tallying", Scene::RaidTallying),
            ("w169-notice", Scene::Notice),
            ("w169-alert-date-changed", Scene::AlertPopup),
            ("w169-alert-level", Scene::AlertPopup),
            ("w169-title", Scene::Title),
            ("w169-daily-attendance", Scene::DailyAttendance),
            ("w169-event-open", Scene::EventStages),
        ] {
            assert_eq!(classify_in(&boxes(file), None, Layout::Ratio169), want, "{file}");
        }
    }

    #[test]
    fn a_dialog_over_a_popup_leaves_the_mission_as_it_was() {
        let mut t = MissionTracker::default();
        t.update_in(Scene::MissionInfo, &boxes("w169-mission-info"), Layout::Ratio169);
        let kept = t.update_in(Scene::AlertPopup, &boxes("w169-alert-level"), Layout::Ratio169).cloned();
        assert_eq!(kept.map(|m| m.number), Some(Some((19, 3))), "the dialog names nothing and keeps the mission");
        // the title screen forgets it
        assert!(t.update_in(Scene::Title, &boxes("w169-title"), Layout::Ratio169).is_none());
    }

    #[test]
    fn the_16_9_mission_popups_name_the_mission() {
        let m = mission_from_info_in(&boxes("w169-mission-info"), Layout::Ratio169).unwrap();
        assert_eq!((m.number, norm(&m.name)), (Some((19, 3)), "설원지구제2지역".to_string()));
        assert_eq!(mission_from_info(&boxes("w169-mission-info")), None, "the 4:3 band does not hold the 16:9 title");
        let e = mission_from_enemy_popup_in(&boxes("w169-enemy-info"), Layout::Ratio169).unwrap();
        assert_eq!(e.name, "설원 지구 제2 지역");
        let mut t = MissionTracker::default();
        assert_eq!(t.update_in(Scene::MissionInfo, &boxes("w169-mission-info"), Layout::Ratio169).unwrap().number, Some((19, 3)));
        assert_eq!(t.update_in(Scene::EnemyInfo, &boxes("w169-enemy-info"), Layout::Ratio169).unwrap().number, Some((19, 3)));
        assert_eq!(t.update_in(Scene::Squad, &boxes("w169-squad"), Layout::Ratio169).unwrap().number, Some((19, 3)));
        assert!(t.update_in(Scene::MissionList, &boxes("w169-mission-list"), Layout::Ratio169).is_none());
    }

    #[test]
    fn the_16_9_squad_screen_names_its_strikers_and_specials() {
        let names = squad_names_in(&boxes("w169-squad"), Layout::Ratio169);
        let texts: Vec<&str> = names.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(texts, vec!["호시노(무장)", "하나코(수영복)", "시로코*테러", "미카", "아코", "키사키"], "{names:?}");
        assert_eq!(names.iter().map(|(_, special)| *special).collect::<Vec<_>>(), vec![false, false, false, false, true, true]);
        // the empty raid formation shows no student, and the quick formation popup only the specials under it
        assert!(squad_names_in(&boxes("w169-raid-formation"), Layout::Ratio169).is_empty());
        let under_popup = squad_names_in(&boxes("w169-quick-formation"), Layout::Ratio169);
        assert_eq!(under_popup.iter().filter(|(_, special)| *special).map(|(n, _)| n.as_str()).collect::<Vec<_>>(), vec!["아코", "키사키"], "{under_popup:?}");
    }

    #[test]
    fn the_new_screens_have_labels() {
        for scene in [Scene::MenuCollection, Scene::Options, Scene::AlertPopup, Scene::Title, Scene::DailyAttendance, Scene::EventStages, Scene::RaidTallying] {
            assert!(!scene.label().is_empty(), "{scene:?}");
        }
    }
}
