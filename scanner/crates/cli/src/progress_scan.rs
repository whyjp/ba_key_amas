//! The progress scan: walks the screens that hold the account's progress and reads them with the parsers of
//! `baocr-progress`. Like the student scan it is the only code that sends input, and only because the player
//! asked for it.
//!
//! Where to click is found by reading the screen, never from coordinates, and only the labels listed in
//! `Target` are ever clicked: tabs, the list buttons, the home icon. Nothing that claims a reward, enters a
//! stage or confirms a dialog is on that list. A section that cannot be reached is left empty with a warning;
//! the sections that were read are still written.
use anyhow::{anyhow, bail, Result};
use baocr_core::ocr_api::{Ocr, TextBox};
use baocr_extract::scene::{classify_in, Scene};
use baocr_hcu::{detect_layout, game_capture, settle, settle_quiet, to_frame, Exclusive, Layout};
use baocr_overlay::BannerState;
use baocr_progress::schema::{Events, PastEvent, ProgressSnapshot, Region};
use baocr_progress::schema::StoryVolume;
use baocr_progress::schema::{Duty, DutyStudent, UnreadStudent};
use baocr_progress::{account, bareceipt, duty, event_past, momotalk, schedule, scroll, story, Rect};
use image::RgbImage;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

/// Everything the scan is allowed to click, by the text OCR reads on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Target {
    Schedule,
    EventList,
    EventStoryTab,
    WorkHub,
    WorkStory,
    MainStory,
    /// From a volume screen back to the volume map.
    ToVolumeMap,
    /// The button that switches the volume map between its two parts.
    PartSwitch,
    FinalVolume,
    Momotalk,
}

impl Target {
    /// Where to click if this box is the label, and where on the screen it may be.
    fn locate(self, b: &TextBox, frame_h: f32) -> Option<(f32, f32)> {
        let t: String = b.text.chars().filter(|c| !c.is_whitespace()).collect();
        let centre = ((b.x0 + b.x1) / 2.0, (b.y0 + b.y1) / 2.0);
        match self {
            // the lobby's bottom bar; OCR reads the label both ways
            Target::Schedule => (b.y0 > frame_h - 100.0 && (t == "스케줄" || t == "스케쥴")).then_some(centre),
            // the label under the calendar icon is a picture-like text OCR often misses; the clock next to
            // it is read reliably, and the icon sits just above it
            Target::EventList => {
                let right_bottom = b.x0 > 900.0 && b.y0 > frame_h * 0.6;
                if right_bottom && t.contains("이벤트일람") {
                    Some((centre.0, b.y0 - 30.0))
                } else {
                    (right_bottom && is_clock(&t)).then_some((centre.0 - 10.0, b.y0 - 55.0))
                }
            }
            Target::EventStoryTab => (b.x1 < 330.0 && t == "이벤트스토리").then_some(centre),
            Target::WorkHub => (b.x0 > 1100.0 && b.y0 > frame_h - 100.0 && t == "업무").then_some(centre),
            Target::WorkStory => (b.x0 > 1000.0 && b.y0 > 150.0 && t == "스토리").then_some(centre),
            Target::MainStory => (b.x1 < 600.0 && b.y0 > 200.0 && t.contains("메인스토리")).then_some(centre),
            Target::ToVolumeMap => (b.y1 < 130.0 && b.x1 < 330.0 && t.ends_with("메인스토리로이동")).then_some(centre),
            Target::PartSwitch => (b.y1 < 130.0 && b.x1 < 330.0 && t.ends_with("부로이동")).then_some(centre),
            Target::Momotalk => (b.x1 < 250.0 && b.y0 > 140.0 && b.y1 < 220.0 && t == "모모톡").then_some(centre),
            Target::FinalVolume => (b.x0 > 700.0 && b.x1 < 950.0 && b.y0 > frame_h * 0.6 && t == "최종편").then_some(centre),
        }
    }
}

/// `AM7:32`, `PM 12:05`.
fn is_clock(t: &str) -> bool {
    let rest = t.strip_prefix("AM").or_else(|| t.strip_prefix("PM"));
    rest.is_some_and(|r| r.split_once(':').is_some_and(|(h, m)| (1..=2).contains(&h.len()) && m.len() == 2 && h.bytes().chain(m.bytes()).all(|c| c.is_ascii_digit())))
}

/// One look at the game: the frame, what OCR read on it and which screen that is.
struct Look {
    img: RgbImage,
    boxes: Vec<TextBox>,
    scene: Scene,
}

struct Nav<'a> {
    ocr: &'a dyn Ocr,
    hcu: &'a Exclusive<'a>,
    region: [f64; 4],
    layout: Layout,
}

