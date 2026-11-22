use super::*;
use image::Rgb;

fn load(name: &str) -> RgbImage {
    let p = format!("{}/../../../fixtures/{}", env!("CARGO_MANIFEST_DIR"), name);
    image::open(&p).unwrap_or_else(|e| panic!("{p}: {e}")).to_rgb8()
}

fn ip(v: Option<i32>) -> i32 {
    v.unwrap_or(-1)
}

#[test]
fn slot_detection() {
    let mut img = RgbImage::new(1280, 960);
    for x in [1061, 1077] {
        for xx in x - 5..=x + 5 {
            for y in 633..641 {
                img.put_pixel(xx, y, Rgb([0, 220, 230]));
            }
        }
    }
    assert_eq!(weapon_stars(&img), Some(2));
}

#[test]
fn pinned_from_real_screens() {
    for (f, pin) in [("detail-saori.png", true), ("detail-satsuki-swimsuit.png", false), ("detail-rio-special.png", true)] {
        assert_eq!(pin_state(&load(f)), Some(pin), "{f}");
    }
}

#[test]
fn detail_visible_vs_roster() {
    for f in [
        "detail-saori.png",
        "detail-shun.png",
        "detail-rio-armed.png",
        "detail-shiroko-terror.png",
        "detail-satsuki-swimsuit.png",
        "detail-serika-newyear-lv1.png",
        "detail-tsurugi-swimsuit-lv1-locked.png",
    ] {
        let img = load(f);
        assert!(detail_visible(&img), "{f}: profile not recognised");
        assert!(!roster_visible_pixels(&img), "{f}: profile mistaken for the roster");
    }
    for f in ["roster-top.png", "roster-scroll1.png"] {
        assert!(!detail_visible(&load(f)), "{f}: roster mistaken for a profile");
    }
}

#[test]
fn roster_rows_and_overlap() {
    let first = roster_cards(&load("roster-top.png"));
    let second = roster_cards(&load("roster-scroll1.png"));
    assert_eq!(first.len(), 12);
    assert!(second.len() >= 12, "scroll={}", second.len());
    let count = second
        .iter()
        .filter(|c| first.iter().any(|p| signature_distance(&c.signature, &p.signature) < 2.0))
        .count();
    assert_eq!(count, 6, "overlap");
}

#[test]
fn roster_scrollbar() {
    let (a, b) = (load("roster-top.png"), load("roster-scroll1.png"));
    let (p, q) = (scroll_thumb(&a), scroll_thumb(&b));
    assert!(roster_visible_pixels(&a) && roster_visible_pixels(&b));
    assert!(p.top >= 179 && p.top <= 185 && q.top > p.top, "thumb {p:?} -> {q:?}");
}

#[test]
fn blank_roster_has_no_cards() {
    assert!(roster_cards(&RgbImage::new(1280, 960)).is_empty());
    assert!(!roster_visible_pixels(&load("detail-saori.png")));
}

#[test]
fn fingerprints_consistent() {
    let a = load("detail-shun.png");
    let b = load("detail-rio-armed.png");
    assert_eq!(detail_distance(&detail_key(&a), &detail_key(&a)), 0.0);
    assert_eq!(frame_distance(&frame_key(&a), &frame_key(&a)), 0.0);
    assert!(detail_distance(&detail_key(&a), &detail_key(&b)) >= 2.0, "different names must exceed the repeat threshold");
    assert!(frame_distance(&frame_key(&a), &frame_key(&b)) > 1.5);
    assert_eq!(panel_delta(&a, &a), 0.0);
    assert!(panel_delta(&a, &b) > 0.004);
}

#[test]
fn meta_pixels_from_real_screens() {
    // (file, role, attack, armor, terrain, basic dot, equipment dot)
    let cases = [
        ("detail-shun.png", "딜러", "폭발", "경장갑", [4, 2, 0], true, true),
        ("detail-rio-armed.png", "딜러", "폭발", "특수장갑", [4, 2, 0], false, false),
    ];
    for (f, role, attack, armor, terrain, basic, equip) in cases {
        let img = load(f);
        assert_eq!(role_from_icon(&img), role, "{f} role");
        let (at, ar) = attack_armor_types(&img);
        assert_eq!((at.as_str(), ar.as_str()), (attack, armor), "{f} types");
        let t = read_terrain(&img);
        assert_eq!([ip(t.urban), ip(t.outdoor), ip(t.indoor)], terrain, "{f} terrain");
        let b = read_badges(&img);
        assert_eq!(b.basic_info_tab, Some(basic), "{f} basic dot");
        assert_eq!(b.equipment_growth, Some(equip), "{f} equipment dot");
        let _ = read_locks(&img);
    }
}

#[test]
fn locked_skills_have_no_level() {
    let img = load("detail-tsurugi-swimsuit-lv1-locked.png");
    assert_eq!(read_skill_locks(&img), SkillLocked { ex: false, basic: false, passive: true, sub: true });
    let ok = load("detail-shun.png");
    assert_eq!(read_skill_locks(&ok), SkillLocked::default());
}

/// 루미: dictionary terrain 1/1/4 (red frown, not the angry face = 0).
#[test]
fn terrain_grade_one_versus_zero() {
    let t = read_terrain(&load("detail-rumi-terrain1.png"));
    assert_eq!([ip(t.urban), ip(t.outdoor), ip(t.indoor)], [1, 1, 4]);
    let s = read_terrain(&load("detail-shun.png"));
    assert_eq!([ip(s.urban), ip(s.outdoor), ip(s.indoor)], [4, 2, 0]);
}

#[test]
fn stars_and_ink_images() {
    let saori = load("detail-saori.png");
    assert_eq!(stars(&saori), Some(5));
    assert_eq!(weapon_stars(&saori), Some(2));
    let name = name_ink_image(&saori);
    assert_eq!(name.dimensions(), (236, 46));
    assert!(name.pixels().any(|p| p.0[0] == 0), "name ink empty");
    let heart = heart_ink_image(&saori);
    assert_eq!(heart.dimensions(), (50, 38));
    assert!(heart.pixels().any(|p| p.0[0] == 0), "heart ink empty");
}
