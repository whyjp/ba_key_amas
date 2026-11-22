use baocr_core::model::*;
use baocr_extract::{DeepFrames, Extractor};
use baocr_ocr::PpOcr;
use image::RgbImage;
use std::path::PathBuf;

fn engine() -> Option<PpOcr> {
    let m = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../models"));
    m.join("korean_PP-OCRv5_rec_mobile.onnx").exists().then(|| PpOcr::new(&m).unwrap())
}

fn fx(name: &str) -> RgbImage {
    image::open(format!("{}/../../../fixtures/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap().to_rgb8()
}

/// Values below were read off the real screens by eye (rio-armed popup/tab, 3-star mystic).
#[test]
fn menu_frames_parse() {
    let Some(ocr) = engine() else { return };
    let ex = Extractor::new(&ocr);
    let mut s = Student::default();
    let f = DeepFrames { popup_top: fx("popup-stats-top.png"), popup_bottom: fx("popup-stats-bottom.png"), level_up: fx("tab-levelup.png"), mystic: fx("tab-mystic-3star.png") };
    ex.deep(&mut s, &f).unwrap();
    let d = s.deep.unwrap();
    let sd = d.stat_detail.unwrap();
    let got = [sd.main.max_hp, sd.main.attack, sd.main.defense, sd.main.healing, sd.accuracy, sd.evasion, sd.critical, sd.critical_damage_pct, sd.defense_pierce, sd.stability, sd.range, sd.cost_recovery, sd.cc_power, sd.cc_resist, sd.type_bonus_pct, sd.max_cost_bonus];
    let want = [33907, 3444, 85, 4848, 101, 1079, 523, 234, 0, 980, 550, 700, 100, 124, 200, 0].map(Some);
    assert_eq!(got, want, "popup {sd:?}");
    assert_eq!(sd.type_bonus_kind, "폭발");
    let b = d.base_stats.unwrap();
    assert_eq!((b.max_hp, b.attack, b.defense, b.healing), (Some(24336), Some(2191), Some(85), Some(4848)));
    assert_eq!(s.level, Some(68), "level from the level-up tab");
    let m = d.mystic.unwrap();
    assert_eq!((m.stars, m.next_stars, m.max_bond_rank, m.next_max_bond_rank), (Some(3), Some(4), Some(20), Some(30)));
    assert_eq!((m.next_max_hp, m.next_attack, m.next_healing), (Some(13837), Some(1424), Some(5179)));
}

#[test]
fn maxed_mystic_tab() {
    let Some(ocr) = engine() else { return };
    let ex = Extractor::new(&ocr);
    let mut s = Student::default();
    let f = DeepFrames { popup_top: fx("popup-stats-top.png"), popup_bottom: fx("popup-stats-bottom.png"), level_up: fx("tab-levelup.png"), mystic: fx("tab-mystic.png") };
    ex.deep(&mut s, &f).unwrap();
    let m = s.deep.unwrap().mystic.unwrap();
    assert_eq!(m.stars, Some(5));
    assert!(m.next_stars.is_none());
    assert_eq!(m.max_growth_reached, Some(true));
}

/// A blank frame must not be "read": no name, no bond, nothing guessed.
#[test]
fn blank_frame_yields_nothing() {
    let Some(ocr) = engine() else { return };
    let blank = RgbImage::from_pixel(1280, 960, image::Rgb([20, 40, 90]));
    let s = Extractor::new(&ocr).profile(&blank, "blank", &baocr_extract::Options::default()).unwrap();
    assert!(s.name.is_empty(), "name {:?}", s.name);
    assert!(s.bond_rank.is_none());
}

/// The main menu is recognised by its bottom labels and the 학생 button is found;
/// no other screen is taken for it.
#[test]
fn lobby_is_recognised_and_student_button_found() {
    let Some(ocr) = engine() else { return };
    let (x, y) = baocr_extract::lobby_student_button(&ocr, &fx("lobby-main-menu.png")).expect("lobby not recognised");
    // 학생 icon sits around (325, 890) on the 1280x960 layout
    assert!((x - 325).abs() <= 25 && (y - 890).abs() <= 30, "button at ({x},{y})");
    for other in ["roster-top.png", "detail-shun.png", "popup-stats-top.png", "tab-mystic.png"] {
        assert!(baocr_extract::lobby_student_button(&ocr, &fx(other)).is_none(), "{other} taken for the lobby");
    }
}
