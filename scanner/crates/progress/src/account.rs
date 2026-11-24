//! Nickname and level from the top-left corner of the lobby (also present on the MomoTalk screens).
//!
//! The corner shows `LV.` and the nickname on one line, the level number under `LV.`. Boxes are found by
//! their position relative to the `LV.` box, in units of its size, so the layout does not matter.
use crate::geom::{cx, cy, digits, h, text, w};
use crate::schema::Account;
use crate::Read;
use baocr_core::ocr_api::TextBox;

pub fn parse_lobby(boxes: &[TextBox]) -> Read<Account> {
    let mut account = Account::default();
    let mut warnings = Vec::new();
    let Some(lv) = boxes.iter().find(|b| text(b).to_ascii_uppercase().trim_end_matches(['.', ':']) == "LV") else {
        warnings.push("account: no LV. label on the screen, nickname and level not read".into());
        return Read { value: account, warnings };
    };

    account.nickname = boxes
        .iter()
        .filter(|b| b.x0 >= lv.x1 - 0.2 * w(lv) && (cy(b) - cy(lv)).abs() <= h(lv))
        .filter_map(|b| Some((b.x0, clean_nickname(&b.text)?)))
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, name)| name);
    if account.nickname.is_none() {
        warnings.push("account: no nickname right of the LV. label".into());
    }

    account.level = boxes
        .iter()
        .filter(|b| (cx(b) - cx(lv)).abs() <= w(lv) && cy(b) > cy(lv) && cy(b) - cy(lv) <= 3.0 * h(lv))
        .filter_map(|b| Some((cy(b) - cy(lv), digits(b, 3)?)))
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, level)| level)
        .filter(|l| (1..=100).contains(l));
    if account.level.is_none() {
        warnings.push("account: no level number under the LV. label".into());
    }
    warnings.push("account: circle is not on this screen, left null".into());
    Read { value: account, warnings }
}

/// The nickname without the decoration OCR reads from the status icon next to it (quotes, crosses).
fn clean_nickname(raw: &str) -> Option<String> {
    // the sprout beside the name reads as a lone jamo ("소다팝ㅇ"); a nickname is made of whole syllables
    let lone_jamo = |c: char| ('\u{3131}'..='\u{318E}').contains(&c);
    let name = raw.trim().trim_end_matches(|c: char| !c.is_alphanumeric() || lone_jamo(c)).trim();
    (!name.is_empty()).then(|| name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::boxes;

    #[test]
    fn lobby_gives_nickname_and_level() {
        let r = parse_lobby(&boxes("w169-lobby"));
        assert_eq!(r.value, Account { nickname: Some("소다팝".into()), level: Some(69), circle: None });
        assert!(r.warnings.iter().any(|w| w.contains("circle")));
    }

    #[test]
    fn an_icon_read_as_a_lone_jamo_is_not_part_of_the_nickname() {
        assert_eq!(clean_nickname("소다팝ㅇ").as_deref(), Some("소다팝"));
        assert_eq!(clean_nickname("소다팝 ㅇ").as_deref(), Some("소다팝"));
        assert_eq!(clean_nickname("소다팝\"").as_deref(), Some("소다팝"));
        assert_eq!(clean_nickname("ㅇ"), None);
        assert_eq!(clean_nickname("Teacher7").as_deref(), Some("Teacher7"));
    }

    #[test]
    fn momotalk_corner_reads_the_same() {
        let r = parse_lobby(&boxes("w169-momotalk-list"));
        assert_eq!(r.value.nickname.as_deref(), Some("소다팝"));
        assert_eq!(r.value.level, Some(69));
    }

    #[test]
    fn other_screens_are_null_with_a_warning() {
        let r = parse_lobby(&boxes("w169-schedule"));
        assert_eq!(r.value, Account::default());
        assert!(r.warnings[0].contains("no LV."));
    }
}
