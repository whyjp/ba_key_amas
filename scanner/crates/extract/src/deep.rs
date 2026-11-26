//! Menu-pass parsing: 상세 stat popup (two frames), 레벨 업 tab, 신비 해방 tab.
//! The frames come from the capture side (clicks); everything here is OCR + parsing.
use baocr_core::frames::DeepFrames;
use anyhow::Result;
use baocr_core::model::*;
use baocr_core::ocr_api::{text_in, Ocr, TextBox};
use baocr_pixels as px;
use image::RgbImage;
use once_cell::sync::Lazy;
use regex::Regex;

/// Popup row labels (spaces removed for comparison), as in the Go program.
const STAT_LABELS: [&str; 14] = [
    "최대체력",
    "공격력",
    "방어력",
    "치유력",
    "명중수치",
    "회피수치",
    "치명수치",
    "치명대미지",
    "방어관통수치",
    "안정수치",
    "일반공격사거리",
    "코스트회복력",
    "군중제어강화력",
    "군중제어저항력",
];
const MAX_COST_LABEL: &str = "최대코스트증가";

static VALUE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^(.*?)\s*(-?[\d,]+)\s*%?\s*$").unwrap());
static INTS: Lazy<Regex> = Lazy::new(|| Regex::new(r"\d+").unwrap());

fn atoi(s: &str) -> Option<Int> {
    s.replace(',', "").parse().ok()
}

fn ints(s: &str) -> Vec<Int> {
    INTS.find_iter(&s.replace(',', "")).filter_map(|m| m.as_str().parse().ok()).collect()
}

fn last_int(s: &str) -> Option<Int> {
    ints(s).last().copied()
}

/// Map OCR'd label text to a canonical label, or "" when unsure (ties are not evidence).
pub fn match_label(text: &str) -> String {
    let hangul: String = text.chars().filter(|c| ('\u{AC00}'..='\u{D7A3}').contains(c)).collect();
    if hangul.contains("특효") {
        return "특효".into();
    }
    let all: Vec<&str> = STAT_LABELS.iter().copied().chain(std::iter::once(MAX_COST_LABEL)).collect();
    let cj = baocr_names::to_jamo(&hangul);
    let (mut best, mut best_label, mut ties) = (usize::MAX, "", 0);
    for l in &all {
        let d = baocr_names::edit_distance(&cj, &baocr_names::to_jamo(l));
        if d < best {
            (best, best_label, ties) = (d, l, 1);
        } else if d == best {
            ties += 1;
        }
    }
    let limit = if cj.len() >= 8 { 2 } else { 1 };
    if best <= limit && ties == 1 {
        return best_label.into();
    }
    // A truncated label ("회복력" for 코스트회복력) is fine when it is a unique suffix.
    if hangul.chars().count() >= 3 {
        let mut found: Option<&str> = None;
        for l in &all {
            if l.ends_with(&hangul) {
                if found.is_some() {
                    return String::new();
                }
                found = Some(l);
            }
        }
        return found.unwrap_or("").into();
    }
    String::new()
}

/// Fold one "<label> <value>" cell into the stat detail.
pub fn apply_cell(d: &mut StatDetail, text: &str) {
    let text = text.replace('|', "");
    let text = text.trim();
    let Some(m) = VALUE.captures(text) else { return };
    let (label_text, Some(val)) = (m[1].to_string(), atoi(&m[2])) else { return };
    let v = Some(val);
    match match_label(&label_text).as_str() {
        "최대체력" => d.main.max_hp = v,
        "공격력" => d.main.attack = v,
        "방어력" => d.main.defense = v,
        "치유력" => d.main.healing = v,
        "명중수치" => d.accuracy = v,
        "회피수치" => d.evasion = v,
        "치명수치" => d.critical = v,
        "치명대미지" => d.critical_damage_pct = v,
        "방어관통수치" => d.defense_pierce = v,
        "안정수치" => d.stability = v,
        "일반공격사거리" => d.range = v,
        "코스트회복력" => d.cost_recovery = v,
        "군중제어강화력" => d.cc_power = v,
        "군중제어저항력" => d.cc_resist = v,
        "최대코스트증가" => d.max_cost_bonus = v,
        "특효" => {
            d.type_bonus_pct = v;
            for k in ["폭발", "관통", "신비", "진동"] {
                if label_text.contains(k) {
                    d.type_bonus_kind = k.into();
                }
            }
        }
        _ => {}
    }
}

const TOP_ROWS: [i32; 5] = [352, 419, 486, 553, 620];
const BOTTOM_ROWS: [i32; 5] = [386, 453, 520, 587, 654];

