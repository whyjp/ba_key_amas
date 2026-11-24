use super::*;
use baocr_core::ocr_api::TextBox;

fn fixture(name: &str) -> RgbImage {
    let p = format!("{}/../../../fixtures/{}", env!("CARGO_MANIFEST_DIR"), name);
    image::open(&p).unwrap_or_else(|e| panic!("{p}: {e}")).to_rgb8()
}

/// Canned OCR: returns text by crop width (tier 32*4, state 77*4, level 53*4).
struct Fake {
    tier: &'static str,
    level: &'static str,
    state: &'static str,
}
impl Ocr for Fake {
    fn boxes(&self, _: &RgbImage) -> Result<Vec<TextBox>> {
        Ok(vec![])
    }
    fn line(&self, img: &RgbImage) -> Result<(String, f32)> {
        let t = match img.width() {
            128 => self.tier,
            308 => self.state,
            _ => self.level,
        };
        Ok((t.to_string(), 1.0))
    }
    fn choose(&self, _: &RgbImage, _: &[String]) -> Result<(usize, Vec<f32>)> {
        Ok((0, vec![]))
    }
}

#[test]
fn unknown_is_not_invented() {
    assert_eq!(equipment_tier("unreadable"), None);
    assert_eq!(catalog().entries.len(), 90);
    let blank = RgbImage::new(1280, 960);
    assert_eq!(match_equipment(&blank, 1, None).unwrap(), None);
}

#[test]
fn tier_and_level_parsing() {
    assert_eq!(equipment_tier(" T4"), Some(4));
    assert_eq!(equipment_tier("[10"), Some(10));
    assert_eq!(parse_level("Lv.40"), Some(40));
    assert_eq!(parse_level("l6u%4 O"), Some(40));
    assert_eq!(parse_level("40"), None);
    assert_eq!(parse_level("Lv.4"), None);
}

#[test]
fn saori_pixels() {
    let img = fixture("detail-saori.png");
    for i in 0..3 {
        assert!(equip_badge(&img, i), "badge {i}");
    }
    let types = ["Hat", "Hairpin", "Watch"];
    let names = ["Ribbon Beret", "Winged Hairpin", "Antique Pocket Watch"];
    for i in 0..3 {
        let m = match_equipment(&img, i + 1, Some(4)).unwrap().expect("match");
        assert_eq!((m.kind.as_str(), m.name.as_str(), m.tier), (types[i as usize], names[i as usize], 4));
    }
}

#[test]
fn saori_with_fake_ocr() {
    let img = fixture("detail-saori.png");
    // Same canned text for every call; slot 4 state also "EMPTY" below.
    let ocr = Fake { tier: "T4", level: "Lv.40", state: "EMPTY" };
    let (eq, raw) = detect_equipment(&img, &ocr).unwrap();
    assert_eq!(eq.len(), 4);
    for (i, e) in eq.iter().take(3).enumerate() {
        assert_eq!(e.equipped, Some(true));
        assert_eq!(e.tier, Some(4));
        assert_eq!(e.level, Some(40));
        // the item name is looked up from the static equipment table by (type, tier), not read from the icon
        assert!(e.kind.is_some() && e.item_name.is_none(), "slot {i}: {e:?}");
    }
    assert_eq!(eq[3].equipped, Some(false));
    assert_eq!(eq[3].level, None);
    assert_eq!(eq[3].slot_kind, "unique_gear");
    assert_eq!(raw["equipment_4_state"], "EMPTY");
    assert!(raw.contains_key("equipment_1_tier") && raw.contains_key("equipment_3_level"));
}

#[test]
fn locked_slots_not_equipped() {
    for f in ["detail-tsurugi-swimsuit-lv1-locked.png", "detail-serika-newyear-lv1.png"] {
        let img = fixture(f);
        // Slot 1 (shoes) has the yellow dot of an item waiting, and no "E" badge: not equipped. Slots 2 and 3 show
        // Lv.10/Lv.20 locks.
        assert!(equip_badge(&img, 0) && !equip_marker(&img, 0), "{f}: slot 0 is a dot without the E badge");
        for i in 1..3 {
            assert!(!equip_badge(&img, i), "{f}: locked slot {i} reported equipped");
        }
        let ocr = Fake { tier: "", level: "", state: "" };
        let (eq, _) = detect_equipment(&img, &ocr).unwrap();
        for e in eq.iter().skip(1) {
            assert_ne!(e.equipped, Some(true), "{f}: {e:?}");
            assert!(e.kind.is_none() && e.level.is_none());
        }
    }
}

#[test]
fn a_lost_t_leaves_the_digit_and_that_is_enough() {
    assert_eq!(equipment_tier("6"), Some(6));
    assert_eq!(equipment_tier(" 10 "), Some(10));
    assert_eq!(equipment_tier("ㄲ"), None);
    assert_eq!(equipment_tier("0"), None);
    assert_eq!(equipment_tier("T7"), Some(7));
    assert_eq!(equipment_tier("4 5 6"), None, "more than a tier is not a tier");
}

