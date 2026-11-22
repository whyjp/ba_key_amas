//! Roster preparation: learn the total number of students before scanning, and
//! open the first student, so progress can be shown as n/m and the scan always
//! starts at the beginning.
use crate::{click_game, game_capture, to_frame, Hcu};
use anyhow::{anyhow, bail, Result};
use baocr_pixels::{detail_visible, roster_cards, roster_visible_pixels, same_cards, scroll_thumb, signature_distance, Card, CardSignature};
use image::RgbImage;
use std::thread::sleep;
use std::time::Duration;

type R4 = [f64; 4];

const MAX_PAGES: usize = 60;
/// Where the wheel is sent on the roster (a spot inside the card grid's scroll area).
const SCROLL_AT: (i32, i32) = (1150, 750);
const SCROLL_STEP: i32 = 300;
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

fn wheel(h: &Hcu, region: R4, delta: i32) -> Result<()> {
    let (x, y) = to_frame(region, SCROLL_AT.0, SCROLL_AT.1);
    h.scroll(x, y, delta)
}

fn at_top(img: &RgbImage) -> bool {
    scroll_thumb(img).top <= THUMB_START_MAX
}

/// Scroll the roster back to its first row.
pub fn scroll_to_top(h: &Hcu, region: R4) -> Result<RgbImage> {
    let mut img = game_capture(h, region)?;
    for _ in 0..MAX_PAGES {
        if at_top(&img) {
            return Ok(img);
        }
        wheel(h, region, -SCROLL_STEP * 3)?;
        sleep(Duration::from_millis(450));
        img = game_capture(h, region)?;
    }
    bail!("could not scroll the roster back to its start")
}

/// Count the students by scrolling through the whole roster. Must be on the roster.
/// `on_count` is told the running total after every page.
pub fn count_roster(h: &Hcu, region: R4, mut on_count: impl FnMut(usize)) -> Result<usize> {
    let mut img = scroll_to_top(h, region)?;
    if !roster_visible_pixels(&img) {
        bail!("the student roster (All) is not visible");
    }
    let mut seen: Vec<CardSignature> = Vec::new();
    for _ in 0..MAX_PAGES {
        if !roster_visible_pixels(&img) {
            bail!("the roster disappeared while counting");
        }
        let cards = roster_cards(&img);
        merge_new_cards(&mut seen, &cards);
        on_count(seen.len());
        wheel(h, region, SCROLL_STEP)?;
        sleep(Duration::from_millis(450));
        let next = game_capture(h, region)?;
        if same_cards(&cards, &roster_cards(&next)) {
            if scroll_thumb(&next).bottom >= THUMB_END {
                scroll_to_top(h, region)?;
                return Ok(seen.len());
            }
            bail!("the roster did not scroll while counting");
        }
        img = next;
    }
    bail!("the roster has more than {MAX_PAGES} pages; stopping the count")
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

/// Click the first card and wait for the profile.
pub fn open_first_student(h: &Hcu, region: R4) -> Result<RgbImage> {
    let img = scroll_to_top(h, region)?;
    let mut cards = roster_cards(&img);
    cards.sort_by_key(|c| (c.row, c.column));
    let first = cards.first().ok_or_else(|| anyhow!("no student cards visible"))?;
    click_game(h, region, first.x, first.y)?;
    for _ in 0..12 {
        sleep(Duration::from_millis(400));
        let img = game_capture(h, region)?;
        if detail_visible(&img) {
            return Ok(img);
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

/// Learn the total and leave the game on the first student's profile.
///
/// Returns `Ok(Some(total))`. When counting fails the game is still left on a
/// profile if one can be opened, and the error is returned so the caller can fall
/// back to an unknown total.
pub fn prepare_scan(h: &Hcu, region: R4, on_count: impl FnMut(usize)) -> Result<usize> {
    h.activate()?;
    let img = game_capture(h, region)?;
    if let Screen::Profile = screen_kind(&img) {
        go_to_roster(h, region)?;
    } else if let Screen::Other = screen_kind(&img) {
        bail!("open the student list or a student profile first");
    }
    let total = count_roster(h, region, on_count)?;
    open_first_student(h, region)?;
    Ok(total)
}

/// Make sure a profile is open (used when the total could not be counted).
pub fn ensure_profile(h: &Hcu, region: R4) -> Result<()> {
    let img = game_capture(h, region)?;
    match screen_kind(&img) {
        Screen::Profile => Ok(()),
        Screen::Roster => open_first_student(h, region).map(|_| ()),
        Screen::Other => bail!("open the student list or a student profile first"),
    }
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
    fn screen_kinds() {
        assert!(matches!(screen_kind(&fx("roster-top.png")), Screen::Roster));
        assert!(matches!(screen_kind(&fx("detail-shun.png")), Screen::Profile));
        assert!(matches!(screen_kind(&RgbImage::new(1280, 960)), Screen::Other));
    }
}
