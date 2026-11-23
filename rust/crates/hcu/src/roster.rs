//! Roster preparation: learn the total number of students before scanning, and
//! open the first student, so progress can be shown as n/m and the scan always
//! starts at the beginning.
use crate::{click_game, game_capture, to_frame, Hcu};
use anyhow::{anyhow, bail, Result};
use baocr_pixels::{detail_visible, roster_cards, roster_visible_pixels, same_cards, scroll_thumb, signature_distance, list_shift, slot_occupied, unowned_banner, Card, CardSignature};
use std::sync::mpsc::{channel, Receiver, Sender};
use image::RgbImage;
use std::thread::sleep;
use std::time::Duration;

type R4 = [f64; 4];

const MAX_PAGES: usize = 60;
/// Where the wheel is sent on the roster (a spot inside the card grid's scroll area).
const SCROLL_AT: (i32, i32) = (1150, 750);
/// Largest wheel movement sent at once.
const SCROLL_MAX: i32 = 1800;
/// The scrollbar thumb's bottom when the list is scrolled to its end.
const THUMB_END: i32 = 940;
/// The thumb's top when the list is at its start.
const THUMB_START_MAX: i32 = 190;
/// A card seen before: same portrait signature.
const KNOWN_DISTANCE: f64 = 2.0;
/// The back arrow on a profile.
const BACK: (i32, i32) = (50, 40);

/// Add the cards of one page that have not been seen yet; returns how many were new.
pub fn merge_new_cards(seen: &mut Vec<CardSignature>, cards: &[Card]) -> usize {
    let mut added = 0;
    for c in cards {
        if !seen.iter().any(|s| signature_distance(s, &c.signature) < KNOWN_DISTANCE) {
            seen.push(c.signature);
            added += 1;
        }
    }
    added
}

/// `count` wheel movements (of the largest size) in one HCU call.
fn wheel(h: &Hcu, region: R4, delta: i32, count: usize) -> Result<()> {
    let (x, y) = to_frame(region, SCROLL_AT.0, SCROLL_AT.1);
    h.scroll_n(x, y, delta, count)
}

fn at_top(img: &RgbImage) -> bool {
    scroll_thumb(img).top <= THUMB_START_MAX
}

/// Scroll the roster back to its first row, `moves` wheel movements per call. The
/// thumb position, not the card positions (which depend on the scroll), says when
/// the top is reached.
pub fn scroll_to_top_by(h: &Hcu, region: R4, moves: usize) -> Result<RgbImage> {
    let mut img = game_capture(h, region)?;
    for _ in 0..MAX_PAGES {
        if at_top(&img) {
            return Ok(img);
        }
        wheel(h, region, -SCROLL_MAX, moves)?;
        sleep(Duration::from_millis(450));
        img = game_capture(h, region)?;
    }
    bail!("could not scroll the roster back to its start")
}

pub fn scroll_to_top(h: &Hcu, region: R4) -> Result<RgbImage> {
    scroll_to_top_by(h, region, 6)
}

/// Grid lines seen so far, counted by line instead of by portrait.
///
/// A line is identified by where it is in the whole list (its position on the
/// screen plus how far the list has scrolled), so no portrait is compared. For
/// each line it keeps how many of its six slots hold a student. Only lines above
/// the "미보유" banner count: from there on the students are not owned.
#[derive(Default)]
pub struct RowTally {
    /// (position in the whole list, students in the line)
    rows: Vec<(i32, usize)>,
}

/// Lines this far down the screen may be cut off by the list's bottom edge.
const CLIPPED_BELOW: i32 = 825;

/// Two sightings of a line closer than this are the same line.
const SAME_LINE: i32 = 60;

impl RowTally {
    /// `offset`: how far the list has scrolled when this page was captured. `last`:
    /// the end of the list, where the bottom lines are whole as well.
    pub fn add_page(&mut self, img: &RgbImage, offset: i32, last: bool) {
        let banner = unowned_banner(img);
        // roster_cards only reports lines fully inside the list, wherever the scroll left them
        for row in roster_cards(img).chunks(6).filter(|r| r.len() == 6) {
            if banner.is_some_and(|by| row[0].y > by) {
                continue;
            }
            // A line cut off by the bottom edge is reported at a wrong position; it is whole
            // (and counted) on the next page, which is at most a screen minus one line further.
            if row[0].y >= CLIPPED_BELOW && !last {
                continue;
            }
            let at = row[0].y + offset;
            if self.rows.iter().all(|(y, _)| (y - at).abs() >= SAME_LINE) {
                self.rows.push((at, row.iter().filter(|c| slot_occupied(img, c.x, c.y)).count()));
            }
        }
    }

    pub fn total(&self) -> usize {
        self.rows.iter().map(|r| r.1).sum()
    }