impl Nav<'_> {
    fn frame_h(&self) -> f32 {
        self.layout.frame_height() as f32
    }

    fn read(&self, img: RgbImage) -> Result<Look> {
        // BAOCR_PROGRESS_FRAMES=<dir> keeps every screen this scan reads, to study the OCR offline (`--boxes`)
        if let Some(dir) = std::env::var_os("BAOCR_PROGRESS_FRAMES") {
            static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
            let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let _ = std::fs::create_dir_all(&dir);
            let _ = img.save(std::path::Path::new(&dir).join(format!("frame-{n:03}.png")));
        }
        let boxes = self.ocr.boxes(&img)?;
        let scene = classify_in(&boxes, None, self.layout);
        Ok(Look { img, boxes, scene })
    }

    fn look(&self) -> Result<Look> {
        self.read(game_capture(self.hcu, self.region)?)
    }

    fn click_at(&self, x: f32, y: f32) -> Result<()> {
        let (fx, fy) = to_frame(self.region, x.round() as i32, y.round() as i32);
        self.hcu.click(fx, fy)
    }

    /// Click a listed label if it is on the screen, and wait for the screen to change and hold still.
    fn press(&self, from: &Look, target: Target) -> Result<Option<Look>> {
        let Some((x, y)) = from.boxes.iter().find_map(|b| target.locate(b, self.frame_h())) else {
            return Ok(None);
        };
        // a button pressed while its screen is still sliding in is ignored: press once more if nothing happened
        let mut shown = None;
        for attempt in 0..2 {
            if std::env::var_os("BAOCR_DEBUG_PROGRESS").is_some() {
                eprintln!("progress: pressing {target:?} at ({x:.0},{y:.0}) on {:?}, attempt {attempt}", from.scene);
            }
            self.click_at(x, y)?;
            match settle(self.hcu, self.region, &from.img) {
                Ok(img) => {
                    shown = Some(img);
                    break;
                }
                Err(e) if attempt == 1 => return Err(e),
                Err(_) => std::thread::sleep(Duration::from_millis(800)),
            }
        }
        let mut look = self.read(shown.expect("two attempts end in a screen or an error"))?;
        // a screen caught while it is still fading in reads as nothing known: look again before giving up
        for _ in 0..3 {
            if look.scene != Scene::Unknown {
                break;
            }
            std::thread::sleep(Duration::from_millis(800));
            look = self.look()?;
        }
        Ok(Some(look))
    }

    /// Click a tab. The game opens a screen on the tab that was open last, so the tab may already be the one
    /// showing and the click changes nothing: the screen is only waited for to hold still.
    fn tab(&self, from: &Look, target: Target) -> Result<Look> {
        let (x, y) = from.boxes.iter().find_map(|b| target.locate(b, self.frame_h())).ok_or_else(|| anyhow!("{target:?}: 탭을 찾지 못했습니다"))?;
        self.click_at(x, y)?;
        std::thread::sleep(Duration::from_millis(700));
        self.settled()
    }

    /// The home icon in the top right corner of every screen that has a header.
    fn home(&self) -> Result<()> {
        self.click_at(1235.0, 22.0)?;
        std::thread::sleep(Duration::from_millis(1500));
        Ok(())
    }

    fn to_lobby(&self) -> Result<Look> {
        for _ in 0..4 {
            let look = self.look()?;
            match look.scene {
                Scene::Lobby => {
                    // the lobby's buttons answer a moment after it appears
                    std::thread::sleep(Duration::from_millis(1000));
                    return self.look();
                }
                // art only: a click anywhere brings the menus back
                Scene::LobbyIdle => self.click_at(640.0, self.frame_h() / 2.0)?,
                _ => self.home()?,
            }
            std::thread::sleep(Duration::from_millis(1200));
        }
        bail!("메인 메뉴로 돌아오지 못했습니다")
    }

    /// Wheel movements over a list: down is positive, as in the student scan.
    fn scroll_list(&self, x: f32, y: f32, down: bool, moves: usize) -> Result<()> {
        let (fx, fy) = to_frame(self.region, x as i32, y as i32);
        self.hcu.scroll_n(fx, fy, if down { 6 } else { -12 }, moves)
    }

    /// The screen once it holds still. Behind a popup the lobby keeps animating and never does: after the wait
    /// runs out, the latest look is taken as it is.
    fn settled(&self) -> Result<Look> {
        match settle_quiet(self.hcu, self.region) {
            Ok(img) => self.read(img),
            Err(e) if format!("{e:#}").contains("did not settle") => self.look(),
            Err(e) => Err(e),
        }
    }
}

/// What the scan did, for the log and the banner.
#[derive(Debug)]
pub struct ProgressSummary {
    pub sections_read: usize,
    pub warnings: Vec<String>,
    pub file: std::path::PathBuf,
}

type Section = fn(&Nav, &Look, &mut ProgressSnapshot) -> Result<()>;

/// The lobby read counts as the first section.
const SECTIONS: usize = 6;

