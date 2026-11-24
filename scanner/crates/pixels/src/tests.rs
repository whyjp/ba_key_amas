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

/// The main menu has a dark-blue wallpaper that matches the header-band colour; it
/// must still not be taken for a profile (nor for the roster).
#[test]
fn main_menu_is_not_a_profile_or_roster() {
    let lobby = load("lobby-main-menu.png");
    assert!(!detail_visible(&lobby), "lobby taken for a profile");
    assert!(!roster_visible_pixels(&lobby), "lobby taken for the roster");
}

#[test]
fn unowned_banner_is_found_only_when_shown() {
    let shown = load("roster-unowned-banner.png");
    let y = unowned_banner(&shown).expect("banner not found");
    assert!((500..=530).contains(&y), "banner at {y}");
    assert!(unowned_banner(&load("roster-top.png")).is_none());
    assert!(unowned_banner(&load("roster-scroll1.png")).is_none());
}

#[test]
fn last_owned_row_has_four_students() {
    let img = load("roster-unowned-banner.png");
    let by = unowned_banner(&img).unwrap();
    let cards = roster_cards(&img);
    let rows: Vec<&[Card]> = cards.chunks(6).filter(|r| r[0].y < by).collect();
    assert_eq!(rows.len(), 1, "one complete owned row is above the banner");
    let n = rows[0].iter().filter(|c| slot_occupied(&img, c.x, c.y)).count();
    assert_eq!(n, 4);
    let full = load("roster-top.png");
    assert!(roster_cards(&full).chunks(6).all(|r| r.iter().all(|c| slot_occupied(&full, c.x, c.y))));
}

#[test]
fn list_shift_measures_the_scroll() {
    let (a, b) = (load("roster-top.png"), load("roster-scroll1.png"));
    let d = list_shift(&a, &b, 700).expect("overlapping pages");
    assert!(d > 20 && d < 700, "shift {d}");
    // the same page has not moved: nothing to measure
    assert_eq!(list_shift(&a, &a, 700), None);
    // unrelated pages do not overlap
    assert_eq!(list_shift(&a, &load("roster-unowned-banner.png"), 700), None);
}

#[test]
fn quick_formation_labels_are_found_by_colour_wherever_the_grid_is_scrolled() {
    let img = image::open(format!("{}/../../../fixtures/screens/quick-formation.png", env!("CARGO_MANIFEST_DIR"))).unwrap().to_rgb8();
    let labels = quick_card_labels(&img);
    // two whole rows of six cards; the third row is cut off by the viewport
    assert_eq!(labels.len(), 12, "{labels:?}");
    assert_eq!(labels[0].cx, 629, "{labels:?}");
    assert!((labels[0].y0 - 428).abs() <= 2 && (labels[0].y1 - 462).abs() <= 2, "{:?}", labels[0]);
    assert_eq!(labels[5].cx, 629 + 5 * 111);
    let second_row = labels[6];
    assert!((second_row.y0 - 580).abs() <= 6, "{second_row:?}");
    let ink = quick_label_ink(&img, &labels[0]);
    assert!(ink.pixels().filter(|p| p[0] == 0).count() > 60, "the name's strokes are ink");
}

#[test]
fn quick_formation_marks_the_students_already_in_the_squad() {
    let img = image::open(format!("{}/../../../fixtures/screens/quick-formation.png", env!("CARGO_MANIFEST_DIR"))).unwrap().to_rgb8();
    // first row: four lime-outlined cards (the squad), then two plain ones; second row: none
    for (cx, y0, want) in [(629, 427, true), (740, 427, true), (851, 428, true), (962, 428, true), (1073, 428, false), (1184, 428, false), (629, 579, false), (740, 579, false), (962, 579, false), (1184, 579, false)] {
        assert_eq!(quick_card_selected(&img, cx, y0), want, "card at x={cx} y={y0}");
    }
}