    #[cfg(test)]
    pub fn lines(&self) -> usize {
        self.rows.len()
    }
}

/// Lines fully shown on a page and the distance between two lines (frame px).
fn page_geometry(img: &RgbImage) -> Option<(i32, f64)> {
    let ys: Vec<i32> = roster_cards(img).chunks(6).filter(|r| r.len() == 6).map(|r| r[0].y).collect();
    if ys.len() < 2 {
        return None;
    }
    Some((ys.len() as i32, (ys[ys.len() - 1] - ys[0]) as f64 / (ys.len() - 1) as f64))
}

/// Height of the list area on screen (frame px).
const LIST_HEIGHT: i32 = 750;
/// Marks the error of a first scroll that went further than a line-safe distance.
const OVERSHOOT: &str = "scroll overshoot";
/// Wheel movements per call used to measure how far one movement goes.
const PROBE_MOVES: usize = 1;
/// Most movements sent in one call.
const MAX_MOVES: usize = 12;

/// Scroll down page by page. Each page and its scroll offset go to the tally thread.
/// The number of wheel movements per call follows the measured travel, so every
/// scroll advances nearly a screen minus one line (so every line is shown whole once).
/// Stops at the "미보유" banner or the end of the list. Returns the movements used last.
fn capture_pages(h: &Hcu, region: R4, first: RgbImage, lines: Option<i32>, delta: i32, tx: &Sender<(RgbImage, i32, bool)>, counts: &Receiver<usize>, on_count: &mut dyn FnMut(usize)) -> Result<usize> {
    let mut img = first;
    let (_, pitch) = page_geometry(&img).ok_or_else(|| anyhow!("no student lines visible"))?;
    // A line is seen whole on some page as long as one scroll is at most the list's
    // height minus one line. Fast mode goes nearly that far; slow mode one line.
    let safe = LIST_HEIGHT - pitch.round() as i32;
    let target = lines.map_or((safe as f64 * 0.85).round() as i32, |n| (n as f64 * pitch).round() as i32);
    let max_shift = safe - 10;
    let (mut moves, mut offset) = (PROBE_MOVES, 0);
    for page in 0..MAX_PAGES {
        if !roster_visible_pixels(&img) {
            bail!("the roster disappeared while counting");
        }
        let thumb = scroll_thumb(&img);
        let banner = unowned_banner(&img);
        if let Ok(dir) = std::env::var("BAOCR_DEBUG_FRAMES") {
            // diagnostics: every counted page and what was seen on it
            let _ = std::fs::create_dir_all(&dir);
            let _ = img.save(format!("{dir}/page_{page:02}.png"));
            eprintln!("count page {page}: moves {moves} offset {offset} thumb {}..{} banner {banner:?}", thumb.top, thumb.bottom);
        }
        let end_of_list = banner.is_none() && (thumb.top < 0 || thumb.bottom >= THUMB_END);
        tx.send((img.clone(), offset, end_of_list)).map_err(|_| anyhow!("the page tally stopped"))?;
        while let Ok(n) = counts.try_recv() {
            on_count(n);
        }
        if banner.is_some() || thumb.top < 0 || thumb.bottom >= THUMB_END {
            return Ok(moves);
        }
        wheel(h, region, delta, moves)?;
        // the list keeps easing out after the call returns; a frame taken too early
        // measures a short travel and the next scroll overshoots (measured: 603 px vs 478 px)
        sleep(Duration::from_millis(450));
        let next = game_capture(h, region)?;
        let moved = list_shift(&img, &next, 760).ok_or_else(|| anyhow!("lost track of the scroll position"))?;
        if moved > max_shift {
            bail!("{OVERSHOOT}: the scroll went {moved}px, more than the {max_shift}px that keeps every line in view");
        }
        offset += moved;
        // closed loop: size the next scroll from the distance one movement really went
        let per_move = moved as f64 / moves as f64;
        moves = ((target as f64 / per_move).round() as usize).clamp(1, MAX_MOVES);
        img = next;
    }
    bail!("the roster has more than {MAX_PAGES} pages; stopping the count")
}

fn count_pages(h: &Hcu, region: R4, lines: Option<i32>, delta: i32, on_count: &mut dyn FnMut(usize)) -> Result<usize> {
    let first = scroll_to_top(h, region)?;
    if !roster_visible_pixels(&first) {
        bail!("the student roster (All) is not visible");
    }
    let (tx, rx) = channel::<(RgbImage, i32, bool)>();
    let (ctx, counts) = channel::<usize>();
    std::thread::scope(|s| {
        // the tally runs beside the scrolling: reading frames never slows the capture
        let worker = s.spawn(move || {
            let mut tally = RowTally::default();
            for (img, offset, last) in rx {
                tally.add_page(&img, offset, last);
                let _ = ctx.send(tally.total());
            }
            tally
        });
        let captured = capture_pages(h, region, first, lines, delta, &tx, &counts, on_count);
        drop(tx);
        // back to the top while the last frames are still being tallied
        let to_top = match &captured {
            Ok(moves) => scroll_to_top_by(h, region, (*moves * 2).max(6)).map(|_| ()),
            Err(_) => Ok(()),
        };
        let tally = worker.join().map_err(|_| anyhow!("the page tally panicked"))?;
        while let Ok(n) = counts.try_recv() {
            on_count(n);
        }
        captured?;
        to_top?;
        Ok(tally.total())
    })
}