pub fn scan_progress(ocr: Arc<dyn Ocr>, hcu: &Exclusive, region: [f64; 4], out: &Path, report: &dyn Fn(BannerState)) -> Result<ProgressSummary> {
    let layout = detect_layout(region).ok_or_else(|| anyhow!("게임 창의 비율을 알 수 없습니다"))?;
    let nav = Nav { ocr: &*ocr, hcu, region, layout };
    let mut snap = ProgressSnapshot::new(chrono::Local::now().to_rfc3339(), layout.label());
    report(BannerState::scanning(0, Some(SECTIONS)));

    let lobby = nav.to_lobby().inspect_err(|_| report(BannerState::failed(0, Some(SECTIONS), "메인 메뉴에서 시작해 주세요")))?;
    let acct = account::parse_lobby(&lobby.boxes);
    snap.account = acct.value;
    snap.warnings.extend(acct.warnings);

    let mut read = 1;
    let steps: [(&str, &str, Section); 5] = [
        ("schedule", "스케줄", schedule_section),
        ("events", "이벤트 스토리", past_events_section),
        ("momotalk", "모모톡 안 읽음", momotalk_unread_section),
        ("duty", "당번 선택", duty_section),
        ("story", "메인 스토리", main_story_section),
    ];
    // BAOCR_PROGRESS_ONLY=momotalk,duty runs only those sections (for checking one without walking the rest)
    let only: Option<Vec<String>> = std::env::var("BAOCR_PROGRESS_ONLY").ok().map(|v| v.split(',').map(|s| s.trim().to_string()).collect());
    for (key, name, step) in steps {
        if only.as_ref().is_some_and(|o| !o.iter().any(|k| k == key)) {
            continue;
        }
        report(BannerState::scanning(read, Some(SECTIONS)));
        let lobby = nav.to_lobby()?;
        match step(&nav, &lobby, &mut snap) {
            Ok(()) => read += 1,
            Err(e) => {
                // a cancel by the player is not a missed section: it ends the scan at once
                if hcu.cancel_token().load(std::sync::atomic::Ordering::SeqCst) {
                    return Err(e);
                }
                snap.warnings.push(format!("{name}: {e:#}"));
            }
        }
    }
    let _ = nav.to_lobby();

    std::fs::create_dir_all(out)?;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let text = serde_json::to_string_pretty(&snap)?;
    let file = if only.is_some() {
        // a partial read must not replace the last full one
        let file = out.join(format!("partial-{stamp}.json"));
        std::fs::write(&file, &text)?;
        file
    } else {
        let file = out.join(format!("progress-{stamp}.json"));
        std::fs::write(&file, &text)?;
        std::fs::write(out.join("latest.json"), &text)?;
        let backup = bareceipt::export(&snap, &[]);
        std::fs::write(out.join("bareceipt-backup.json"), bareceipt::to_file_text(&backup.value))?;
        file
    };
    report(BannerState::finished(read, Some(SECTIONS)));
    Ok(ProgressSummary { sections_read: read, warnings: snap.warnings, file })
}

/// 스케줄: the region ranks and their total, merged across the scrolled pages by region name.
fn schedule_section(nav: &Nav, lobby: &Look, snap: &mut ProgressSnapshot) -> Result<()> {
    let first = nav.press(lobby, Target::Schedule)?.ok_or_else(|| anyhow!("로비에서 스케줄 버튼을 찾지 못했습니다"))?;
    if first.scene != Scene::Schedule {
        bail!("스케줄 화면이 열리지 않았습니다 ({:?})", first.scene);
    }
    let (x, y) = (950.0, nav.frame_h() * 0.6);
    let list = Rect { x0: 640, y0: 150, x1: 1270, y1: nav.frame_h() as i32 - 20 };
    nav.scroll_list(x, y, false, 6)?;
    let mut look = nav.settled()?;
    let mut regions: BTreeMap<String, Region> = BTreeMap::new();
    let mut total = None;
    for _ in 0..14 {
        let page = schedule::parse_schedule(&look.boxes);
        for r in page.value.regions {
            regions.insert(squash(&r.name), r);
        }
        total = total.or(page.value.total_rank);
        nav.scroll_list(x, y, true, 2)?;
        let next = nav.settled()?;
        let moved = scroll::content_shift(&look.img, &next.img, list, 500);
        look = next;
        if moved == Some(0) {
            break;
        }
    }
    for r in schedule::parse_schedule(&look.boxes).value.regions {
        regions.insert(squash(&r.name), r);
    }
    snap.schedule.regions = regions.into_values().collect();
    snap.schedule.total_rank = total;
    if snap.schedule.regions.is_empty() {
        bail!("지역 줄을 읽지 못했습니다");
    }
    Ok(())
}