#[test]
fn quick_formation_labels_follow_the_scroll() {
    for name in ["quick-formation-scrolled-a.png", "quick-formation-scrolled-b.png"] {
        let img = image::open(format!("{}/../../../fixtures/screens/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap().to_rgb8();
        let labels = quick_card_labels(&img);
        assert_eq!(labels.len(), 12, "{name}: {labels:?}");
        assert!(labels.iter().all(|l| l.y0 > 318 && l.y1 < 650));
        assert_eq!(labels.iter().filter(|l| l.cx == 629).count(), 2, "{name}: one label per row in the first column");
    }
}

#[test]
fn quick_formation_tab_is_read_from_the_red_fill() {
    let img = image::open(format!("{}/../../../fixtures/screens/quick-formation.png", env!("CARGO_MANIFEST_DIR"))).unwrap().to_rgb8();
    assert_eq!(quick_tab_is_striker(&img), Some(true));
    // a screen without the popup has neither tab
    let lobby = image::open(format!("{}/../../../fixtures/lobby-main-menu.png", env!("CARGO_MANIFEST_DIR"))).unwrap().to_rgb8();
    assert_eq!(quick_tab_is_striker(&lobby), None);
}

#[test]
fn the_fast_frame_key_tells_views_apart_like_the_slow_one() {
    let dir = format!("{}/../../../fixtures/screens", env!("CARGO_MANIFEST_DIR"));
    let names = ["quick-formation", "quick-formation-live-a", "quick-formation-scrolled-a", "quick-formation-scrolled-b", "lobby-idle", "work-hub-full"];
    let frames: Vec<RgbImage> = names.iter().map(|n| image::open(format!("{dir}/{n}.png")).unwrap().to_rgb8()).collect();
    for (i, a) in frames.iter().enumerate() {
        assert_eq!(frame_distance(&fast_frame_key(a), &fast_frame_key(a)), 0.0);
        for (j, b) in frames.iter().enumerate().skip(i + 1) {
            let (slow, fast) = (frame_distance(&frame_key(a), &frame_key(b)), frame_distance(&fast_frame_key(a), &fast_frame_key(b)));
            assert!((slow - fast).abs() < 1.0, "{} vs {}: slow {slow:.2}, fast {fast:.2}", names[i], names[j]);
            // the settle threshold (1.5) must fall on the same side
            assert_eq!(slow < 1.5, fast < 1.5, "{} vs {}: slow {slow:.2}, fast {fast:.2}", names[i], names[j]);
        }
    }
}

#[test]
fn the_fast_frame_key_is_the_block_mean() {
    let mut img = RgbImage::from_pixel(1280, 960, Rgb([10, 20, 30]));
    for y in 0..30 {
        for x in 0..40 {
            img.put_pixel(x, y, Rgb([200, 100, 0]));
        }
    }
    let k = fast_frame_key(&img);
    assert_eq!(&k[0..3], &[200, 100, 0]);
    assert_eq!(&k[3..6], &[10, 20, 30]);
    assert_eq!(fast_frame_key(&RgbImage::new(10, 10)), [0u8; 3072], "a frame too small to key is blank, not a panic");
}

#[test]
fn art_only_screens_are_never_taken_for_a_battle() {
    // the main menu with its UI hidden shows only the illustration: no text, big saturated areas
    let dir = format!("{}/../../../fixtures/screens", env!("CARGO_MANIFEST_DIR"));
    for name in ["lobby-idle", "lobby", "work-hub-full"] {
        let path = format!("{dir}/{name}.png");
        let Ok(img) = image::open(&path) else { continue };
        assert!(!looks_like_battle(&img.to_rgb8()), "{name} looks like a battle");
    }
}

// ---- 16:9 (1280x720) recordings -------------------------------------------------------------

fn w169(name: &str) -> RgbImage {
    let img = load(&format!("screens/w169-{name}.png"));
    assert_eq!(img.dimensions(), (1280, 720), "{name}");
    img
}

#[test]
fn w169_profile_and_student_list_are_told_apart() {
    let profile = w169("student-profile");
    assert!(detail_visible(&profile), "16:9 profile not recognised");
    assert!(!roster_visible_pixels(&profile), "16:9 profile mistaken for the list");
    let list = w169("student-list");
    assert!(!detail_visible(&list), "16:9 list mistaken for a profile");
    assert!(roster_visible_pixels(&list), "16:9 list not recognised");
    let rows: Vec<i32> = roster_cards(&list).chunks(6).map(|r| r[0].y).collect();
    assert_eq!(roster_cards(&list).len(), 12);
    assert_eq!(rows, vec![313, 580], "card tops of the two rows");
    let t = scroll_thumb(&list);
    assert!((180..=182).contains(&t.top) && (212..=214).contains(&t.bottom), "{t:?}");
}

#[test]
fn w169_other_screens_are_neither_profile_nor_list() {
    for n in ["lobby", "quick-formation", "raid-quick-formation", "squad", "event-open"] {
        let img = w169(n);
        assert!(!detail_visible(&img), "{n}: taken for a profile");
        assert!(!roster_visible_pixels(&img), "{n}: taken for the student list");
    }
}

#[test]
fn w169_screens_are_never_battle_and_4_3_battle_still_is() {
    for n in ["lobby", "student-list", "student-profile", "quick-formation", "raid-quick-formation", "squad", "event-open"] {
        assert!(!looks_like_battle(&w169(n)), "{n}");
    }
    assert!(looks_like_battle(&load("screens/battle.png")));
}

#[test]
fn w169_quick_formation_grid_is_read_like_the_4_3_one() {
    let q = w169("quick-formation");
    assert_eq!(quick_tab_is_striker(&q), Some(true));
    let labels = quick_card_labels(&q);
    assert_eq!(labels.len(), 12, "{labels:?}");
    assert!(labels.iter().all(|l| (305..=307).contains(&l.y0) || (458..=460).contains(&l.y0)), "{labels:?}");
    let selected: Vec<bool> = labels.iter().filter(|l| l.y0 < 400).map(|l| quick_card_selected(&q, l.cx, l.y0)).collect();
    assert_eq!(selected, vec![true, true, true, true, false, false], "squad members are outlined");
    // the 4:3 recording gives the same reading, 120 px lower
    let q43 = load("screens/quick-formation.png");
    assert_eq!(quick_tab_is_striker(&q43), Some(true));
    let l43 = quick_card_labels(&q43);
    assert_eq!(l43.len(), 12);
    assert!((l43[0].y0 - labels[0].y0 - 120).abs() <= 1, "{} vs {}", l43[0].y0, labels[0].y0);
    let raid = w169("raid-quick-formation");
    assert_eq!(quick_card_labels(&raid).len(), 6);
}

#[test]
fn w169_panel_delta_follows_the_panel() {
    let a = w169("student-profile");
    assert_eq!(panel_delta(&a, &a), 0.0);
    // the shifted panel region covers the profile's right panel: a different screen differs a lot there
    assert!(panel_delta(&a, &w169("student-list")) > 0.2);
}
