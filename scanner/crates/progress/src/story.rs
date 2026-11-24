//! Main story: the volume screen (volume title, its `클리어` badge, the chapter list) and the chapter screen
//! (`모든 에피소드를 클리어 했습니다.`).
//!
//! A chapter is cleared only when its own screen says so; the volume screen lists chapters without their state,
//! so those stay null. Absence of a cleared mark is also null, not false: a missed OCR box must not turn a
//! cleared chapter into an uncleared one. The caller decides after a second look.
use crate::geom::{cx, cy, squashed, text, w};
use crate::schema::{Chapter, StoryVolume};
use crate::Read;
use baocr_core::ocr_api::TextBox;
use regex::Regex;

/// The volume screen: the volume's title and chapters, cleared when the `클리어` badge sits on its card.
pub fn parse_main_volume(boxes: &[TextBox]) -> Read<Option<StoryVolume>> {
    let mut warnings = Vec::new();
    let title_re = Regex::new(r"^((?:Final|\d+)\s*\.\s*.+?)\s*편?$").unwrap();
    let titles: Vec<(&TextBox, String)> = boxes.iter().filter_map(|b| Some((b, title_re.captures(text(b))?[1].to_string()))).collect();
    // the card's name plate ends in 편; the list header above the chapters may not
    let Some((plate, volume)) = titles.iter().find(|(b, _)| text(b).ends_with('편')).or(titles.first()) else {
        warnings.push("story: no volume title, not the main story volume screen".into());
        return Read { value: None, warnings };
    };

    let chapter_re = Regex::new(r"^(\d+)\s*장\s*(.+)$").unwrap();
    let mut chapters = Vec::new();
    for b in boxes {
        if let Some(c) = chapter_re.captures(text(b)) {
            chapters.push(Chapter { chapter: format!("{}장", &c[1]), title: Some(c[2].trim().to_string()), cleared: None });
        }
    }
    chapters.sort_by_key(|c| c.chapter.trim_end_matches('장').parse::<u32>().unwrap_or(0));
    if chapters.is_empty() {
        warnings.push("story: no chapter rows in the volume screen".into());
    }

    let badge = boxes.iter().any(|b| squashed(b) == "클리어" && cy(b) < cy(plate) && cx(b) >= plate.x0 - 0.3 * w(plate) && cx(b) <= plate.x1);
    let cleared = badge.then_some(true);
    if !badge {
        warnings.push("story: no 클리어 badge on the volume card, cleared left null".into());
    }
    Read { value: Some(StoryVolume { volume: volume.clone(), chapters, cleared }), warnings }
}

/// What a chapter screen says about one chapter.
#[derive(Debug, Clone, PartialEq)]
pub struct ChapterRead {
    pub chapter: Option<Chapter>,
}

/// The chapter screen: its `N장` label and title, and the all-cleared banner.
pub fn parse_chapter_screen(boxes: &[TextBox]) -> Read<ChapterRead> {
    let mut warnings = Vec::new();
    let label_re = Regex::new(r"^(\d+)\s*장$").unwrap();
    let label = boxes.iter().find(|b| label_re.is_match(text(b)));
    let Some(label) = label else {
        warnings.push("story: no N장 label, not a chapter screen".into());
        return Read { value: ChapterRead { chapter: None }, warnings };
    };
    let number = &label_re.captures(text(label)).unwrap()[1];
    let title = boxes
        .iter()
        .filter(|b| b.y0 >= label.y0 && b.y0 > cy(label) && (b.x0 - label.x0).abs() <= w(label) && !text(b).is_empty())
        .min_by(|a, b| a.y0.total_cmp(&b.y0))
        .map(|b| text(b).to_string());
    let banner = Regex::new(r"모든\s*에피소드를?\s*클리어").unwrap();
    let cleared = boxes.iter().any(|b| banner.is_match(&b.text)).then_some(true);
    if cleared.is_none() {
        warnings.push("story: no all-cleared banner, cleared left null".into());
    }
    Read { value: ChapterRead { chapter: Some(Chapter { chapter: format!("{number}장"), title, cleared }) }, warnings }
}

/// One volume card of the volume map, with where its name plate is (frame pixels) so the caller can click it.
#[derive(Debug, Clone, PartialEq)]
pub struct VolumeCard {
    pub volume: StoryVolume,
    pub plate: (f32, f32),
}