/// 이벤트 일람, 이벤트 스토리 탭: the stamps of the always-open past events, numbered by their place in the list.
fn past_events_section(nav: &Nav, lobby: &Look, snap: &mut ProgressSnapshot) -> Result<()> {
    let list = nav.press(lobby, Target::EventList)?.ok_or_else(|| anyhow!("로비에서 이벤트 일람 버튼을 찾지 못했습니다"))?;
    if list.scene != Scene::EventList {
        bail!("이벤트 일람이 열리지 않았습니다 ({:?})", list.scene);
    }
    nav.tab(&list, Target::EventStoryTab)?;
    let (x, y) = (900.0, nav.frame_h() * 0.6);
    nav.scroll_list(x, y, false, 12)?;
    let mut look = nav.settled()?;

    let cells = event_past::read_grid(&look.img);
    let cols = cells.iter().map(|c| c.col + 1).max().unwrap_or(0) as u32;
    let rows: Vec<i32> = cells.iter().filter(|c| c.col == 0).map(|c| c.rect.y0).collect();
    if cols == 0 || rows.len() < 2 {
        bail!("이벤트 카드 격자를 읽지 못했습니다");
    }
    let (top, pitch) = (rows[0], rows[1] - rows[0]);
    let area = Rect { x0: (0.40 * 1280.0) as i32, y0: (0.15 * nav.frame_h()) as i32, x1: 1280, y1: nav.frame_h() as i32 };
    let mut found: BTreeMap<u32, bool> = BTreeMap::new();
    let mut scrolled = 0;
    for _ in 0..30 {
        // the stamps shimmer when a list opens: a card counts as stamped if either of two looks saw it
        let again = nav.settled().map(|l| event_past::read_grid(&l.img)).unwrap_or_default();
        for c in event_past::read_grid(&look.img) {
            let row = ((c.rect.y0 + scrolled - top) as f32 / pitch as f32).round() as i32;
            let also = again.iter().any(|a| a.row == c.row && a.col == c.col && a.complete);
            if row >= 0 {
                let idx = row as u32 * cols + c.col as u32;
                *found.entry(idx).or_insert(false) |= c.complete || also;
            }
        }
        nav.scroll_list(x, y, true, 1)?;
        let next = nav.settled()?;
        let shift = scroll::content_shift(&look.img, &next.img, area, 700);
        if std::env::var_os("BAOCR_DEBUG_PROGRESS").is_some() {
            eprintln!("progress: list moved {shift:?} px");
        }
        match shift {
            Some(0) => break,
            Some(d) => scrolled += d,
            None => {
                snap.warnings.push("이벤트 스토리: 목록이 얼마나 움직였는지 재지 못해 읽기를 멈췄습니다".into());
                break;
            }
        }
        look = next;
    }
    snap.events = Events {
        open: snap.events.open.take(),
        past: found.into_iter().map(|(index, cleared)| PastEvent { index, cleared: Some(cleared), scanned_while_open: false }).collect(),
    };
    Ok(())
}

