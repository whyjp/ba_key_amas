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
        assert!(e.kind.is_some() && e.item_name.is_some(), "slot {i}: {e:?}");
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
        // Slot 1 (shoes) is really equipped; slots 2 and 3 show Lv.10/Lv.20 locks.
        assert!(equip_badge(&img, 0), "{f}: slot 0 should be equipped");
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
