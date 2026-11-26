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

    /// See [`Observer::grid_state`].
    pub fn grid_state(&mut self, img: &RgbImage) -> crate::observe::GridState {
        self.observer.grid_state(img)
    }

    /// See [`Observer::unread_band`].
    pub fn unread_band(&self, img: &RgbImage) -> Option<(u32, u32, u32, u32)> {
        self.observer.unread_band(img)
    }

    /// See [`Observer::in_popup`].
    pub fn in_popup(&self) -> bool {
        self.observer.in_popup()
    }

    /// See [`Observer::striker_popup`].
    pub fn striker_popup(&self, img: &RgbImage) -> bool {
        self.observer.striker_popup(img)
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
}

mod controls;
mod present;
#[cfg(test)]
mod tests;