/// 업무, 스토리, 1부 메인 스토리: the volume map in both of its parts, and the final volume. A volume is cleared
/// when its card says so. Selecting a card hides its badge, so each part is read twice with another card selected.
fn main_story_section(nav: &Nav, lobby: &Look, snap: &mut ProgressSnapshot) -> Result<()> {
    let hub = nav.press(lobby, Target::WorkHub)?.ok_or_else(|| anyhow!("로비에서 업무 버튼을 찾지 못했습니다"))?;
    let hub = nav.press(&hub, Target::WorkStory)?.ok_or_else(|| anyhow!("업무 화면에서 스토리 버튼을 찾지 못했습니다"))?;
    let mut look = nav.press(&hub, Target::MainStory)?.ok_or_else(|| anyhow!("스토리 화면에서 1부 메인 스토리 버튼을 찾지 못했습니다"))?;
    let mut warnings = Vec::new();
    let mut volumes: Vec<StoryVolume> = Vec::new();
    let merge = |cards: baocr_progress::Read<Vec<story::VolumeCard>>, volumes: &mut Vec<StoryVolume>| {
        for c in cards.value {
            match volumes.iter_mut().find(|v| v.volume.eq_ignore_ascii_case(&c.volume.volume)) {
                Some(v) => v.cleared = v.cleared.or(c.volume.cleared),
                None => volumes.push(c.volume),
            }
        }
        cards.warnings
    };

    // it opens on the volume that was open last: a single volume's screen shows its badge and chapters
    let single = story::parse_main_volume(&look.boxes).value;
    if let Some(map) = nav.press(&look, Target::ToVolumeMap)? {
        volumes.extend(single);
        look = map;
    }
    for _ in 0..2 {
        // two passes: the second starts with a different card selected, so the badge the first one hid shows
        for _pass in 0..2 {
            // the card selected now has no badge in view: read what is shown, then select a card whose state is
            // known, so the sweep below sees the badge of the one that was selected
            let first = story::parse_volume_cards(&look.boxes);
            let known = first.value.iter().find(|c| c.volume.cleared.is_some()).map(|c| c.plate);
            warnings.extend(merge(first, &mut volumes).into_iter().filter(|w| !w.contains("no badge")));
            if let Some((x, y)) = known {
                nav.click_at(x, y)?;
                std::thread::sleep(Duration::from_millis(1600));
            }
            // the wheel moves the map sideways: sweep it from its left end to its right end, reading every view
            let (mx, my) = (640.0, nav.frame_h() * 0.62);
            nav.scroll_list(mx, my, false, 8)?;
            std::thread::sleep(Duration::from_millis(1500));
            let mut quiet = 0;
            for _ in 0..12 {
                look = nav.look()?;
                if std::env::var_os("BAOCR_DEBUG_PROGRESS").is_some() {
                    let seen: Vec<_> = story::parse_volume_cards(&look.boxes).value.into_iter().map(|c| (c.volume.volume, c.volume.cleared)).collect();
                    eprintln!("progress: map view {seen:?}");
                }
                let before = volumes.len();
                warnings.extend(merge(story::parse_volume_cards(&look.boxes), &mut volumes).into_iter().filter(|w| !w.contains("no badge")));
                quiet = if volumes.len() == before { quiet + 1 } else { 0 };
                if quiet >= 2 {
                    break;
                }
                nav.scroll_list(mx, my, true, 1)?;
                std::thread::sleep(Duration::from_millis(1500));
            }
            // the selected card shows its synopsis instead of its badge: select another card and read again
            let cards = story::parse_volume_cards(&look.boxes);
            if let Some(c) = cards.value.iter().find(|c| c.volume.cleared.is_some()) {
                nav.click_at(c.plate.0, c.plate.1)?;
                std::thread::sleep(Duration::from_millis(1600));
                look = nav.look()?;
                warnings.extend(merge(story::parse_volume_cards(&look.boxes), &mut volumes).into_iter().filter(|w| !w.contains("no badge")));
            }
        }
        if let Some(final_view) = nav.press(&look, Target::FinalVolume)? {
            if let Some(v) = story::parse_main_volume(&final_view.boxes).value {
                if !volumes.iter().any(|k| k.volume.eq_ignore_ascii_case(&v.volume)) {
                    volumes.push(v);
                }
            }
            look = nav.press(&final_view, Target::ToVolumeMap)?.ok_or_else(|| anyhow!("볼륨 지도로 돌아오지 못했습니다"))?;
        }
        look = nav.press(&look, Target::PartSwitch)?.ok_or_else(|| anyhow!("부 전환 버튼을 찾지 못했습니다"))?;
    }
    for v in &volumes {
        if v.cleared.is_none() {
            warnings.push(format!("메인 스토리: {} 의 클리어 여부를 화면에서 읽지 못했습니다", v.volume));
        }
    }
    snap.story.main = volumes;
    snap.warnings.extend(warnings);
    Ok(())
}

/// The × of a popup: the box at the right end of the popup's title row.
fn close_button(look: &Look, title: &TextBox) -> Option<(f32, f32)> {
    look.boxes
        .iter()
        .find(|b| {
            let t = b.text.trim();
            (t == "×" || t == "X" || t == "x") && (((b.y0 + b.y1) / 2.0) - ((title.y0 + title.y1) / 2.0)).abs() < 30.0 && b.x0 > title.x1 + 100.0
        })
        .map(|b| ((b.x0 + b.x1) / 2.0, (b.y0 + b.y1) / 2.0))
}

/// 모모톡, messages tab: the students with unread messages and how many each. The tab already lists only the
/// unread ones, sorted by 안읽음; its header carries the total, which the rows must add up to.
fn momotalk_unread_section(nav: &Nav, lobby: &Look, snap: &mut ProgressSnapshot) -> Result<()> {
    let popup = nav.press(lobby, Target::Momotalk)?.ok_or_else(|| anyhow!("로비에서 모모톡 버튼을 찾지 못했습니다"))?;
    let title = popup.boxes.iter().find(|b| b.text.contains("MomoTalk")).cloned().ok_or_else(|| anyhow!("모모톡 창이 열리지 않았습니다"))?;
    // the messages tab is the second icon of the left rail, under the students tab
    nav.click_at(title.x0 + 22.0, title.y1 + 138.0)?;
    std::thread::sleep(Duration::from_millis(900));
    // the list keeps the place it was left at: start from its top
    nav.scroll_list(title.x0 + 290.0, title.y1 + 260.0, false, 6)?;
    std::thread::sleep(Duration::from_millis(700));
    let mut look = nav.settled()?;
    let mut rows: Vec<UnreadStudent> = Vec::new();
    let mut total = None;
    // a read that does not add up to the header's total is repeated from the top of the list
    for pass in 0..3 {
        if pass > 0 {
            nav.scroll_list(title.x0 + 290.0, title.y1 + 260.0, false, 6)?;
            std::thread::sleep(Duration::from_millis(900));
            look = nav.settled()?;
        }
        let mut quiet = 0;
        for _ in 0..16 {
            let page = momotalk::parse_unread(&look.boxes);
            if page.value.total.is_none() && total.is_none() {
                bail!("안 읽은 메시지 목록이 열리지 않았습니다");
            }
            total = total.or(page.value.total);
            let before = rows.len();
            for mut r in page.value.rows {
                r.name = canonical_name(&r.name);
                if !rows.iter().any(|k| k.name == r.name) {
                    rows.push(r);
                }
            }
            quiet = if rows.len() == before { quiet + 1 } else { 0 };
            if quiet >= 2 {
                break;
            }
            nav.scroll_list(title.x0 + 290.0, title.y1 + 260.0, true, 1)?;
            std::thread::sleep(Duration::from_millis(700));
            look = nav.settled()?;
        }
        if total.is_some_and(|t| t == rows.iter().map(|r| r.count).sum::<u32>()) {
            break;
        }
    }
    if let Some((x, y)) = close_button(&look, &title) {
        nav.click_at(x, y)?;
        std::thread::sleep(Duration::from_millis(800));
    }
    let sum: u32 = rows.iter().map(|r| r.count).sum();
    if total.is_some_and(|t| t != sum) {
        snap.warnings.push(format!("모모톡: 안 읽은 메시지 합계 {} 와 읽은 줄의 합 {} 가 다릅니다", total.unwrap_or(0), sum));
    }
    snap.momotalk.unread_total = total;
    snap.momotalk.unread = rows;
    Ok(())
}