fn read_popup_rows(boxes: &[TextBox], ys: &[i32], d: &mut StatDetail, raw: &mut std::collections::BTreeMap<String, String>, prefix: &str) {
    let mut n = 0;
    for &y in ys {
        for (x0, x1) in [(232, 640), (650, 1062)] {
            let t = text_in(boxes, (x0, y - 21, x1, y + 21));
            raw.insert(format!("{prefix}_{n}"), t.clone());
            apply_cell(d, &t);
            n += 1;
        }
    }
}

/// Count gold stars in the 신비 해방 star row at the given x centres.
fn stars_at(img: &RgbImage, centers: &[i32]) -> Int {
    centers
        .iter()
        .filter(|&&cx| px::fraction(img, (cx - 9, 353, cx + 9, 371), |c| c[0] > 220 && c[1] > 170 && c[2] < 110) > 0.35)
        .count() as Int
}

/// The "after unlock" number of a "current -> after" row. It is drawn in green,
/// so isolating green ink leaves exactly that number whether or not the arrow
/// glyph is recognised.
fn green_number(ocr: &dyn Ocr, img: &RgbImage, row: px::Rect) -> Option<Int> {
    let line = px::ink_to_line(&px::ink_image(img, row, px::green_ink));
    let (text, _) = ocr.line(&line).ok()?;
    last_int(&text)
}

fn parse_mystic(ocr: &dyn Ocr, my: &RgbImage, raw: &mut std::collections::BTreeMap<String, String>) -> Result<Mystic> {
    let boxes = ocr.boxes(my)?;
    let at = |r: px::Rect| text_in(&boxes, r);
    let mut m = Mystic { stars: Some(stars_at(my, &[779, 807, 835, 863, 891])), ..Default::default() };
    let next = stars_at(my, &[1012, 1040, 1068, 1096, 1124]);
    if next > 0 {
        m.next_stars = Some(next);
    }
    m.unlock_skill_text = at((676, 457, 1010, 519));
    m.max_bond_rank_text = at((1018, 485, 1200, 515));
    m.status_text = at((670, 570, 1205, 640));
    raw.insert("mystic_status".into(), m.status_text.clone());
    let hangul: String = m.status_text.chars().filter(|c| ('\u{AC00}'..='\u{D7A3}').contains(c)).collect();
    if !hangul.is_empty() {
        m.max_growth_reached = Some(hangul.contains("도달"));
    }
    let ranks = ints(&m.max_bond_rank_text);
    if ranks.len() >= 2 {
        m.max_bond_rank = Some(ranks[0]);
        m.next_max_bond_rank = Some(ranks[1]);
    }
    if m.next_stars.is_some() {
        // "<current> -> <after unlock>" rows: the number after the arrow is the preview.
        m.next_max_hp = green_number(ocr, my, (858, 383, 940, 409));
        m.next_attack = green_number(ocr, my, (1100, 383, 1180, 409));
        m.next_healing = green_number(ocr, my, (826, 420, 900, 440));
    }
    // Material quantities and costs are deliberately not collected (they depend on
    // what the player did); which materials a student needs is fixed data.
    Ok(m)
}

pub(crate) fn extract_deep(ocr: &dyn Ocr, s: &mut Student, f: &DeepFrames) -> Result<()> {
    let mut d = Deep::default();
    let mut detail = StatDetail::default();
    read_popup_rows(&ocr.boxes(&f.popup_top)?, &TOP_ROWS, &mut detail, &mut s.raw_ocr, "popup_top");
    read_popup_rows(&ocr.boxes(&f.popup_bottom)?, &BOTTOM_ROWS, &mut detail, &mut s.raw_ocr, "popup_bottom");
    d.stat_detail = Some(detail);

    // 레벨 업 tab: growth stats without gear/bond, and a large, reliable "Lv.N".
    let lv = ocr.boxes(&f.level_up)?;
    let cell = |r: px::Rect| last_int(&text_in(&lv, r));
    d.base_stats = Some(Stats {
        max_hp: cell((686, 380, 945, 410)),
        attack: cell((960, 380, 1200, 410)),
        defense: cell((686, 414, 945, 444)),
        healing: cell((960, 414, 1200, 444)),
    });
    let tab_level = last_int(&text_in(&lv, (760, 350, 860, 376))).filter(|l| (1..=150).contains(l));
    match (s.level, tab_level) {
        (None, Some(t)) => {
            s.level = Some(t);
            s.warnings.push("level_from_levelup_tab".into());
        }
        (Some(a), Some(t)) if a != t => s.warnings.push(format!("level_mismatch: strip={a} levelup_tab={t}")),
        _ => {}
    }
    d.mystic = Some(parse_mystic(ocr, &f.mystic, &mut s.raw_ocr)?);
    s.deep = Some(d);
    baocr_names::deep_cross_check(s);
    Ok(())
}