#[test]
fn a_level_limits_the_tier() {
    assert_eq!(min_tier_for_level(55), Some(7));
    assert_eq!(min_tier_for_level(50), Some(6));
    assert_eq!(min_tier_for_level(10), Some(1));
    assert_eq!(min_tier_for_level(70), Some(10));
    assert_eq!(min_tier_for_level(71), None);
}

#[test]
fn nothing_is_claimed_for_a_slot_without_a_gold_level_badge() {
    let img = RgbImage::new(1280, 960);
    let kinds = ["Hat".to_string(), "Hairpin".to_string(), "Watch".to_string()];
    let ocr = Fake { tier: "T6", level: "Lv.50", state: "EMPTY" };
    let (eq, _) = detect_equipment_for(&img, &ocr, &kinds).unwrap();
    assert!(eq.iter().take(3).all(|e| e.kind.is_none() && e.item_name.is_none()));
}

#[test]
fn the_tier_is_taken_from_the_number_and_the_icon_together() {
    assert_eq!(resolve_tier(Some(6), Some((6, 30.0))), (Some(6), "both"));
    assert_eq!(resolve_tier(Some(6), None), (Some(6), "number"));
    assert_eq!(resolve_tier(None, Some((7, 30.0))), (Some(7), "icon"));
    assert_eq!(resolve_tier(None, None), (None, "none"));
    assert_eq!(resolve_tier(Some(5), Some((6, 10.0))), (Some(6), "icon over number"), "a sure icon wins a disagreement");
    assert_eq!(resolve_tier(Some(5), Some((6, 35.0))), (Some(5), "number over icon"), "an unsure icon does not");
}

#[test]
fn a_greyed_slot_with_a_notification_dot_is_empty_not_equipped() {
    // kayoko: three greyed items and the yellow dots that look like the equipped badge
    let img = fixture("detail-kayoko-no-gear.png");
    assert!((0..3).all(|i| equip_badge(&img, i)), "the dots do pass for the badge");
    assert!((0..3).all(|i| icon_vividness(&img, i) < 0.006));
    let ocr = Fake { tier: "", level: "", state: "EMPTY" };
    let kinds = ["Shoes".to_string(), "Hairpin".to_string(), "Charm".to_string()];
    let (eq, _) = detect_equipment_for(&img, &ocr, &kinds).unwrap();
    for e in eq.iter().take(3) {
        assert_eq!(e.equipped, Some(false), "{e:?}");
        assert!(e.tier.is_none() && e.level.is_none() && e.kind.is_none(), "{e:?}");
    }
}

#[test]
fn an_item_in_colour_is_not_taken_for_a_greyed_one() {
    let img = fixture("detail-saori.png");
    assert!((0..3).all(|i| icon_vividness(&img, i) >= 0.006));
}

#[test]
fn a_misread_v_does_not_lose_the_level() {
    assert_eq!(loose_level("L060"), Some(60));
    assert_eq!(loose_level("Lo40"), Some(40));
    assert_eq!(loose_level("L0A0"), Some(40));
    assert_eq!(loose_level("L020"), Some(20));
    assert_eq!(loose_level("Ll"), Some(1));
    assert_eq!(loose_level("Lyl"), Some(1));
    assert_eq!(loose_level("1060"), None, "no L: not a level label");
    assert_eq!(loose_level("Lo"), None, "zero is not a level");
    assert_eq!(loose_level("L900"), None);
    let obs: Vec<String> = ["L060", "1060", "L060", "160"].iter().map(|s| s.to_string()).collect();
    assert_eq!(vote_level(&obs), Some(60));
    assert_eq!(vote_level(&["L060".to_string()]), None, "one sighting is not enough");
}

#[test]
fn only_the_e_badge_proves_an_item_is_equipped() {
    // 호시노(무장), 2026-11-24: three items in colour with Lv and T labels and the E badge
    let on = fixture("detail-equipped-bag-t8.png");
    assert!((0..3).all(|i| equip_marker(&on, i)));
    // 유우카(파자마): three greyed items, each with a yellow dot and nothing else (the necklace was read as tier 5)
    let off = fixture("detail-three-greyed-slots.png");
    assert!((0..3).all(|i| equip_badge(&off, i) && !equip_marker(&off, i)));
    let kinds = ["Shoes".to_string(), "Bag".to_string(), "Necklace".to_string()];
    let ocr = Fake { tier: "", level: "", state: "EMPTY" };
    let (eq, _) = detect_equipment_for(&off, &ocr, &kinds).unwrap();
    for e in eq.iter().take(3) {
        assert_eq!(e.equipped, Some(false), "{e:?}");
        assert!(e.tier.is_none() && e.level.is_none(), "{e:?}");
    }
}

#[test]
fn a_zero_for_the_v_is_not_a_level_digit() {
    assert_eq!(parse_level("Lo3"), None);
    assert_eq!(parse_level("L03"), None);
    let obs: Vec<String> = ["L1", "Lo", "Lo3", "L1", "Ll"].iter().map(|s| s.to_string()).collect();
    assert_eq!(vote_level(&obs), Some(1), "Lv.1 read three ways beats the single stray Lo3");
}