/// The duty student menu (the icon at the top right of the lobby): who is picked and each student's memorial
/// lobby. It is closed with its ×, never with 취소 or 확인, so nothing is changed.
fn duty_section(nav: &Nav, lobby: &Look, snap: &mut ProgressSnapshot) -> Result<()> {
    // the one unlabelled icon this scan presses: the top bar is laid out the same in both screen layouts
    let _ = lobby;
    let mut opened = None;
    for _ in 0..3 {
        nav.click_at(1075.0, 37.0)?;
        std::thread::sleep(Duration::from_millis(1800));
        let look = nav.look()?;
        if let Some(title) = look.boxes.iter().find(|b| b.text.replace(' ', "") == "당번선택").cloned() {
            opened = Some((look, title));
            break;
        }
    }
    let Some((mut look, title)) = opened else {
        bail!("당번 선택 창이 열리지 않았습니다");
    };
    let first = duty::parse_duty(&look.boxes, &look.img);
    let (gx, gy) = (title.x0 + 150.0, title.y1 + 260.0);
    nav.scroll_list(gx, gy, false, 8)?;
    std::thread::sleep(Duration::from_millis(800));
    look = nav.settled()?;
    let mut cards: BTreeMap<String, DutyStudent> = BTreeMap::new();
    let owned = owned_count();
    let (mut counted, mut limit) = (first.value.selected_count, first.value.limit);
    // one pass down the whole list; a pass that read fewer students than the account has is repeated from the top
    for pass in 0..3 {
    let mut quiet = 0;
    for _page in 0..60 {
        reread_plates(nav.ocr, &mut look);
        let page = duty::parse_duty(&look.boxes, &look.img);
        counted = page.value.selected_count.or(counted);
        limit = page.value.limit.or(limit);
        let before = cards.len();
        for mut c in page.value.cards {
            c.name = canonical_name(&c.name);
            cards.insert(squash(&c.name), c);
        }
        quiet = if cards.len() == before { quiet + 1 } else { 0 };
        if std::env::var_os("BAOCR_PROGRESS_DEBUG").is_some() {
            eprintln!("  duty page {}: {} new, {} in all, quiet {}", _page, cards.len() - before, cards.len(), quiet);
        }
        if quiet >= 3 {
            break;
        }
        if quiet == 1 {
            // Nothing new: the end of the list, a frame taken before the last wheel step had shown, or a step the game
            // dropped. Scrolling again at once would add a second step on top of a first that lands late and skip a
            // whole page of students (a run read 94 of 106 that way). So look again at the same place first.
            std::thread::sleep(Duration::from_millis(1500));
            look = nav.settled()?;
            continue;
        }
        nav.scroll_list(gx, gy, true, 1)?;
        std::thread::sleep(Duration::from_millis(700));
        look = nav.settled()?;
    }
    if owned.is_none_or(|n| cards.len() >= n) {
        break;
    }
    if std::env::var_os("BAOCR_PROGRESS_DEBUG").is_some() {
        eprintln!("  duty pass {pass}: {} of {} students, again from the top", cards.len(), owned.unwrap_or(0));
    }
    nav.scroll_list(gx, gy, false, 12)?;
    std::thread::sleep(Duration::from_millis(1200));
    look = nav.settled()?;
    }
    if let Some((x, y)) = close_button(&look, &title) {
        nav.click_at(x, y)?;
        std::thread::sleep(Duration::from_millis(800));
    }
    let students: Vec<DutyStudent> = cards.into_values().collect();
    let picked = students.iter().filter(|s| s.selected == Some(true)).count() as u32;
    if counted.is_some_and(|c| c != picked) {
        snap.warnings.push(format!("당번 선택: 화면의 선택 수 {} 와 표시가 보인 학생 수 {} 가 다릅니다", counted.unwrap_or(0), picked));
    }
    snap.duty = Duty { selected_count: counted, limit, students };
    Ok(())
}

