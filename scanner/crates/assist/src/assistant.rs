//! The coordinator: observation in, widgets, settings, surface out.
//!
//! It owns no screen-reading and no drawing. Each frame it asks the `Observer` what is
//! on screen, asks every `Widget` what it would show, and sends each output where the
//! `Settings` say (beside the game, inside it, nowhere) through the `Surface`. Clicks on
//! the card's buttons come back as events and change the settings.
//!
//! The main card is always there while the add-on runs and says what the add-on is doing and
//! what it can do: the game's state, what is on screen, how stale the student list is, the
//! scan, the display switches and the way to close it. While a scan runs the card shows the scan.
use crate::library::LibraryInfo;
use crate::observe::{Observation, Observer};
use crate::raid_lookup::{self, RaidLookup};
use crate::settings::{Placement, Settings};
use crate::surface::Surface;
use crate::widget::{is_mission_screen, Context, Kind, Lookup, Output, Widget, WidgetId};
use crate::widgets;
use baocr_core::ocr_api::TextBox;
use baocr_extract::raid::RaidRef;
use baocr_extract::scene::{MissionRef, Scene};
use baocr_overlay::{scan_rows, Anchor, BannerState, Mark, PanelState, Presentation, Row};
use baocr_wikidb::{StudentBrief, TypeTable, Wiki};
use image::RgbImage;
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const BUTTON_INGAME: &str = "ingame";
const BUTTON_PLACE: &str = "place:";
const BUTTON_SCAN: &str = "scan:start";
const BUTTON_SCAN_DEEP: &str = "scan:deep";
const BUTTON_PROGRESS_SCAN: &str = "scan:progress";
/// Sends the last student scan and progress scan together.
const BUTTON_UPLOAD: &str = "upload";
/// The gear in the card's header: the display options replace the data section until it is pressed again.
const BUTTON_SETTINGS: &str = "settings";
const BUTTON_CODE_PASTE: &str = "code:paste";
const BUTTON_CODE_CHANGE: &str = "code:change";
const BUTTON_LINK_OPEN: &str = "upload:open";
const BUTTON_LINK_COPY: &str = "upload:copy";
/// Sent by the card's 종료 button and by the tray icon's 종료.
const BUTTON_QUIT: &str = "quit";
/// Folds and unfolds the student-list section.
const BUTTON_STUDENT_SECTION: &str = "section:student";

/// Something the player asked for that the add-on itself does not do: the program's main loop
/// carries it out (the scan is the only thing that sends input to the game).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Scan the student list; `deep` also reads the popup and the two tabs of each student.
    StartScan { deep: bool },
    /// Read the account's progress (schedule ranks, story and event progress) from the game's screens.
    StartProgressScan,
    /// Send the last student scan and progress scan to the web server.
    Upload,
    /// Take the member code from the clipboard (the card shows it and sends it with the upload).
    PasteMemberCode,
    /// Open a page of the web server in the browser.
    OpenUrl(String),
    /// Put a text on the clipboard.
    CopyText(String),
    /// End the add-on.
    Quit,
}

/// What was last handed to the surface, so that an identical output is not handed over again.
#[derive(Default)]
struct Sent {
    main: Option<PanelState>,
    ingame: Option<Option<PanelState>>,
    marks: Option<Vec<Mark>>,
}

/// Keeps `slot` at `new`; true when that is a change the surface has to be told about.
fn changed<T: PartialEq>(slot: &mut Option<T>, new: &T) -> bool
where
    T: Clone,
{
    if slot.as_ref() == Some(new) {
        return false;
    }
    *slot = Some(new.clone());
    true
}

pub struct Assistant<'a, S: Surface> {
    observer: Observer<'a>,
    wiki: &'a Wiki,
    students: &'a [StudentBrief],
    types: &'a TypeTable,
    widgets: Vec<Box<dyn Widget>>,
    settings: Settings,
    settings_path: Option<PathBuf>,
    surface: S,
    last: Observation,
    library: LibraryInfo,
    game_present: bool,
    now: SystemTime,
    /// The age line last shown, to redraw only when the minute it names changes.
    age_shown: String,
    /// How many times the card and the icons were worked out (a test and the profile read it).
    presented: Cell<u64>,
    scanning: bool,
    /// What the last upload said (a line each), shown under the upload buttons; empty before the first.
    upload_note: Vec<String>,
    /// An upload is under way (the main loop is waiting for the server).
    uploading: bool,
    /// The display options are shown instead of the data section.
    settings_open: bool,
    /// The member code pasted on the card, until an upload has used it (then only its hash is kept, in the account file).
    member_code: Option<String>,
    /// An account is already saved: no code is asked for.
    account_saved: bool,
    /// The page of the last upload.
    upload_link: Option<String>,
    scan_state: RefCell<BannerState>,
    /// The player's mouse and keyboard are blocked while the scan runs.
    input_locked: bool,
    commands: Vec<Command>,
    sent: RefCell<Sent>,
    /// The screen is changing and the last reading no longer describes it: no icons until the next reading.
    view_moving: Cell<bool>,
    /// The wiki's answer for the last mission asked about (the same mission is asked about on every redraw).
    looked_up: RefCell<Option<(MissionRef, Lookup)>>,
    /// The same for the raid encounter, and the hour it was asked in (the season it falls in can change).
    looked_up_raid: RefCell<Option<(RaidRef, i64, RaidLookup)>>,
}

fn lookup(wiki: &Wiki, obs: &Observation) -> Lookup {
    let Some(m) = obs.mission.as_ref().filter(|_| is_mission_screen(obs.scene)) else { return Lookup::None };
    let found = match (m.stage_no, m.place.as_deref()) {
        (Some(n), Some(place)) => wiki.dungeon_missions(place, n),
        (Some(_), None) => return Lookup::PlaceUnknown,
        _ => wiki.campaign_missions(m.number, &m.name),
    }
    .unwrap_or_default();
    match found.first() {
        Some(brief) => Lookup::Found { brief: brief.clone(), alternatives: found.len() },
        None => Lookup::NotFound,
    }
}

impl<'a, S: Surface> Assistant<'a, S> {
    pub fn new(observer: Observer<'a>, wiki: &'a Wiki, students: &'a [StudentBrief], types: &'a TypeTable, settings: Settings, settings_path: Option<PathBuf>, surface: S) -> Self {
        surface.set_dock(settings.dock());
        Assistant {
            observer,
            wiki,
            students,
            types,
            widgets: widgets::all(),
            settings,
            settings_path,
            surface,
            last: Observation::new(Scene::Unknown),
            library: LibraryInfo::default(),
            game_present: false,
            now: SystemTime::now(),
            age_shown: String::new(),
            presented: Cell::new(0),
            scanning: false,
            upload_note: Vec::new(),
            uploading: false,
            settings_open: false,
            member_code: None,
            account_saved: false,
            upload_link: None,
            scan_state: RefCell::new(BannerState::counting(0)),
            input_locked: false,
            commands: Vec::new(),
            sent: RefCell::default(),
            looked_up: RefCell::new(None),
            view_moving: Cell::new(false),
            looked_up_raid: RefCell::new(None),
        }
    }

    /// The game went away: forget everything read from it and show nothing of it.
    pub fn forget(&mut self) {
        self.observer.forget();
        self.show(Observation::new(Scene::Unknown));
    }

