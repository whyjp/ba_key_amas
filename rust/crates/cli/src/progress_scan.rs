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
use baocr_progress::{account, bareceipt, event_past, schedule, scroll, Rect};
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

    fn settled(&self) -> Result<Look> {
        self.read(settle_quiet(self.hcu, self.region)?)
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
const SECTIONS: usize = 3;

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
    let steps: [(&str, Section); 2] = [("스케줄", schedule_section), ("이벤트 스토리", past_events_section)];
    for (name, step) in steps {
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
    let file = out.join(format!("progress-{stamp}.json"));
    let text = serde_json::to_string_pretty(&snap)?;
    std::fs::write(&file, &text)?;
    std::fs::write(out.join("latest.json"), &text)?;
    let backup = bareceipt::export(&snap, &[]);
    std::fs::write(out.join("bareceipt-backup.json"), bareceipt::to_file_text(&backup.value))?;
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

/// A region name without spaces: OCR spacing of the same name differs from page to page.
fn squash(name: &str) -> String {
    name.chars().filter(|c| !c.is_whitespace()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_texts() {
        assert!(is_clock("AM7:32") && is_clock("PM12:05") && is_clock("AM4:15"));
        assert!(!is_clock("AX+O") && !is_clock("AM:15") && !is_clock("7:32"));
    }
}