/// How many students the account has, from the last student scan saved in the add-on's folder (best effort).
fn owned_count() -> Option<usize> {
    let path = std::env::var_os("BAOCR_ROSTER_JSON").map(std::path::PathBuf::from).or_else(|| {
        std::env::var_os("APPDATA").map(|d| std::path::PathBuf::from(d).join("baocr").join("students").join("students.json"))
    })?;
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
    v.get("entries")?.as_array().map(|a| a.len()).filter(|n| *n > 0)
}

/// Reads every student plate of the duty grid again (see `reread_plate`), in place.
fn reread_plates(ocr: &dyn Ocr, look: &mut Look) {
    let header = look.boxes.iter().find(|b| b.text.replace(' ', "") == "당번선택").map(|b| b.y1);
    let bottom = look.boxes.iter().find(|b| b.text.replace(' ', "") == "취소").map(|b| b.y0 - 45.0);
    let Some(header) = header else { return };
    let img = &look.img;
    for b in look.boxes.iter_mut() {
        let hangul = b.text.chars().any(|c| ('가'..='힣').contains(&c));
        let plate = b.y0 > header + 60.0 && bottom.is_none_or(|lim| b.y1 < lim) && hangul && b.x1 - b.x0 < 200.0 && b.y1 - b.y0 < 40.0;
        if plate {
            b.text = reread_plate(ocr, img, b);
        }
    }
    // a student plate is a dark slate-blue pill; text over the lobby behind the window (a lone "글") is not one. The
    // test is on the picture, not on the letters, so a one-letter name such as 슌 stays.
    let (hdr, lim) = (header, bottom);
    look.boxes.retain(|b| {
        let in_grid = b.y0 > hdr + 60.0 && lim.is_none_or(|l| b.y1 < l) && b.x1 - b.x0 < 200.0 && b.y1 - b.y0 < 40.0;
        !in_grid || on_plate(&look.img, b)
    });
}

/// Is the box on a student plate (dark slate blue behind the letters)?
fn on_plate(img: &RgbImage, b: &TextBox) -> bool {
    let (x0, y0) = (b.x0.max(0.0) as u32, b.y0.max(0.0) as u32);
    let (x1, y1) = ((b.x1 as u32).min(img.width()), (b.y1 as u32).min(img.height()));
    let (mut hit, mut n) = (0u32, 0u32);
    for y in y0..y1 {
        for x in x0..x1 {
            let p = img.get_pixel(x, y).0;
            let d = (p[0] as i32 - 70).abs() + (p[1] as i32 - 86).abs() + (p[2] as i32 - 108).abs();
            hit += (d < 70) as u32;
            n += 1;
        }
    }
    n > 0 && hit as f32 / n as f32 >= 0.3
}

/// Scales a name plate is read again at. The game window is only about 480 x 360 screen pixels, so the 1280 x 960 frame
/// is that picture enlarged 2.7 times and a plate's letters are 5 to 8 pixels tall in the original. The recogniser
/// reads such letters better when they are enlarged again (measured: "아야네(수영복)" and "시로코*테러" came out right at
/// 3x and 4x and wrong at 1x). The profile screen, whose name is set in a large font, reads 106 of 106 exactly.
const PLATE_SCALES: [u32; 3] = [2, 3, 4];

/// What the whole pipeline (detector and recogniser) reads on one enlarged crop of a plate, with its mean confidence.
fn read_enlarged(ocr: &dyn Ocr, crop: &RgbImage, k: u32) -> Option<(String, f32)> {
    let big = image::imageops::resize(crop, crop.width() * k, crop.height() * k, image::imageops::FilterType::CatmullRom);
    let mut canvas = RgbImage::from_pixel(big.width() + 80, big.height() + 80, image::Rgb([255, 255, 255]));
    image::imageops::overlay(&mut canvas, &big, 40, 40);
    let mut boxes = ocr.boxes(&canvas).ok()?;
    if boxes.is_empty() {
        return None;
    }
    boxes.sort_by(|a, b| a.x0.total_cmp(&b.x0));
    let text: String = boxes.iter().map(|b| b.text.as_str()).collect::<Vec<_>>().join("").split_whitespace().collect();
    let conf = boxes.iter().map(|b| b.score).sum::<f32>() / boxes.len() as f32;
    (!text.is_empty()).then_some((text, conf))
}