/// The cards of the volume map (`N. 이름 편`). A card's state is its badge: `클리어` is cleared, `New` is open but
/// not cleared. The badge sits up and to the left of the name plate by a fixed distance (114 px up, 254 px from the plate's right edge). A card with no badge in view is null, never false: the card the player has selected
/// shows its synopsis instead of the badge, and a card cut off by the screen edge has its badge off screen.
pub fn parse_volume_cards(boxes: &[TextBox]) -> Read<Vec<VolumeCard>> {
    let mut warnings = Vec::new();
    let plate_re = Regex::new(r"(?i)^((?:\d+|Ex)\s*\.\s*.+?)편$").unwrap();
    let mut cards = Vec::new();
    for b in boxes.iter().filter(|b| b.x0 < 900.0) {
        let name = squashed(b);
        let Some(c) = plate_re.captures(&name) else { continue };
        // the badge sits 114 px above and 254 px left of the plate's right edge; OCR boxes differ in height
        // from read to read, so the offsets are plain pixels (UI elements are the same size in both layouts)
        let badge = boxes.iter().find(|k| {
            let label = squashed(k);
            (label == "클리어" || label == "New") && ((b.y0 - k.y1) - 114.0).abs() < 25.0 && ((b.x1 - cx(k)) - 254.0).abs() < 30.0
        });
        let cleared = badge.map(|k| squashed(k) == "클리어");
        if cleared.is_none() {
            warnings.push(format!("story: no badge in view for volume {}, cleared left null", &c[1]));
        }
        cards.push(VolumeCard { volume: StoryVolume { volume: c[1].to_string(), chapters: vec![], cleared }, plate: (cx(b), cy(b)) });
    }
    if cards.is_empty() {
        warnings.push("story: no volume cards on the screen".into());
    }
    Read { value: cards, warnings }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::boxes;

    #[test]
    fn final_volume_with_its_chapters_and_badge() {
        let r = parse_main_volume(&boxes("w169-main-story"));
        let v = r.value.unwrap();
        assert_eq!(v.volume, "Final.그리고 모든 기적이 시작되는 곳");
        assert_eq!(v.cleared, Some(true));
        let ch: Vec<(&str, Option<&str>)> = v.chapters.iter().map(|c| (c.chapter.as_str(), c.title.as_deref())).collect();
        assert_eq!(
            ch,
            vec![("1장", Some("살레탈환작전")), ("2장", Some("거짓된성소공략전")), ("3장", Some("아트라하시스의방주 점령전")), ("4장", Some("프레나파테스 결전"))]
        );
        // the volume screen does not show a chapter's state
        assert!(v.chapters.iter().all(|c| c.cleared.is_none()));
    }

    #[test]
    fn volume_screen_rejects_other_screens() {
        assert!(parse_main_volume(&boxes("w169-story-hub")).value.is_none());
    }

    #[test]
    fn cleared_chapter_screen() {
        let r = parse_chapter_screen(&boxes("w169-main-chapter-cleared"));
        assert_eq!(r.value.chapter, Some(Chapter { chapter: "4장".into(), title: Some("프레나파테스 결전".into()), cleared: Some(true) }));
        assert!(r.warnings.is_empty());
    }

    #[test]
    fn chapter_screen_without_the_banner_is_null_not_false() {
        let mut b = boxes("w169-main-chapter-cleared");
        b.retain(|x| !x.text.contains("모든"));
        let r = parse_chapter_screen(&b);
        assert_eq!(r.value.chapter.unwrap().cleared, None);
        assert_eq!(r.warnings.len(), 1);
    }

    #[test]
    fn other_screens_are_not_chapter_screens() {
        assert_eq!(parse_chapter_screen(&boxes("w169-main-story")).value.chapter, None);
    }

    fn states(name: &str) -> Vec<(String, Option<bool>)> {
        parse_volume_cards(&boxes(name)).value.into_iter().map(|c| (c.volume.volume, c.volume.cleared)).collect()
    }

    #[test]
    fn part_two_cards_show_their_badges() {
        let s = states("menu-main-story-part2-4x3");
        assert_eq!(
            s,
            vec![("5.백화요란".to_string(), None), ("Ex.데카그라마톤".to_string(), Some(true)), ("6.흘러간시간들의오라토리오".to_string(), Some(true))]
        );
    }

    #[test]
    fn the_16_9_map_reads_with_the_same_offsets_whatever_the_plate_height() {
        let s = states("w169-main-story-part2-map");
        assert_eq!(s, vec![("5.백화요란".to_string(), None), ("EX.데카그라마톤".to_string(), Some(true)), ("6.흘러간시간들의오라토리오".to_string(), Some(true))]);
    }

    #[test]
    fn part_one_new_card_is_not_cleared() {
        let s = states("menu-main-story-part1-4x3");
        assert!(s.contains(&("0.총학생회".to_string(), Some(true))), "{s:?}");
        assert!(s.contains(&("1.불꽃과그림자".to_string(), Some(false))), "{s:?}");
    }

    #[test]
    fn the_selected_card_shows_no_badge() {
        let s = states("menu-main-story-part2-sel5-4x3");
        assert_eq!(s[0], ("5.백화요란".to_string(), None));
        assert_eq!(s[2], ("6.흘러간시간들의오라토리오".to_string(), Some(true)));
    }
}