/// Count the students from the first line to the "미보유" banner. Must be on the roster.
/// `on_count` is told the running total after every page.
///
/// Fast path: several lines per scroll. If a page shares no line with the previous
/// one (the scroll skipped lines) the count is redone one line at a time.
pub fn count_roster(h: &Hcu, region: R4, mut on_count: impl FnMut(usize)) -> Result<usize> {
    // How far a wheel movement goes differs between machines and game sessions: when the
    // first scroll goes too far, start over from the top with a smaller movement.
    let mut delta = SCROLL_MAX;
    let mut last = None;
    for _ in 0..4 {
        match count_pages(h, region, None, delta, &mut on_count) {
            Ok(n) => return Ok(n),
            Err(e) if format!("{e:#}").contains(OVERSHOOT) => {
                eprintln!("{e:#}; retrying with a smaller wheel movement");
                last = Some(e);
                delta /= 2;
            }
            Err(e) => {
                // the count always restarts from the top, so a half-way result is never reused
                eprintln!("fast roster count failed ({e:#}); counting one line per scroll");
                return count_pages(h, region, Some(1), delta, &mut on_count);
            }
        }
    }
    Err(last.unwrap())
}

/// From a profile, go back to the roster list.
pub fn go_to_roster(h: &Hcu, region: R4) -> Result<RgbImage> {
    click_game(h, region, BACK.0, BACK.1)?;
    for _ in 0..12 {
        sleep(Duration::from_millis(400));
        let img = game_capture(h, region)?;
        if roster_visible_pixels(&img) {
            return Ok(img);
        }
    }
    bail!("the roster did not come back after pressing the back arrow")
}

/// Wait until the card grid stops moving (a scroll animation is still easing out
/// right after the wheel stops) and return that still frame.
fn settled_roster(h: &Hcu, region: R4) -> Result<RgbImage> {
    let mut prev = game_capture(h, region)?;
    for _ in 0..10 {
        sleep(Duration::from_millis(300));
        let img = game_capture(h, region)?;
        if roster_visible_pixels(&img) && same_cards(&roster_cards(&prev), &roster_cards(&img)) && !roster_cards(&img).is_empty() {
            return Ok(img);
        }
        prev = img;
    }
    bail!("the roster never held still")
}

/// Click the first card and wait for the profile. The grid is allowed to settle
/// first, and a click that did not open anything is retried on a fresh frame.
pub fn open_first_student(h: &Hcu, region: R4) -> Result<RgbImage> {
    scroll_to_top(h, region)?;
    for _ in 0..3 {
        let img = settled_roster(h, region)?;
        let mut cards = roster_cards(&img);
        cards.sort_by_key(|c| (c.row, c.column));
        let first = cards.first().ok_or_else(|| anyhow!("no student cards visible"))?;
        click_game(h, region, first.x, first.y)?;
        for _ in 0..8 {
            sleep(Duration::from_millis(300));
            let img = game_capture(h, region)?;
            if detail_visible(&img) {
                return Ok(img);
            }
        }
    }
    bail!("the first student's profile did not open")
}

/// How the game screen looks right now.
pub enum Screen {
    Roster,
    Profile,
    Other,
}

pub fn screen_kind(img: &RgbImage) -> Screen {
    if detail_visible(img) {
        Screen::Profile
    } else if roster_visible_pixels(img) {
        Screen::Roster
    } else {
        Screen::Other
    }
}

/// From the main menu, press the 학생 button and wait for the student list.
fn enter_student_menu(h: &Hcu, region: R4, button: (i32, i32)) -> Result<RgbImage> {
    click_game(h, region, button.0, button.1)?;
    for _ in 0..15 {
        sleep(Duration::from_millis(400));
        let img = game_capture(h, region)?;
        if roster_visible_pixels(&img) {
            return Ok(img);
        }
    }
    bail!("the student list did not open after pressing 학생 on the main menu")
}