/// The text of one plate box read again at several enlargements. A reading that two or more of the readings (the
/// original included) agree on wins; with no agreement the most confident one stays.
fn reread_plate(ocr: &dyn Ocr, img: &RgbImage, b: &TextBox) -> String {
    let (w, h) = (img.width() as i32, img.height() as i32);
    let (x0, y0) = ((b.x0 as i32 - 4).max(0), (b.y0 as i32 - 4).max(0));
    let (x1, y1) = ((b.x1 as i32 + 4).min(w), (b.y1 as i32 + 4).min(h));
    if x1 - x0 < 8 || y1 - y0 < 8 {
        return b.text.clone();
    }
    let crop = image::imageops::crop_imm(img, x0 as u32, y0 as u32, (x1 - x0) as u32, (y1 - y0) as u32).to_image();
    let mut readings: Vec<(String, f32)> = vec![(squash(&b.text), b.score)];
    for k in PLATE_SCALES {
        if let Some(r) = read_enlarged(ocr, &crop, k) {
            if std::env::var_os("BAOCR_PROGRESS_DEBUG").is_some() {
                eprintln!("  plate {:?} ({:.3}) at {k}x -> {:?} ({:.3})", b.text, b.score, r.0, r.1);
            }
            readings.push(r);
        }
    }
    let mut best: Option<(usize, f32, &str)> = None;
    for (text, conf) in &readings {
        let votes = readings.iter().filter(|(o, _)| o == text).count();
        let top = readings.iter().filter(|(o, _)| o == text).map(|(_, c)| *c).fold(0.0, f32::max);
        if best.is_none_or(|(v, c, _)| votes > v || (votes == v && top > c)) {
            best = Some((votes, top, text));
        }
    }
    match best {
        Some((votes, _, text)) if votes >= 2 => text.to_string(),
        _ => b.text.clone(),
    }
}

/// What the duty-list path reads on one plate picture: the detector finds the letters, then `reread_plate` votes over the
/// enlarged readings. This is the per-plate step of `reread_plates` without the page bookkeeping, so the plate evaluation
/// (`--eval-plates`) scores the same code the scan runs. Returns the raw text, with no dictionary applied.
pub fn read_plate(ocr: &dyn Ocr, plate: &RgbImage) -> String {
    match plate_box(ocr, plate) {
        Some(b) => squash(&reread_plate(ocr, plate, &b)),
        None => String::new(),
    }
}

/// The detector's reading of a plate picture alone, before any re-reading: all its boxes joined left to right.
pub fn detect_plate(ocr: &dyn Ocr, plate: &RgbImage) -> String {
    plate_box(ocr, plate).map(|b| squash(&b.text)).unwrap_or_default()
}

fn plate_box(ocr: &dyn Ocr, plate: &RgbImage) -> Option<TextBox> {
    let mut boxes = ocr.boxes(plate).ok()?;
    if boxes.is_empty() {
        return None;
    }
    boxes.sort_by(|a, b| a.x0.total_cmp(&b.x0));
    let text = boxes.iter().map(|b| b.text.as_str()).collect::<Vec<_>>().join("");
    let score = boxes.iter().map(|b| b.score).sum::<f32>() / boxes.len() as f32;
    Some(TextBox {
        x0: boxes.iter().map(|b| b.x0).fold(f32::MAX, f32::min),
        y0: boxes.iter().map(|b| b.y0).fold(f32::MAX, f32::min),
        x1: boxes.iter().map(|b| b.x1).fold(f32::MIN, f32::max),
        y1: boxes.iter().map(|b| b.y1).fold(f32::MIN, f32::max),
        text,
        score,
    })
}

/// A student name read off a list, set to the dictionary's spelling when the dictionary is sure of it: the list font
/// gave "순" for 슌, "시로코테러" for 시로코*테러 and "우네" for 아야네. An ambiguous or unknown reading stays as read.
fn canonical_name(raw: &str) -> String {
    if std::env::var_os("BAOCR_NO_SNAP").is_some() {
        return raw.to_string();
    }
    let (name, m) = baocr_names::resolve(raw);
    match m.status.as_str() {
        "exact" | "corrected" if !name.is_empty() => name,
        _ => baocr_names::snap(raw).unwrap_or_else(|| raw.to_string()),
    }
}

/// A region name without spaces: OCR spacing of the same name differs from page to page.
fn squash(name: &str) -> String {
    name.chars().filter(|c| !c.is_whitespace()).collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn list_names_are_set_to_the_dictionary_spelling() {
        assert_eq!(super::canonical_name("노아"), "노아");
        assert_eq!(super::canonical_name("순"), "슌");
        assert_eq!(super::canonical_name("시로코테러"), "시로코*테러");
        assert_eq!(super::canonical_name("하례"), "하레");
        assert_eq!(super::canonical_name("키시키"), "키사키");
        assert_eq!(super::canonical_name("아네"), "아네", "two names are as close: not guessed");
    }

    use super::*;

    #[test]
    fn clock_texts() {
        assert!(is_clock("AM7:32") && is_clock("PM12:05") && is_clock("AM4:15"));
        assert!(!is_clock("AX+O") && !is_clock("AM:15") && !is_clock("7:32"));
    }
}
