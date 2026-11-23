//! One area of the mission (임무) list: its number, the Normal or Hard tab, and each row's stars.
//!
//! Rows are the `19-2` style labels; the three star icons under each are counted from pixels. The list
//! scrolls, so a read returns the rows visible and the caller merges reads by label. Which tab is active is
//! told from the tab backgrounds: the active tab is the dark one.
use crate::geom::{h, text, w};
use crate::schema::{Mission, MissionArea, Mode};
use crate::stars::count_stars;
use crate::vision::{mean_luma, Rect};
use crate::Read;
use baocr_core::ocr_api::TextBox;
use image::RgbImage;
use regex::Regex;

pub fn parse_mission_area(boxes: &[TextBox], img: &RgbImage) -> Read<MissionArea> {
    let mut warnings = Vec::new();
    let area_re = Regex::new(r"제\s*(\d+)\s*임무").unwrap();
    let area = boxes.iter().find_map(|b| area_re.captures(&b.text)?[1].parse().ok());
    if area.is_none() {
        warnings.push("missions: no 제N임무 title, area number not read".into());
    }
    let mode = active_tab(boxes, img);
    if mode.is_none() {
        warnings.push("missions: could not tell the Normal or Hard tab apart".into());
    }

    let label_re = Regex::new(r"^(\d{1,2})\s*-\s*(\d{1,2})$").unwrap();
    let mut rows: Vec<(f32, Mission)> = Vec::new();
    for b in boxes {
        let Some(c) = label_re.captures(text(b)) else { continue };
        let label = format!("{}-{}", &c[1], &c[2]);
        let stars = count_stars(img, b);
        if stars.is_none() {
            warnings.push(format!("missions: {label} stars not readable"));
        }
        rows.push((b.y0, Mission { label, stars, cleared: stars.map(|s| s > 0) }));
    }
    rows.sort_by(|a, b| a.0.total_cmp(&b.0));
    Read { value: MissionArea { area, mode, missions: rows.into_iter().map(|r| r.1).collect() }, warnings }
}

fn active_tab(boxes: &[TextBox], img: &RgbImage) -> Option<Mode> {
    let tab = |name: &str| boxes.iter().find(|b| text(b).eq_ignore_ascii_case(name));
    let luma = |b: &TextBox| {
        let (px, py) = (0.25 * w(b), 0.3 * h(b));
        mean_luma(img, Rect { x0: (b.x0 - px) as i32, y0: (b.y0 - py) as i32, x1: (b.x1 + px) as i32, y1: (b.y1 + py) as i32 })
    };
    let (n, hd) = (luma(tab("Normal")?)?, luma(tab("Hard")?)?);
    if (n - hd).abs() < 20.0 {
        return None;
    }
    Some(if n < hd { Mode::Normal } else { Mode::Hard })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{boxes, png};

    #[test]
    fn area_19_normal_rows() {
        let r = parse_mission_area(&boxes("w169-mission-list"), &png("w169-mission-list"));
        let a = r.value;
        assert_eq!(a.area, Some(19));
        assert_eq!(a.mode, Some(Mode::Normal));
        let rows: Vec<(&str, Option<u8>, Option<bool>)> = a.missions.iter().map(|m| (m.label.as_str(), m.stars, m.cleared)).collect();
        assert_eq!(
            rows,
            vec![("19-2", Some(2), Some(true)), ("19-3", Some(3), Some(true)), ("19-4", Some(3), Some(true)), ("19-5", Some(0), Some(false))]
        );
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
    }

    #[test]
    fn a_screen_without_missions_reads_nothing() {
        let r = parse_mission_area(&boxes("w169-lobby"), &png("w169-lobby"));
        assert_eq!(r.value.area, None);
        assert_eq!(r.value.mode, None);
        assert!(r.value.missions.is_empty());
    }
}