    /// Whether the game window is there (the card says so).
    pub fn set_game_present(&mut self, present: bool) {
        if self.game_present != present {
            self.game_present = present;
            self.present();
        }
    }

    /// What is stored about the player's students (shown on the card).
    pub fn set_library(&mut self, info: LibraryInfo) {
        self.library = info;
        self.present();
    }

    /// Time moves on: the age of the student list is redrawn when the minute it shows changes.
    pub fn tick(&mut self, now: SystemTime) {
        self.now = now;
        let age = self.library.age_text(now);
        if age != self.age_shown {
            // remembered, or every tick finds the text "changed" and draws the whole card again (13 ms in 빠른 편성)
            self.age_shown = age;
            self.present();
        }
    }

    /// How many times everything shown was worked out from the observation.
    pub fn presented(&self) -> u64 {
        self.presented.get()
    }

    /// Whether the player asked for something the main loop has to carry out.
    pub fn has_commands(&self) -> bool {
        !self.commands.is_empty()
    }

    /// Requests made through the card since the last call.
    pub fn take_commands(&mut self) -> Vec<Command> {
        std::mem::take(&mut self.commands)
    }

    /// A scan is starting: the card shows it, and the player's mouse and keyboard are blocked
    /// (ESC stops the scan and gives them back).
    pub fn begin_scan(&mut self) {
        self.scanning = true;
        *self.scan_state.borrow_mut() = BannerState::counting(0);
        self.input_locked = self.surface.lock_input(true);
        self.present();
    }

    pub fn scan_progress(&self, state: BannerState) {
        *self.scan_state.borrow_mut() = state;
        self.present();
    }

    /// Give the player's input back (the scan ended or was stopped).
    pub fn release_input(&mut self) {
        if self.input_locked {
            self.surface.lock_input(false);
            self.input_locked = false;
        }
    }

    /// The scan is over: the card is the main card again, with the new state of the list.
    pub fn scan_finished(&mut self, info: LibraryInfo) {
        self.release_input();
        self.scanning = false;
        self.library = info;
        self.forget();
    }

    /// The player's page from an earlier upload (it is shown until the next upload says otherwise).
    pub fn set_page_link(&mut self, url: Option<String>) {
        self.upload_link = url;
        self.present();
    }

    /// Whether an account is saved from an earlier upload.
    pub fn set_account_saved(&mut self, saved: bool) {
        self.account_saved = saved;
        self.present();
    }

    /// The member code pasted from the clipboard (surrounding blanks dropped); an empty text clears it.
    pub fn set_member_code(&mut self, text: &str) {
        let code = text.trim();
        self.member_code = (!code.is_empty()).then(|| code.to_string());
        self.present();
    }

    pub fn member_code(&self) -> Option<&str> {
        self.member_code.as_deref()
    }

    /// An upload starts: the card says so until the server has answered.
    pub fn begin_upload(&mut self) {
        self.uploading = true;
        self.upload_note = vec!["보내는 중...".into()];
        self.present();
    }

    /// A line under the upload buttons (the link of the last upload stays).
    pub fn say(&mut self, text: &str) {
        self.upload_note = vec![text.to_string()];
        self.present();
    }

