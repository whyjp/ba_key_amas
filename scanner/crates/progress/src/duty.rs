//! The duty student menu (당번 선택): which students are picked as duty students (at most 10) and which of them
//! have the memorial lobby unlocked.
//!
//! The menu is a grid of cards, each with a name plate under the portrait. A picked card has a green numbered badge
//! at the top right of the portrait (the card's outline is no use: it touches the neighbour's); the small icon at the lower right of the portrait is pink when the student's memorial lobby is
//! unlocked and grey when it is not (the pink-means-unlocked reading is inferred from the screen, not from the
//! game's own text). The grid scrolls, so a read returns the cards fully in view and the caller merges reads by
//! name. Offsets are plain pixels from the name plate's centre: UI elements are the same size in both layouts.
use crate::geom::{cx, cy, squashed, text};
use crate::schema::DutyStudent;
use crate::vision::{count, Rect};
use crate::Read;
use baocr_core::ocr_api::TextBox;
use image::RgbImage;
use regex::Regex;

/// What one look at the menu shows.
#[derive(Debug, Clone, PartialEq)]
pub struct DutyView {
    /// The number picked now and the most that can be picked (`선택 중 7/10`).
    pub selected_count: Option<u32>,
    pub limit: Option<u32>,
    pub cards: Vec<DutyStudent>,
}

/// Light green of the numbered badge of a picked card.
fn is_badge_green(p: [u8; 3]) -> bool {
    let [r, g, b] = p.map(|c| c as i32);
    g > 215 && (170..=230).contains(&r) && (100..=175).contains(&b)
}

/// Pink of an unlocked memorial lobby icon.
fn is_icon_pink(p: [u8; 3]) -> bool {
    let [r, g, b] = p.map(|c| c as i32);
    r > 230 && g < 160 && b > 180
}

pub fn parse_duty(boxes: &[TextBox], img: &RgbImage) -> Read<DutyView> {
    let mut warnings = Vec::new();
    let counter = Regex::new(r"^선택중(\d+)/(\d+)$").unwrap();
    let Some((header, caps)) = boxes.iter().find_map(|b| counter.captures(&squashed(b)).map(|c| (b, (c[1].parse::<u32>().ok(), c[2].parse::<u32>().ok())))) else {
        warnings.push("duty: no 선택 중 n/m counter, not the duty menu".into());
        return Read { value: DutyView { selected_count: None, limit: None, cards: vec![] }, warnings };
    };
    // the grid ends above the 취소 / 확인 buttons; a plate cut off by the grid's edge is read as nothing sensible
    let bottom = boxes.iter().find(|b| squashed(b) == "취소").map(|b| b.y0 - 45.0);
    let labels = ["전체랜덤", "당번선택", "OFF", "ESC", "Space", "취소", "확인"];
    let mut cards = Vec::new();
    for b in boxes {
        let t = squashed(b);
        let plate = b.y0 > header.y1 + 60.0
            && bottom.is_none_or(|lim| b.y1 < lim)
            && t.chars().any(|c| ('가'..='힣').contains(&c))
            && !t.chars().any(|c| c.is_ascii_digit())
            && !labels.contains(&t.as_str());
        if !plate {
            continue;
        }
        let (px, py) = (cx(b) as i32, cy(b) as i32);
        let badge = Rect { x0: px + 30, y0: py - 114, x1: px + 50, y1: py - 94 };
        let selected = count(img, badge, is_badge_green) >= 150;
        let icon = Rect { x0: px + 12, y0: py - 52, x1: px + 36, y1: py - 32 };
        let memorial = Some(count(img, icon, is_icon_pink) >= 25);
        cards.push(DutyStudent { name: text(b).to_string(), selected: Some(selected), memorial_lobby: memorial });
    }
    cards.sort_by(|a, b| a.name.cmp(&b.name));
    if cards.is_empty() {
        warnings.push("duty: no student plates in view".into());
    }
    Read { value: DutyView { selected_count: caps.0, limit: caps.1, cards }, warnings }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{boxes, png};

    fn view() -> DutyView {
        parse_duty(&boxes("menu-duty-select-4x3"), &png("menu-duty-select-4x3")).value
    }

    #[test]
    fn the_counter_and_the_limit() {
        let v = view();
        assert_eq!((v.selected_count, v.limit), (Some(7), Some(10)));
    }

    #[test]
    fn seven_students_are_picked_and_they_are_the_numbered_cards() {
        let picked: Vec<String> = view().cards.into_iter().filter(|c| c.selected == Some(true)).map(|c| c.name).collect();
        assert_eq!(picked, vec!["노아", "사츠키(수영복)", "시로코(라이딩)", "아리스", "키라라", "토모에", "토모에(치파오)"]);
    }

    #[test]
    fn memorial_lobby_is_the_pink_icon() {
        let locked: Vec<String> = view().cards.into_iter().filter(|c| c.memorial_lobby == Some(false)).map(|c| c.name).collect();
        assert_eq!(locked, vec!["노아", "사츠키(수영복)", "아리스"]);
    }

    #[test]
    fn cut_off_plates_and_level_boxes_are_not_students() {
        assert_eq!(view().cards.len(), 12);
    }

    #[test]
    fn other_screens_are_not_the_duty_menu() {
        let r = parse_duty(&boxes("w169-lobby"), &png("w169-lobby"));
        assert!(r.value.cards.is_empty() && r.value.selected_count.is_none());
    }
}