/// Bring the game to the student list from wherever it is, if the screen is
/// recognised: a profile goes back to the list, the main menu enters 학생.
/// Any other screen is left alone (no clicks) and reported.
///
/// `student_button` recognises the main menu in a frame and returns where 학생 is.
fn reach_roster(h: &Hcu, region: R4, student_button: &dyn Fn(&RgbImage) -> Option<(i32, i32)>) -> Result<()> {
    let img = game_capture(h, region)?;
    match screen_kind(&img) {
        Screen::Roster => Ok(()),
        Screen::Profile => go_to_roster(h, region).map(|_| ()),
        Screen::Other => match student_button(&img) {
            Some(b) => enter_student_menu(h, region, b).map(|_| ()),
            None => bail!("open the student list, a student profile or the main menu first"),
        },
    }
}

/// Learn the total and leave the game on the first student's profile.
///
/// Wherever the game is (profile, list or main menu) it ends on the first
/// student. The list is always scrolled to its start before counting and again
/// afterwards, so the scroll position it was left in does not matter.
pub fn prepare_scan(h: &Hcu, region: R4, student_button: &dyn Fn(&RgbImage) -> Option<(i32, i32)>, on_count: impl FnMut(usize)) -> Result<usize> {
    h.activate()?;
    reach_roster(h, region, student_button)?;
    let total = count_roster(h, region, on_count)?;
    open_first_student(h, region)?;
    Ok(total)
}

/// Make sure a profile is open (used when the total could not be counted).
pub fn ensure_profile(h: &Hcu, region: R4, student_button: &dyn Fn(&RgbImage) -> Option<(i32, i32)>) -> Result<()> {
    let img = game_capture(h, region)?;
    if let Screen::Profile = screen_kind(&img) {
        return Ok(());
    }
    reach_roster(h, region, student_button)?;
    open_first_student(h, region).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fx(name: &str) -> RgbImage {
        image::open(format!("{}/../../../fixtures/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap().to_rgb8()
    }

    /// Two overlapping roster pages must count each student once.
    #[test]
    fn overlapping_pages_count_unique_cards() {
        let (a, b) = (fx("roster-top.png"), fx("roster-scroll1.png"));
        let (ca, cb) = (roster_cards(&a), roster_cards(&b));
        let mut seen = Vec::new();
        let first = merge_new_cards(&mut seen, &ca);
        assert_eq!(first, ca.len(), "every card on the first page is new");
        assert_eq!(merge_new_cards(&mut seen, &ca), 0, "the same page twice adds nothing");
        let second = merge_new_cards(&mut seen, &cb);
        assert!(second > 0 && second < cb.len(), "the scrolled page overlaps the first: {second} new of {}", cb.len());
        assert_eq!(seen.len(), ca.len() + second);
    }

    #[test]
    fn tally_counts_lines_by_position_and_stops_at_the_banner() {
        let (top, next) = (fx("roster-top.png"), fx("roster-scroll1.png"));
        let shift = list_shift(&top, &next, 700).unwrap();
        let mut t = RowTally::default();
        t.add_page(&top, 0, false);
        let (one_page, lines) = (t.total(), t.lines());
        assert!(one_page > 0 && one_page == lines * 6, "full lines only: {one_page}");
        t.add_page(&top, 0, false); // the same page again adds nothing
        assert_eq!(t.total(), one_page);
        t.add_page(&next, shift, false); // the scrolled page overlaps: only its new lines count
        assert!(t.lines() > lines && t.lines() < lines + 3, "{} lines", t.lines());
        // the page with the banner: its last owned line has four students, nothing below counts
        let mut b = RowTally::default();
        b.add_page(&fx("roster-unowned-banner.png"), 0, false);
        assert_eq!(b.total(), 4);
    }

    /// A line cut off by the bottom edge is reported at a wrong position and must not
    /// be counted as a line of its own (it showed up as 10 extra students).
    #[test]
    fn line_clipped_by_the_bottom_edge_is_left_for_the_next_page() {
        let img = fx("roster-page-clipped-bottom.png");
        let whole = roster_cards(&img).chunks(6).filter(|r| r[0].y < CLIPPED_BELOW).count();
        let mut t = RowTally::default();
        t.add_page(&img, 3145, false);
        assert_eq!(t.lines(), whole);
        assert!(roster_cards(&img).chunks(6).count() > whole, "the fixture has a clipped line");
        // at the end of the list the bottom line is whole and counts
        let mut end = RowTally::default();
        end.add_page(&img, 3145, true);
        assert!(end.lines() > whole);
    }

    #[test]
    fn page_geometry_of_a_roster_page() {
        let (rows, pitch) = page_geometry(&fx("roster-top.png")).unwrap();
        assert!(rows >= 2 && (200.0..320.0).contains(&pitch), "rows {rows} pitch {pitch}");
    }

    #[test]
    fn screen_kinds() {
        assert!(matches!(screen_kind(&fx("roster-top.png")), Screen::Roster));
        assert!(matches!(screen_kind(&fx("detail-shun.png")), Screen::Profile));
        assert!(matches!(screen_kind(&RgbImage::new(1280, 960)), Screen::Other));
    }
}
