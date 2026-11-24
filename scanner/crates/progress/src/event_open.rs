//! The running event: its period and the stage rows of the Quest or Story tab.
//!
//! The event's name is a picture logo, so it is left null and the period (`개최 기간`) is the identity. A stage
//! row is a number box (`08`), the stage name to its right, and the `입장` button at the row's end. The tab
//! that is showing cannot be told from the text (both labels are always there), so the caller, who clicked
//! it, passes the kind. Quest rows have three star icons under the number; story rows have a book icon.
//! The list scrolls, so a read returns the rows visible and the caller merges reads by stage number.
use crate::geom::{cy, digits, h, text};
use crate::schema::{OpenEvent, Period, Stage, StageKind};
use crate::stars::count_stars;
use crate::Read;
use baocr_core::ocr_api::TextBox;
use image::RgbImage;
use regex::Regex;

/// `None` when the screen shows no event period (no open event on it).
pub fn parse_open_event(boxes: &[TextBox], kind: StageKind, img: Option<&RgbImage>) -> Read<Option<OpenEvent>> {
    let mut warnings = Vec::new();
    let Some(period) = parse_period(boxes) else {
        warnings.push("event: no 개최 기간 line, not an open event screen".into());
        return Read { value: None, warnings };
    };
    let mut stages = Vec::new();
    for number in boxes.iter().filter(|b| digits(b, 2).is_some()) {
        let Some(name) = stage_name(boxes, number) else { continue };
        let stars = match (kind, img) {
            (StageKind::Quest, Some(img)) => count_stars(img, number),
            _ => None,
        };
        let n = digits(number, 2).unwrap();
        if kind == StageKind::Quest && stars.is_none() {
            warnings.push(format!("event: stage {n:02} stars not readable"));
        }
        stages.push((cy(number), Stage { number: n, name, stars, kind }));
    }
    stages.sort_by(|a, b| a.0.total_cmp(&b.0));
    if stages.is_empty() {
        warnings.push("event: no stage rows visible".into());
    }
    warnings.push("event: name is a picture, left null".into());
    let stages = stages.into_iter().map(|s| s.1).collect();
    Read { value: Some(OpenEvent { name: None, period, stages, scanned_while_open: true }), warnings }
}

fn parse_period(boxes: &[TextBox]) -> Option<Period> {
    let re = Regex::new(r"(\d{4}-\d{2}-\d{2})\s*(\d{2}:\d{2})\s*[~∼～-]\s*(\d{4}-\d{2}-\d{2})\s*(\d{2}:\d{2})").unwrap();
    boxes.iter().find_map(|b| {
        let c = re.captures(&b.text)?;
        Some(Period { start: format!("{} {}", &c[1], &c[2]), end: format!("{} {}", &c[3], &c[4]) })
    })
}

/// The name box of the row a number box starts: right of the number, on its line, with an `입장` button
/// further right on the same line. Other number boxes (the minigame counter) have no such row.
fn stage_name(boxes: &[TextBox], number: &TextBox) -> Option<String> {
    let name = boxes
        .iter()
        .filter(|b| b.x0 >= number.x1 - 0.1 * h(number) && b.x0 - number.x1 <= 4.0 * h(number))
        .filter(|b| (cy(b) - cy(number)).abs() <= h(number) && text(b).chars().count() >= 2 && digits(b, 2).is_none())
        .min_by(|a, b| a.x0.total_cmp(&b.x0))?;
    let button = boxes.iter().any(|b| b.x0 >= name.x1 && text(b).contains("입장") && (cy(b) - cy(name)).abs() <= h(name));
    button.then(|| text(name).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{boxes, png};

    fn rows(e: &OpenEvent) -> Vec<(u32, &str, Option<u8>)> {
        e.stages.iter().map(|s| (s.number, s.name.as_str(), s.stars)).collect()
    }

    #[test]
    fn quest_tab_rows_with_stars_and_period() {
        let r = parse_open_event(&boxes("w169-event-open"), StageKind::Quest, Some(&png("w169-event-open")));
        let e = r.value.expect("open event");
        assert_eq!(e.period, Period { start: "2026-11-10 11:00".into(), end: "2026-11-24 10:59".into() });
        assert_eq!(e.name, None);
        assert!(e.scanned_while_open);
        assert_eq!(
            rows(&e),
            vec![
                (8, "쓰레기 섬 연안", Some(3)),
                (9, "전인미답의 땅", Some(3)),
                (10, "마코토 대륙", Some(3)),
                (11, "블루 홀의 중심", Some(3)),
                (12, "끝없는 망망대해", Some(3)),
            ]
        );
        assert!(e.stages.iter().all(|s| s.kind == StageKind::Quest));
        // the minigame counter (48) is not a stage
        assert!(e.stages.iter().all(|s| s.number != 48));
    }

    #[test]
    fn quest_tab_without_the_frame_leaves_stars_null() {
        let r = parse_open_event(&boxes("w169-event-open"), StageKind::Quest, None);
        let e = r.value.unwrap();
        assert!(e.stages.iter().all(|s| s.stars.is_none()));
        assert!(r.warnings.iter().any(|w| w.contains("stars not readable")));
    }

    #[test]
    fn story_tab_rows_have_no_stars() {
        let r = parse_open_event(&boxes("w169-event-open-story"), StageKind::Story, None);
        let e = r.value.unwrap();
        assert_eq!(
            rows(&e),
            vec![
                (5, "정체불명의 갤리온 복도", None),
                (6, "비스마루 창고", None),
                (7, "비스마루 선장실", None),
                (8, "비스마루 포 갑판", None),
                (9, "트라이던트 함상", None),
            ]
        );
        assert_eq!(e.period.end, "2026-11-24 10:59");
    }

    #[test]
    fn a_screen_without_a_period_is_not_an_open_event() {
        let r = parse_open_event(&boxes("w169-event-list-past"), StageKind::Quest, None);
        assert!(r.value.is_none());
    }
}