    /// The upload ended: `text` is what to show, one line per line.
    pub fn upload_finished(&mut self, text: &str, link: Option<String>) {
        self.uploading = false;
        self.upload_link = link;
        self.upload_note = text.lines().map(str::to_string).filter(|l| !l.trim().is_empty()).collect();
        self.present();
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    /// Whether the frame shows the 빠른 편성 grid as it was last read, so that its text need not be read again.
    pub fn grid_unchanged(&self, img: &RgbImage) -> bool {
        self.observer.grid_unchanged(img)
    }

    /// See [`Observer::striker_popup`].
    pub fn striker_popup(&self, img: &RgbImage) -> bool {
        self.observer.striker_popup(img)
    }

    /// See [`Observer::grid_settled`].
    pub fn grid_settled(&mut self, img: &RgbImage) -> bool {
        self.observer.grid_settled(img)
    }

    /// A new settled frame: read it and show what follows from it.
    pub fn on_frame(&mut self, img: &RgbImage, boxes: &[TextBox], fallback: Scene) -> Scene {
        let obs = self.observer.observe(img, boxes, fallback);
        let scene = obs.scene;
        self.show(obs);
        scene
    }

    /// The screen is changing (a scroll, a popup closing): icons placed for the old view would sit
    /// on the wrong things until the new one is read, so take them down.
    pub fn transition(&self) {
        // Remembered, not only sent: any later `present` (a click on the card, the list's age moving
        // on a minute) draws from the last reading, and would put the old view's icons back.
        self.view_moving.set(true);
        self.send_marks(Vec::new());
    }

    fn send_main(&self, mut card: PanelState) {
        // closing the add-on is the icon in the header of every main card
        card.close = Some((BUTTON_QUIT.into(), "애드온 종료".into()));
        // the display options are one press away on every card but the battle's (which keeps out of the way)
        if card.scene != Scene::Battle.label() {
            card.settings = Some((BUTTON_SETTINGS.into(), if self.settings_open { "표시 설정 닫기".into() } else { "표시 설정".into() }));
        }
        if changed(&mut self.sent.borrow_mut().main, &card) {
            self.surface.main_card(card);
        }
    }

    fn send_ingame(&self, card: Option<PanelState>) {
        if changed(&mut self.sent.borrow_mut().ingame, &card) {
            self.surface.ingame_card(card);
        }
    }

    fn send_marks(&self, marks: Vec<Mark>) {
        if changed(&mut self.sent.borrow_mut().marks, &marks) {
            self.surface.marks(marks);
        }
    }

    /// Show what an observation calls for.
    pub fn show(&mut self, obs: Observation) {
        self.last = obs;
        self.view_moving.set(false);
        self.present();
    }

    /// Handle the card's button clicks. Returns whether anything changed.
    pub fn poll(&mut self) -> bool {
        let events = self.surface.take_events();
        let mut changed = false;
        for e in events {
            changed |= self.handle(&e);
        }
        if changed {
            if let Some(path) = &self.settings_path {
                let _ = self.settings.save(path);
            }
            self.present();
        }
        changed
    }

    fn handle(&mut self, event: &str) -> bool {
        // the card was dragged and snapped to a side: remember where
        if let Some(anchor) = Anchor::decode(event) {
            self.settings.set_anchor(anchor);
            return true;
        }
        // the pin button: the card now stays open, or folds into a tab
        if let Some(p) = Presentation::decode(event) {
            self.settings.set_presentation(p);
            return true;
        }
        if event == BUTTON_SETTINGS {
            self.settings_open = !self.settings_open;
            return true;
        }
        if event == BUTTON_STUDENT_SECTION {
            self.settings.student_open = !self.settings.student_open;
            return true;
        }
        if event == BUTTON_QUIT {
            if !self.commands.contains(&Command::Quit) {
                self.commands.push(Command::Quit);
            }
            return false;
        }
        if event == BUTTON_SCAN {
            // one scan at a time; a click while scanning is ignored
            let start = Command::StartScan { deep: self.settings.scan_deep };
            if !self.scanning && !self.commands.contains(&start) {
                self.commands.push(start);
            }
            return false;
        }
        if event == BUTTON_PROGRESS_SCAN {
            if !self.scanning && !self.commands.contains(&Command::StartProgressScan) {
                self.commands.push(Command::StartProgressScan);
            }
            return false;
        }
        if event == BUTTON_UPLOAD {
            if self.uploading || self.scanning {
                return false;
            }
            if !self.account_saved && self.member_code.is_none() {
                self.upload_note = vec!["회원코드를 먼저 붙여넣어 주세요 (아래 업로드 항목)".into()];
                return true;
            }
            if !self.commands.contains(&Command::Upload) {
                self.commands.push(Command::Upload);
            }
            return false;
        }
        if event == BUTTON_CODE_PASTE {
            if !self.commands.contains(&Command::PasteMemberCode) {
                self.commands.push(Command::PasteMemberCode);
            }
            return false;
        }
        if event == BUTTON_CODE_CHANGE {
            self.account_saved = false;
            self.member_code = None;
            return true;
        }
        if event == BUTTON_LINK_OPEN || event == BUTTON_LINK_COPY {
            if let Some(url) = self.upload_link.clone() {
                self.commands.push(if event == BUTTON_LINK_OPEN { Command::OpenUrl(url) } else { Command::CopyText(url) });
            }
            return false;
        }
        if event == BUTTON_SCAN_DEEP {
            self.settings.scan_deep = !self.settings.scan_deep;
            return true;
        }
        if event == BUTTON_INGAME {
            self.settings.ingame_visible = !self.settings.ingame_visible;
            return true;
        }
        if let Some(widget) = event.strip_prefix(BUTTON_PLACE).and_then(WidgetId::from_key) {
            if let Some(w) = self.widgets.iter().find(|w| w.id() == widget) {
                self.settings.cycle(widget, w.kind());
                return true;
            }
        }
        false
    }

    /// The card's body: one folding section for collecting and uploading the data, or, behind the header's gear,
    /// the display options.
    fn control_rows(&self) -> Vec<Row> {
        let on_off = |on: bool| if on { "켜짐" } else { "꺼짐" }.to_string();
        if self.settings_open {
            let mut rows = vec![Row::Heading("표시 설정".into())];
            rows.push(Row::Button { id: BUTTON_INGAME.into(), label: "인게임 표시".into(), value: on_off(self.settings.ingame_visible), on: self.settings.ingame_visible });
            for w in &self.widgets {
                let p = self.settings.placement(w.id());
                rows.push(Row::Button { id: format!("{BUTTON_PLACE}{}", w.id().key()), label: w.title().into(), value: p.label(w.kind()).into(), on: p != Placement::Off });
            }
            return rows;
        }
        let open = self.settings.student_open;
        let mut rows = vec![Row::Section { id: BUTTON_STUDENT_SECTION.into(), title: "데이터 수집 · 업로드".into(), open, summary: if open { String::new() } else { self.library.age_text(self.now) } }];
        if !open {
            return rows;
        }
        rows.push(Row::Heading("1  학생 정보".into()));
        rows.push(Row::Hint { label: "갱신".into(), value: self.library.age_text(self.now), tip: self.library.exact_text() });
        rows.push(Row::Button { id: BUTTON_SCAN_DEEP.into(), label: "스캔 · 상세 포함".into(), value: on_off(self.settings.scan_deep), on: self.settings.scan_deep });
        rows.push(Row::Button { id: BUTTON_SCAN.into(), label: "학생 정보 스캔".into(), value: "시작".into(), on: false });
        rows.push(Row::Heading("2  진행 상태".into()));
        rows.push(Row::Button { id: BUTTON_PROGRESS_SCAN.into(), label: "진행 상태 스캔".into(), value: "시작".into(), on: false });
        rows.push(Row::Heading("3  업로드".into()));
        if self.account_saved {
            rows.push(Row::Button { id: BUTTON_CODE_CHANGE.into(), label: "계정".into(), value: "저장됨 · 바꾸기".into(), on: false });
        } else {
            rows.push(Row::Info { label: "회원코드".into(), value: self.member_code.clone().unwrap_or_else(|| "붙여넣기 필요".into()) });
            rows.push(Row::Button { id: BUTTON_CODE_PASTE.into(), label: "회원코드 붙여넣기".into(), value: "클립보드".into(), on: false });
        }
        rows.push(Row::Button { id: BUTTON_UPLOAD.into(), label: "스캔 결과 업로드".into(), value: "전송".into(), on: false });
        if let Some((first, rest)) = self.upload_note.split_first() {
            rows.push(Row::Info { label: "결과".into(), value: first.clone() });
            rows.extend(rest.iter().map(|l| Row::Note(l.clone())));
        }
        if self.upload_link.is_some() {
            rows.push(Row::Heading("4  내 페이지".into()));
            rows.push(Row::Button { id: BUTTON_LINK_OPEN.into(), label: "내 페이지".into(), value: "열기".into(), on: false });
            rows.push(Row::Button { id: BUTTON_LINK_COPY.into(), label: "주소".into(), value: "복사".into(), on: false });
        }
        rows
    }

    /// In a battle the add-on keeps out of the way: no in-game card, no icons, only the status and
    /// the two things the player may still need (the in-game switch, closing).
    fn present_battle(&self) {
        let on_off = |on: bool| if on { "켜짐" } else { "꺼짐" }.to_string();
        let rows = vec![
            Row::Info { label: "게임".into(), value: if self.game_present { "연결됨" } else { "대기 중" }.into() },
            Row::Heading("표시".into()),
            Row::Button { id: BUTTON_INGAME.into(), label: "인게임 표시".into(), value: on_off(self.settings.ingame_visible), on: self.settings.ingame_visible },
        ];
        self.send_main(PanelState { scene: Scene::Battle.label().to_string(), title: String::new(), rows, ..Default::default() });
        self.send_ingame(None);
        self.send_marks(Vec::new());
    }

    fn lookup_cached(&self, obs: &Observation) -> Lookup {
        let Some(m) = obs.mission.as_ref().filter(|_| is_mission_screen(obs.scene)) else { return Lookup::None };
        if let Some((cached, found)) = self.looked_up.borrow().as_ref() {
            if cached == m {
                return found.clone();
            }
        }
        let found = lookup(self.wiki, obs);
        *self.looked_up.borrow_mut() = Some((m.clone(), found.clone()));
        found
    }

    fn raid_lookup_cached(&self, obs: &Observation) -> RaidLookup {
        let Some(raid) = obs.raid.as_ref() else { return RaidLookup::None };
        let now = self.now.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
        if let Some((cached, hour, found)) = self.looked_up_raid.borrow().as_ref() {
            if cached == raid && *hour == now / 3600 {
                return found.clone();
            }
        }
        let found = raid_lookup::lookup(self.wiki, raid, now).unwrap_or(RaidLookup::NotFound);
        *self.looked_up_raid.borrow_mut() = Some((raid.clone(), now / 3600, found.clone()));
        found
    }

    fn present(&self) {
        if self.scanning {
            // the card is the scan: how far it is, and that the player's input is locked
            let (title, rows) = scan_rows(&self.scan_state.borrow(), self.input_locked);
            self.send_main(PanelState { scene: "학생 스캔".into(), title, rows, ..Default::default() });
            self.send_ingame(None);
            self.send_marks(Vec::new());
            return;
        }
        if self.last.scene == Scene::Battle {
            self.present_battle();
            return;
        }
        let obs = &self.last;
        let cx = Context { obs, lookup: self.lookup_cached(obs), raid: self.raid_lookup_cached(obs), students: self.students, types: self.types };
        let (mut outside, mut ingame): (Vec<Row>, Vec<Row>) = (Vec::new(), Vec::new());
        let (mut outside_title, mut ingame_title) = (String::new(), String::new());
        let mut marks = Vec::new();
        for w in &self.widgets {
            let placement = self.settings.effective(w.id());
            if placement == Placement::Off {
                continue;
            }
            match (w.render(&cx), placement) {
                (Some(Output::Card { title, rows }), Placement::Outside) => {
                    if outside_title.is_empty() {
                        outside_title = title;
                    }
                    outside.extend(rows);
                }
                (Some(Output::Card { title, rows }), Placement::InGame) => {
                    if ingame_title.is_empty() {
                        ingame_title = title;
                    }
                    ingame.extend(rows);
                }
                (Some(Output::Marks(m)), Placement::InGame) => marks.extend(m),
                _ => {}
            }
        }
        // The main card: first what the add-on sees (the game, then the screen's own information),
        // then what it can do. It is shown whatever the screen, so it can always be used and closed.
        let mut rows = vec![Row::Info { label: "게임".into(), value: if self.game_present { "연결됨" } else { "대기 중" }.into() }];
        rows.extend(outside);
        let icons_wanted = self.widgets.iter().any(|w| w.kind() == Kind::Marks && self.settings.effective(w.id()) != Placement::Off);
        if obs.scene == Scene::LobbyIdle {
            rows.push(Row::Note("메뉴가 숨겨졌습니다. 화면을 한 번 클릭하면 메뉴가 돌아옵니다".into()));
        }
        if obs.grid_without_names() && icons_wanted {
            rows.push(Row::Note("학생 이름이 보이는 보기(목록 오른쪽 위 버튼)에서 아이콘이 표시됩니다".into()));
        }
        rows.extend(self.control_rows());
        if std::env::var_os("BAOCR_DEBUG_ASSIST").is_some() {
            eprintln!("assist: scene={:?} squad={:?} cards={:?} marks={} kind={:?}", obs.scene, obs.squad, obs.cards.iter().map(|c| c.text.as_str()).collect::<Vec<_>>(), marks.len(), obs.grid_kind);
        }
        let scene = if self.game_present { obs.scene.label().to_string() } else { "게임 대기 중".to_string() };
        self.send_main(PanelState { scene, title: outside_title, rows, ..Default::default() });
        self.send_ingame((!ingame.is_empty()).then(|| PanelState { scene: String::new(), title: ingame_title, rows: ingame, ..Default::default() }));
        self.send_marks(if self.view_moving.get() { Vec::new() } else { marks });
        self.presented.set(self.presented.get() + 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use baocr_overlay::Dock;
    use baocr_core::ocr_api::Ocr;
    use baocr_extract::scene::MissionRef;
    use baocr_extract::CardName;
    use baocr_overlay::Mark;
    use rusqlite::Connection;
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::time::Duration;

    struct NoOcr;
    impl Ocr for NoOcr {
        fn boxes(&self, _: &RgbImage) -> anyhow::Result<Vec<TextBox>> {
            anyhow::bail!("no ocr in tests")
        }
        fn line(&self, _: &RgbImage) -> anyhow::Result<(String, f32)> {
            anyhow::bail!("no ocr in tests")
        }
        fn choose(&self, _: &RgbImage, _: &[String]) -> anyhow::Result<(usize, Vec<f32>)> {
            anyhow::bail!("no ocr in tests")
        }
    }

    #[derive(Default)]
    struct Shown {
        main: Option<PanelState>,
        ingame: Option<PanelState>,
        marks: Vec<Mark>,
        clicks: Vec<String>,
        docks: Vec<Dock>,
        locks: Vec<bool>,
    }

    #[derive(Clone, Default)]
    struct Fake(Rc<RefCell<Shown>>);

    impl Surface for Fake {
        fn main_card(&self, card: PanelState) {
            self.0.borrow_mut().main = Some(card);
        }
        fn ingame_card(&self, card: Option<PanelState>) {
            self.0.borrow_mut().ingame = card;
        }
        fn marks(&self, marks: Vec<Mark>) {
            self.0.borrow_mut().marks = marks;
        }
        fn take_events(&self) -> Vec<String> {
            std::mem::take(&mut self.0.borrow_mut().clicks)
        }
        fn set_dock(&self, dock: Dock) {
            self.0.borrow_mut().docks.push(dock);
        }
        fn lock_input(&self, on: bool) -> bool {
            self.0.borrow_mut().locks.push(on);
            on
        }
    }

    fn wiki() -> Wiki {
        let conn = Connection::open_in_memory().unwrap();
        baocr_wikidb::create(&conn).unwrap();
        conn.execute_batch(
            "INSERT INTO source VALUES (1,'t','u','2026-11-23',NULL,'n');
             INSERT INTO label VALUES ('Terrain','Outdoor','야외'),('Terrain','Indoor','실내'),('ArmorType','HeavyArmor','중장갑'),('ArmorType','Unarmed','특수장갑'),('BulletType','Mystic','신비'),('BulletType','Normal','일반'),('BulletType','Explosion','폭발');
             INSERT INTO type_effectiveness VALUES ('Mystic','HeavyArmor',20000),('Pierce','HeavyArmor',5000),('Mystic','Unarmed',20000),('Pierce','Unarmed',10000);
             INSERT INTO enemy VALUES (20,'b','시로&쿠로','Boss','Main','Normal','Unarmed','S',1,'{}'),(21,'b','시로&쿠로','Boss','Main','Explosion','Unarmed','S',1,'{}'),(22,'s','세트의 분노','Boss','Main','Normal','Unarmed','S',1,'{}');
             INSERT INTO raid (id, kind, path_name, name_ko, display_ko, dungeon_type, terrain, released_global, source_id, raw) VALUES
               (3,'Raid','s','시로&쿠로','시로&쿠로',NULL,'[\"Street\",\"Indoor\"]',1,1,'{}'),(1000000,'MultiFloorRaid','set','세트의 분노','세트의 분노',NULL,'[\"Outdoor\"]',1,1,'{}');
             INSERT INTO raid_difficulty (raid_kind, raid_id, idx, label, level, floor_start, battle_duration, boss_enemy_id, released_global) VALUES
               ('Raid',3,0,'Normal',NULL,NULL,180,20,1),('Raid',3,5,'Insane',NULL,NULL,180,21,1),('MultiFloorRaid',1000000,0,NULL,NULL,1,180,22,1);
             INSERT INTO raid_difficulty_enemy VALUES ('Raid',3,0,0,20),('Raid',3,5,0,21),('MultiFloorRaid',1000000,0,0,22);
             INSERT INTO enemy VALUES (1,'a','중장갑병','Minion','Main','Normal','HeavyArmor','S',1,'{}');
             INSERT INTO stage VALUES (10,'Campaign',NULL,19,3,0,'설원 지구 제2 지역','Outdoor',85,NULL,NULL,NULL,1);
             INSERT INTO stage_formation VALUES (100,10,0,'Grade1',1,1);
             INSERT INTO formation_enemy VALUES (100,0,1);
             INSERT INTO formation_enemy_weight VALUES (100,0,1,1,100,100,NULL);
             INSERT INTO raid_difficulty_enemy_weight VALUES ('Raid',3,0,0,17,1,100,100,NULL),('Raid',3,5,0,80,1,100,100,NULL),('MultiFloorRaid',1000000,0,0,25,1,100,100,NULL);
             INSERT INTO student VALUES (1,'a','a','신비학생','','',3,'Main','','','Mystic','HeavyArmor',1,4,1,'',1,1,'{}'),(2,'b','b','관통학생','','',3,'Main','','','Pierce','HeavyArmor',1,0,1,'',1,1,'{}'),
                                        (3,'c','c','스페셜학생','','',3,'Support','','','Mystic','HeavyArmor',1,2,1,'',1,1,'{}');",
        )
        .unwrap();
        Wiki::from_connection(conn)
    }

    struct Rig {
        fake: Fake,
        wiki: Wiki,
        students: Vec<StudentBrief>,
        types: TypeTable,
    }

    fn rig() -> Rig {
        let w = wiki();
        let (students, types) = (w.students().unwrap(), w.type_table().unwrap());
        Rig { fake: Fake::default(), wiki: w, students, types }
    }

    fn mission() -> Option<MissionRef> {
        Some(MissionRef { number: Some((19, 3)), stage_no: None, place: None, name: "설원 지구 제2 지역".into() })
    }

    fn squad_obs(scene: Scene) -> Observation {
        Observation { mission: mission(), squad: vec![1, 2, 3], ..Observation::new(scene) }
    }

    fn raid_obs(scene: Scene, file_mode: baocr_extract::scene::RaidMode, difficulty: Option<usize>, squad: Vec<i64>) -> Observation {
        let boss = if file_mode == baocr_extract::scene::RaidMode::UnrestrictedRaid { "세트의 분노" } else { "시로&쿠로" };
        let raid = baocr_extract::raid::RaidRef { mode: file_mode, boss: boss.into(), difficulty, words: baocr_extract::raid::RaidScreenWords { armors: vec!["특수장갑".into()], attack: None, level: Some(17) }, terrains: vec!["시가지".into(), "실내".into()] };
        Observation { raid: Some(raid), squad, ..Observation::new(scene) }
    }

    fn assistant<'a>(r: &'a Rig, ocr: &'a NoOcr) -> Assistant<'a, Fake> {
        Assistant::new(Observer::new(ocr, &r.students), &r.wiki, &r.students, &r.types, Settings::default(), None, r.fake.clone())
    }

    fn rows_of(p: &PanelState) -> String {
        format!("{:?}", p.rows)
    }

    fn click(r: &Rig, id: &str) {
        r.fake.0.borrow_mut().clicks.push(id.into());
    }

    #[test]
    fn the_main_card_always_says_what_the_game_and_the_add_on_are_doing_and_what_it_can_do() {
        let (r, ocr) = (rig(), NoOcr);
        let mut a = assistant(&r, &ocr);
        a.show(Observation::new(Scene::Lobby));
        let shown = r.fake.0.borrow();
        let main = shown.main.as_ref().unwrap();
        let text = rows_of(main);
        assert_eq!(main.scene, "게임 대기 중", "no game found yet");
        for needed in ["대기 중", "스캔 기록 없음", "scan:start", "scan:deep", "scan:progress", "upload", "section:student"] {
            assert!(text.contains(needed), "{needed} in {text}");
        }
        assert!(!text.contains("place:formation"), "the display options are behind the header's gear: {text}");
        assert_eq!(main.settings.as_ref().map(|s| s.0.as_str()), Some("settings"));
        drop(shown);
        click(&r, "settings");
        a.poll();
        {
            let shown = r.fake.0.borrow();
            let text = rows_of(shown.main.as_ref().unwrap());
            assert!(text.contains("ingame") && text.contains("place:formation") && !text.contains("scan:start"), "{text}");
        }
        click(&r, "settings");
        a.poll();
        a.set_game_present(true);
        let shown = r.fake.0.borrow();
        let main = shown.main.as_ref().unwrap();
        assert_eq!(main.scene, "메인 메뉴");
        assert!(rows_of(main).contains("연결됨"));
    }

    #[test]
    fn by_default_the_mission_and_verdict_are_beside_the_game_and_nothing_is_inside() {
        let (r, ocr) = (rig(), NoOcr);
        let mut a = assistant(&r, &ocr);
        a.set_game_present(true);
        a.show(squad_obs(Scene::Squad));
        let shown = r.fake.0.borrow();
        let main = shown.main.as_ref().unwrap();
        assert_eq!((main.scene.as_str(), main.title.as_str()), ("편성", "19-3 설원 지구 제2 지역"));
        assert!(rows_of(main).contains("야외") && rows_of(main).contains("Verdict"), "{main:?}");
        assert!(shown.ingame.is_none() && shown.marks.is_empty());
    }

    #[test]
    fn the_boss_list_shows_the_encounter_the_difficulty_is_not_chosen_yet_and_no_squad_is_judged() {
        use baocr_extract::scene::RaidMode::MockTotalAssault;
        let (r, ocr) = (rig(), NoOcr);
        let mut a = assistant(&r, &ocr);
        a.set_game_present(true);
        a.show(raid_obs(Scene::MockRaidBosses, MockTotalAssault, None, vec![]));
        let shown = r.fake.0.borrow();
        let main = shown.main.as_ref().unwrap();
        assert_eq!((main.scene.as_str(), main.title.as_str()), ("모의 총력전 보스", "모의 총력전 시로&쿠로"));
        let text = rows_of(main);
        for needed in ["선택 전", "특수장갑", "시가지 · 실내 중 선택", "Lv", "Normal 일반", "Insane 폭발"] {
            assert!(text.contains(needed), "{needed} in {text}");
        }
        assert!(!text.contains("Verdict"), "{text}");
        drop(shown);
        click(&r, "settings");
        a.poll();
        assert!(rows_of(r.fake.0.borrow().main.as_ref().unwrap()).contains("place:raid_info"), "the card can be moved or turned off in the settings");
    }

    #[test]
    fn the_raid_formation_judges_the_party_read_and_says_that_it_is_one_of_several() {
        use baocr_extract::scene::RaidMode::{MockTotalAssault, UnrestrictedRaid};
        let (r, ocr) = (rig(), NoOcr);
        let mut a = assistant(&r, &ocr);
        a.set_game_present(true);
        // difficulty known from the room popup, squad read through the quick formation popup
        a.show(raid_obs(Scene::RaidFormation(MockTotalAssault), MockTotalAssault, Some(0), vec![1, 2, 3]));
        {
            let shown = r.fake.0.borrow();
            let main = shown.main.as_ref().unwrap();
            assert_eq!((main.scene.as_str(), main.title.as_str()), ("모의 총력전 편성", "모의 총력전 시로&쿠로 Normal"));
            let text = rows_of(main);
            assert!(text.contains("Verdict") && text.contains("1부대~4부대") && text.contains("한 부대만"), "{text}");
            assert!(!text.contains("선택 전"), "the difficulty is known: {text}");
        }
        // the formation names no student: nothing to judge, and the card says how to get a reading
        a.show(raid_obs(Scene::RaidFormation(MockTotalAssault), MockTotalAssault, Some(0), vec![]));
        let text = rows_of(r.fake.0.borrow().main.as_ref().unwrap());
        assert!(!text.contains("Verdict") && text.contains("빠른 편성 창을 열면"), "{text}");
        // 제약해제결전 has one party: no talk of four
        a.show(raid_obs(Scene::RaidFormation(UnrestrictedRaid), UnrestrictedRaid, None, vec![1, 2, 3]));
        let text = rows_of(r.fake.0.borrow().main.as_ref().unwrap());
        assert!(text.contains("Verdict") && !text.contains("1부대~4부대"), "{text}");
        // a boss list screen without the difficulty judged against the first one says so
        a.show(raid_obs(Scene::RaidFormation(MockTotalAssault), MockTotalAssault, None, vec![1, 2, 3]));
        assert!(rows_of(r.fake.0.borrow().main.as_ref().unwrap()).contains("Normal 기준으로 판정"));
    }

    #[test]
    fn a_raid_formation_reached_without_the_boss_having_been_seen_says_so_and_a_mission_card_is_not_drawn_for_a_raid() {
        let (r, ocr) = (rig(), NoOcr);
        let mut a = assistant(&r, &ocr);
        a.show(Observation::new(Scene::RaidFormation(baocr_extract::scene::RaidMode::MockTotalAssault)));
        let text = rows_of(r.fake.0.borrow().main.as_ref().unwrap());
        assert!(text.contains("보스 정보를 읽지 못했습니다") && !text.contains("Verdict"), "{text}");
        // the raid card is gone on a mission screen
        a.show(squad_obs(Scene::Squad));
        assert!(!rows_of(r.fake.0.borrow().main.as_ref().unwrap()).contains("선택 전"));
    }

    #[test]
    fn an_unchanged_raid_card_is_not_sent_again() {
        use baocr_extract::scene::RaidMode::MockTotalAssault;
        let (r, ocr) = (rig(), NoOcr);
        let counting = Counting::default();
        let mut a = Assistant::new(Observer::new(&ocr, &r.students), &r.wiki, &r.students, &r.types, Settings::default(), None, &counting);
        for _ in 0..4 {
            a.show(raid_obs(Scene::RaidFormation(MockTotalAssault), MockTotalAssault, Some(0), vec![1, 2, 3]));
        }
        assert_eq!(counting.0.borrow()[0], 1, "one card, shown once");
    }

    #[test]
    fn a_widget_moves_inside_the_game_by_its_button() {
        let (r, ocr) = (rig(), NoOcr);
        let mut a = assistant(&r, &ocr);
        a.show(squad_obs(Scene::Squad));
        click(&r, "place:formation");
        a.poll();
        let shown = r.fake.0.borrow();
        assert!(rows_of(shown.ingame.as_ref().expect("an in-game card")).contains("Verdict"));
        // (the button of the same name stays on the main card, so look for the verdict row itself)
        assert!(!rows_of(shown.main.as_ref().unwrap()).contains("Verdict") && rows_of(shown.main.as_ref().unwrap()).contains("야외"));
    }

    #[test]
    fn the_master_switch_removes_everything_inside_the_game_but_not_the_card_beside_it() {
        let (r, ocr) = (rig(), NoOcr);
        let mut a = assistant(&r, &ocr);
        click(&r, "place:formation");
        a.show(squad_obs(Scene::QuickFormation));
        a.poll();
        assert!(r.fake.0.borrow().ingame.is_some() && !r.fake.0.borrow().marks.is_empty(), "icons and the moved card are inside the game");
        click(&r, "ingame");
        a.poll();
        let shown = r.fake.0.borrow();
        assert!(shown.ingame.is_none() && shown.marks.is_empty());
        assert!(rows_of(shown.main.as_ref().unwrap()).contains("야외"), "the card beside the game stays");
        assert!(!a.settings().ingame_visible);
    }

    #[test]
    fn slot_stats_follow_the_squad_and_icons_can_be_turned_off_one_by_one() {
        let (r, ocr) = (rig(), NoOcr);
        let mut a = assistant(&r, &ocr);
        let mut obs = squad_obs(Scene::QuickFormation);
        obs.cards = vec![CardName { text: "관통학생".into(), x: 629.0, y0: 428.0, bare: false }];
        a.show(obs);
        {
            let shown = r.fake.0.borrow();
            assert_eq!(shown.marks.len(), 4, "{:?}", shown.marks);
            assert!(shown.marks.iter().any(|m| m.cx == 111 && m.top() == 700), "first striker slot");
            assert!(shown.marks.iter().any(|m| m.cx == 490), "first special slot");
        }
        click(&r, "place:slot_stats");
        a.poll();
        assert_eq!(r.fake.0.borrow().marks.len(), 1, "only the grid card's icons are left");
    }

    #[test]
    fn a_grid_without_names_says_how_to_get_icons() {
        let (r, ocr) = (rig(), NoOcr);
        let mut a = assistant(&r, &ocr);
        a.show(Observation::new(Scene::QuickFormation));
        assert!(rows_of(r.fake.0.borrow().main.as_ref().unwrap()).contains("이름이 보이는 보기"));
    }

    #[test]
    fn a_squad_of_one_or_two_gets_no_verdict_and_a_new_mission_does_not_inherit_the_old_squad() {
        let (r, ocr) = (rig(), NoOcr);
        let mut a = assistant(&r, &ocr);
        a.show(Observation { mission: mission(), squad: vec![1, 3], ..Observation::new(Scene::Squad) });
        assert!(!rows_of(r.fake.0.borrow().main.as_ref().unwrap()).contains("Verdict"), "two students are not a squad to judge");
        a.show(squad_obs(Scene::Squad));
        assert!(rows_of(r.fake.0.borrow().main.as_ref().unwrap()).contains("Verdict"));
    }

    #[test]
    fn closing_the_game_forgets_the_mission_so_the_next_session_starts_clean() {
        let (r, ocr) = (rig(), NoOcr);
        let mut a = assistant(&r, &ocr);
        a.show(squad_obs(Scene::Squad));
        assert!(rows_of(r.fake.0.borrow().main.as_ref().unwrap()).contains("야외"));
        a.forget();
        let shown = r.fake.0.borrow();
        let main = shown.main.as_ref().unwrap();
        assert!(main.title.is_empty() && shown.marks.is_empty() && shown.ingame.is_none(), "{main:?}");
        assert!(!rows_of(main).contains("야외"));
    }

    #[test]
    fn a_dragged_card_is_remembered_and_the_saved_place_is_applied_at_start() {
        let (r, ocr) = (rig(), NoOcr);
        let path = std::env::temp_dir().join(format!("baocr-dock-{}.json", std::process::id()));
        let mut settings = Settings::default();
        let left = Dock { anchor: Anchor::Outside(baocr_overlay::Side::Left), presentation: None };
        settings.set_dock(left);
        let mut a = Assistant::new(Observer::new(&ocr, &r.students), &r.wiki, &r.students, &r.types, settings, Some(path.clone()), r.fake.clone());
        assert_eq!(r.fake.0.borrow().docks, vec![left], "the saved place is applied first");
        click(&r, "dock:r");
        assert!(a.poll());
        assert_eq!(a.settings().dock().anchor, Anchor::Outside(baocr_overlay::Side::Right));
        assert_eq!(Settings::load(&path).dock(), a.settings().dock(), "and saved");
        // dropped inside the game, and the pin button: both are remembered, and are independent
        click(&r, "dock:il");
        click(&r, "present:pinned");
        assert!(a.poll());
        let want = Dock { anchor: Anchor::Inside(baocr_overlay::Corner::TopLeft), presentation: Some(Presentation::Pinned) };
        assert_eq!(a.settings().dock(), want);
        assert_eq!(Settings::load(&path).dock(), want);
        click(&r, "present:bookmark");
        assert!(a.poll());
        assert_eq!(a.settings().dock(), Dock { presentation: Some(Presentation::Bookmark), ..want });
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn card_widgets_stay_off_when_set_off_and_unknown_buttons_are_ignored() {
        let (r, ocr) = (rig(), NoOcr);
        let mut a = assistant(&r, &ocr);
        a.show(squad_obs(Scene::Squad));
        for _ in 0..2 {
            click(&r, "place:mission_info");
            a.poll();
        }
        assert!(!rows_of(r.fake.0.borrow().main.as_ref().unwrap()).contains("야외"));
        assert!(r.fake.0.borrow().ingame.is_none());
        click(&r, "nonsense");
        assert!(!a.poll());
    }

    #[test]
    fn a_tick_that_changes_nothing_does_not_work_the_card_out_again() {
        let (r, ocr) = (rig(), NoOcr);
        let mut a = assistant(&r, &ocr);
        a.show(squad_obs(Scene::Squad));
        let now = SystemTime::now();
        a.tick(now);
        let before = a.presented();
        for k in 1..=20 {
            a.tick(now + Duration::from_millis(100 * k));
        }
        assert_eq!(a.presented(), before, "twenty ticks inside one minute draw nothing");
        a.set_library(LibraryInfo { updated: Some(now - Duration::from_secs(120)), count: 3 });
        a.tick(now);
        let after_library = a.presented();
        a.tick(now + Duration::from_secs(61));
        assert_eq!(a.presented(), after_library + 1, "the minute moving on is drawn once");
    }

    #[test]
    fn the_age_of_the_student_list_is_shown_with_its_exact_time_as_a_tooltip_and_moves_on_with_time() {
        let (r, ocr) = (rig(), NoOcr);
        let mut a = assistant(&r, &ocr);
        let now = SystemTime::now();
        a.set_library(LibraryInfo { updated: Some(now - Duration::from_secs(3 * 86_400 + 2 * 3600)), count: 106 });
        a.tick(now);
        let hint = |r: &Rig| {
            r.fake.0.borrow().main.as_ref().unwrap().rows.iter().find_map(|row| match row {
                Row::Hint { value, tip, .. } => Some((value.clone(), tip.clone())),
                _ => None,
            })
        };
        let (value, tip) = hint(&r).expect("a hint row for the student list");
        assert_eq!(value, "3일 2시간 경과 · 106명");
        assert!(tip.starts_with("마지막 스캔\n20"), "{tip}");
        // a day later the card says so without any other event
        a.tick(now + Duration::from_secs(86_400));
        assert_eq!(hint(&r).unwrap().0, "4일 2시간 경과 · 106명");
    }

    #[test]
    fn the_scan_is_started_by_hand_and_its_option_is_kept() {
        let (r, ocr) = (rig(), NoOcr);
        let mut a = assistant(&r, &ocr);
        assert!(a.take_commands().is_empty(), "nothing is scanned until the player asks");
        click(&r, "scan:start");
        a.poll();
        assert_eq!(a.take_commands(), vec![Command::StartScan { deep: false }]);
        click(&r, "scan:deep");
        a.poll();
        click(&r, "scan:start");
        a.poll();
        assert_eq!(a.take_commands(), vec![Command::StartScan { deep: true }]);
        assert!(a.settings().scan_deep);
    }

    #[test]
    fn the_progress_scan_is_its_own_command_and_waits_for_a_running_scan() {
        let (r, ocr) = (rig(), NoOcr);
        let mut a = assistant(&r, &ocr);
        click(&r, "scan:progress");
        a.poll();
        assert_eq!(a.take_commands(), vec![Command::StartProgressScan]);
        // without an account or a code the upload button asks for the code; with one it asks for the upload, once
        click(&r, "upload");
        a.poll();
        assert!(a.take_commands().is_empty());
        assert!(rows_of(r.fake.0.borrow().main.as_ref().unwrap()).contains("회원코드를 먼저"));
        click(&r, "code:paste");
        a.poll();
        assert_eq!(a.take_commands(), vec![Command::PasteMemberCode]);
        a.set_member_code("  AB12CD34EF56G \n");
        assert_eq!(a.member_code(), Some("AB12CD34EF56G"));
        click(&r, "upload");
        click(&r, "upload");
        a.poll();
        assert_eq!(a.take_commands(), vec![Command::Upload]);
        a.begin_upload();
        click(&r, "upload");
        a.poll();
        assert!(a.take_commands().is_empty(), "no second upload while one is under way");
        a.upload_finished("updated (106 students)\nyour page: https://example.test/#/u/x", Some("https://example.test/#/u/x".into()));
        assert!(rows_of(r.fake.0.borrow().main.as_ref().unwrap()).contains("your page"), "the answer stays on the card");
        click(&r, "upload:open");
        click(&r, "upload:copy");
        a.poll();
        assert_eq!(a.take_commands(), vec![Command::OpenUrl("https://example.test/#/u/x".into()), Command::CopyText("https://example.test/#/u/x".into())]);
        a.begin_scan();
        click(&r, "scan:progress");
        a.poll();
        assert!(a.take_commands().is_empty(), "one scan at a time");
    }

    #[test]
    fn during_a_scan_the_card_shows_the_scan_blocks_input_and_says_how_to_get_control_back() {
        let (r, ocr) = (rig(), NoOcr);
        let mut a = assistant(&r, &ocr);
        a.show(squad_obs(Scene::Squad));
        a.begin_scan();
        assert_eq!(r.fake.0.borrow().locks, vec![true], "the player's input is blocked");
        a.scan_progress(BannerState::scanning(5, Some(106)));
        {
            let shown = r.fake.0.borrow();
            let main = shown.main.as_ref().unwrap();
            assert_eq!((main.scene.as_str(), main.title.as_str()), ("학생 스캔", "학생 정보를 스캔하는 중"));
            let text = rows_of(main);
            assert!(text.contains("5/106") && text.contains("ESC") && text.contains("직접 조작"), "{text}");
            assert!(!text.contains("scan:start"), "no second scan, no other controls while it runs");
            assert!(shown.ingame.is_none() && shown.marks.is_empty());
        }
        // a click or a screen change does not bring the normal card back over the scan
        click(&r, "scan:start");
        a.poll();
        a.show(squad_obs(Scene::Squad));
        assert!(a.take_commands().is_empty(), "one scan at a time");
        assert_eq!(r.fake.0.borrow().main.as_ref().unwrap().scene, "학생 스캔");
        // pressing ESC ends the scan: the input comes back first, then the card says why
        a.release_input();
        assert_eq!(r.fake.0.borrow().locks, vec![true, false]);
        a.scan_progress(BannerState::failed(5, Some(106), "ESC로 중단했습니다. 입력 제어권이 돌아왔습니다"));
        assert!(rows_of(r.fake.0.borrow().main.as_ref().unwrap()).contains("제어권이 돌아왔습니다"));
        a.scan_finished(LibraryInfo { updated: Some(SystemTime::now()), count: 106 });
        let shown = r.fake.0.borrow();
        assert_eq!(shown.locks, vec![true, false], "released once, not twice");
        assert!(rows_of(shown.main.as_ref().unwrap()).contains("scan:start"), "the main card is back with the new state of the list");
    }

    #[derive(Default)]
    struct Counting(RefCell<[u32; 3]>);
    impl Surface for &Counting {
        fn main_card(&self, _: PanelState) {
            self.0.borrow_mut()[0] += 1;
        }
        fn ingame_card(&self, _: Option<PanelState>) {
            self.0.borrow_mut()[1] += 1;
        }
        fn marks(&self, _: Vec<Mark>) {
            self.0.borrow_mut()[2] += 1;
        }
        fn take_events(&self) -> Vec<String> {
            Vec::new()
        }
        fn set_dock(&self, _: Dock) {}
        fn lock_input(&self, _: bool) -> bool {
            false
        }
    }

    #[test]
    fn an_unchanged_output_is_not_sent_to_the_surface_again() {
        let (r, ocr) = (rig(), NoOcr);
        let counting = Counting::default();
        let mut a = Assistant::new(Observer::new(&ocr, &r.students), &r.wiki, &r.students, &r.types, Settings::default(), None, &counting);
        for _ in 0..5 {
            a.show(squad_obs(Scene::Squad));
            a.transition();
            a.show(squad_obs(Scene::Squad));
        }
        assert_eq!(*counting.0.borrow(), [1, 1, 1], "the first output only: the icons were empty all along");
        a.show(squad_obs(Scene::QuickFormation));
        assert_eq!(counting.0.borrow()[0], 2, "a different card is sent");
    }

    #[test]
    fn in_a_battle_only_the_status_the_switch_and_closing_are_left() {
        let (r, ocr) = (rig(), NoOcr);
        let mut a = assistant(&r, &ocr);
        a.set_game_present(true);
        click(&r, "place:formation");
        a.show(squad_obs(Scene::QuickFormation));
        a.poll();
        assert!(r.fake.0.borrow().ingame.is_some() && !r.fake.0.borrow().marks.is_empty());
        a.show(Observation::new(Scene::Battle));
        {
            let shown = r.fake.0.borrow();
            let main = shown.main.as_ref().unwrap();
            assert_eq!(main.scene, "전투 중");
            let text = rows_of(main);
            assert!(text.contains("ingame") && text.contains("연결됨"), "{text}");
            assert!(!text.contains("scan:start") && !text.contains("place:"), "nothing that reads or drives the game: {text}");
            assert!(shown.ingame.is_none() && shown.marks.is_empty(), "no stale cards or icons");
        }
        click(&r, "quit");
        a.poll();
        assert_eq!(a.take_commands(), vec![Command::Quit], "closing works in a battle");
        click(&r, "ingame");
        a.poll();
        assert!(!a.settings().ingame_visible, "and so does the switch");
        // leaving the battle: the next screen starts clean, nothing of the old one comes back
        a.show(Observation::new(Scene::Unknown));
        let shown = r.fake.0.borrow();
        assert!(rows_of(shown.main.as_ref().unwrap()).contains("scan:start"));
        assert!(shown.ingame.is_none() && shown.marks.is_empty());
    }

    #[test]
    fn the_student_section_folds_and_unfolds_and_the_close_button_is_in_the_header() {
        let (r, ocr) = (rig(), NoOcr);
        let mut a = assistant(&r, &ocr);
        a.show(Observation::new(Scene::Lobby));
        {
            let shown = r.fake.0.borrow();
            let main = shown.main.as_ref().unwrap();
            assert_eq!(main.close.as_ref().map(|c| c.0.as_str()), Some("quit"), "closing is the header icon");
            let text = rows_of(main);
            assert!(text.contains("scan:start") && !text.contains("애드온 종료"), "{text}");
        }
        click(&r, "section:student");
        a.poll();
        {
            let shown = r.fake.0.borrow();
            let main = shown.main.as_ref().unwrap();
            let text = rows_of(main);
            assert!(!text.contains("scan:start") && !text.contains("scan:deep"), "folded: no scan controls, {text}");
            assert!(text.contains("section:student") && !text.contains("ingame"), "only the section header stays: {text}");
            let summary = main.rows.iter().find_map(|row| match row {
                Row::Section { summary, open: false, .. } => Some(summary.clone()),
                _ => None,
            });
            assert_eq!(summary.as_deref(), Some("스캔 기록 없음"), "a folded section still says how stale the list is");
            assert_eq!(main.close.as_ref().map(|c| c.0.as_str()), Some("quit"));
        }
        assert!(!a.settings().student_open);
        click(&r, "section:student");
        a.poll();
        assert!(rows_of(r.fake.0.borrow().main.as_ref().unwrap()).contains("scan:start"));
        // the close icon reports quit through the same command as before
        click(&r, "quit");
        a.poll();
        assert_eq!(a.take_commands(), vec![Command::Quit]);
    }

    #[test]
    fn icons_of_a_view_that_is_gone_do_not_come_back_before_the_next_view_is_read() {
        let (r, ocr) = (rig(), NoOcr);
        let mut a = assistant(&r, &ocr);
        let mut obs = squad_obs(Scene::QuickFormation);
        obs.cards = vec![CardName { text: "관통학생".into(), x: 629.0, y0: 428.0, bare: false }];
        a.show(obs.clone());
        assert!(!r.fake.0.borrow().marks.is_empty(), "icons while the popup is read");
        // the popup closes: the screen moves and the icons come down
        a.transition();
        assert!(r.fake.0.borrow().marks.is_empty());
        // anything that redraws the card from the last reading (a click, the list's age moving on)
        // must not bring them back
        click(&r, "ingame");
        click(&r, "ingame");
        a.poll();
        a.tick(SystemTime::now() + Duration::from_secs(3600));
        assert!(r.fake.0.borrow().marks.is_empty(), "stale icons were put back");
        // the new view is read: the popup is gone, still no icons
        a.show(squad_obs(Scene::Squad));
        assert!(r.fake.0.borrow().marks.is_empty());
        // opening the popup again brings them back
        a.show(obs);
        assert!(!r.fake.0.borrow().marks.is_empty());
        // a battle clears them whatever came before
        a.show(Observation::new(Scene::Battle));
        assert!(r.fake.0.borrow().marks.is_empty() && r.fake.0.borrow().ingame.is_none());
        a.transition();
        a.tick(SystemTime::now() + Duration::from_secs(7200));
        assert!(r.fake.0.borrow().marks.is_empty());
    }

    #[test]
    fn the_card_and_the_tray_can_both_close_the_add_on() {
        let (r, ocr) = (rig(), NoOcr);
        let mut a = assistant(&r, &ocr);
        click(&r, "quit");
        a.poll();
        assert_eq!(a.take_commands(), vec![Command::Quit]);
        // asking twice does not stack requests
        click(&r, "quit");
        click(&r, "quit");
        a.poll();
        assert_eq!(a.take_commands(), vec![Command::Quit]);
    }
}
