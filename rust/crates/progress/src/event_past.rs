//! The past-event grid (이벤트 일람, 이벤트 스토리 tab): which cards carry the red `complete!` stamp.
//!
//! Event names are picture logos, so cards are identified by their place in the list, which is chronological.
//! The cards are found from the image: each card has a grey border and the rows and columns line up, so grey lines
//! as tall as a card (vertical) or as wide as the whole panel (horizontal) cut the list into cells. Only cells whose four sides are visible
//! count; a half-visible row at the bottom is left for the next scroll. The stamp is a red blob about a third
//! of a card high in the right half of the card; red letters of a logo are smaller or sit further left.
//! Recorded in 16:9 only (3 columns); 4:3 relies on the same proportions.
use crate::schema::PastEvent;
use crate::vision::{blobs, is_border_grey, is_red, long_lines, Rect};
use crate::Read;
use image::RgbImage;

/// Where the lists start, as a fraction of the frame: the right 60 percent, below the top bar.
const LIST_LEFT: f32 = 0.40;
const LIST_TOP: f32 = 0.15;

#[derive(Debug, Clone, PartialEq)]
pub struct Cell {
    pub row: usize,
    pub col: usize,
    pub rect: Rect,
    pub complete: bool,
}

/// The fully visible cards, row by row, with their stamp state.
pub fn read_grid(img: &RgbImage) -> Vec<Cell> {
    let (w, h) = (img.width() as i32, img.height() as i32);
    let area = Rect { x0: (w as f32 * LIST_LEFT) as i32, y0: (h as f32 * LIST_TOP) as i32, x1: w, y1: h };
    let xs = long_lines(img, area, true, (0.15 * h as f32) as i32, 6, is_border_grey);
    let ys = long_lines(img, area, false, (0.35 * w as f32) as i32, 6, is_border_grey);
    let (cols, rows) = (spans(&xs), spans(&ys));
    let mut cells = Vec::new();
    for (r, &(y0, y1)) in rows.iter().enumerate() {
        for (c, &(x0, x1)) in cols.iter().enumerate() {
            let rect = Rect { x0: x0 + 4, y0: y0 + 4, x1: x1 - 4, y1: y1 - 4 };
            cells.push(Cell { row: r, col: c, rect, complete: has_stamp(img, rect) });
        }
    }
    cells
}

/// Past events of the visible grid. `first_index` is the list position of the top-left card; the scan knows it
/// from how far it scrolled, the image does not.
pub fn parse_past_grid(img: &RgbImage, first_index: u32) -> Read<Vec<PastEvent>> {
    let cells = read_grid(img);
    let mut warnings = Vec::new();
    if cells.is_empty() {
        warnings.push("past events: no card grid found on the frame".into());
    }
    let cols = cells.iter().map(|c| c.col + 1).max().unwrap_or(0) as u32;
    let events = cells
        .iter()
        .map(|c| PastEvent { index: first_index + c.row as u32 * cols + c.col as u32, cleared: Some(c.complete), scanned_while_open: false })
        .collect();
    Read { value: events, warnings }
}

/// Spans between consecutive lines that are about as long as the longest one; the rest are borders of the
/// panel or of a card cut off by the scroll.
fn spans(lines: &[i32]) -> Vec<(i32, i32)> {
    let all: Vec<(i32, i32)> = lines.windows(2).map(|p| (p[0], p[1])).collect();
    let longest = all.iter().map(|s| s.1 - s.0).max().unwrap_or(0);
    all.into_iter().filter(|s| (s.1 - s.0) as f32 >= 0.8 * longest as f32).collect()
}

fn has_stamp(img: &RgbImage, cell: Rect) -> bool {
    let min_side = (0.18 * cell.h() as f32) as i32;
    blobs(img, cell, is_red).iter().any(|b| {
        let r = b.bounds;
        let centre_x = (r.x0 + r.x1) as f32 / 2.0;
        b.pixels >= 150 && r.w() >= min_side && r.h() >= min_side && centre_x >= cell.x0 as f32 + 0.6 * cell.w() as f32
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::png;

    #[test]
    fn nine_cards_eight_stamped() {
        let cells = read_grid(&png("w169-event-list-past"));
        assert_eq!(cells.len(), 9, "{cells:?}");
        assert_eq!(cells.iter().map(|c| c.col).max(), Some(2));
        let stamped: Vec<bool> = cells.iter().map(|c| c.complete).collect();
        // 축제대소동, 이반 쿠팔라, 위시리스트 / 히나, 술래잡기, 바니 체이서 / 운영일지, 신춘광상곡, 해피 발렌타인
        assert_eq!(stamped, vec![true, true, true, true, true, true, true, true, false]);
    }

    #[test]
    fn indices_follow_the_list_order_from_the_first_visible_card() {
        let r = parse_past_grid(&png("w169-event-list-past"), 6);
        let idx: Vec<u32> = r.value.iter().map(|e| e.index).collect();
        assert_eq!(idx, (6..15).collect::<Vec<u32>>());
        assert_eq!(r.value[8].cleared, Some(false));
        assert!(r.value.iter().all(|e| !e.scanned_while_open));
        assert!(r.warnings.is_empty());
    }

    #[test]
    fn a_frame_without_a_grid_reads_nothing() {
        let r = parse_past_grid(&png("w169-lobby"), 0);
        assert!(r.value.is_empty());
        assert_eq!(r.warnings.len(), 1);
    }
}
