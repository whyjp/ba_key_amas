//! Profile extraction: one 1280x960 (4:3) profile frame -> `Student`.
//!
//! One detection+recognition pass over the frame reads the panel numbers; the two
//! small overlaid texts the frame pass misses (name strip, bond heart) are read
//! closed-set: ink-isolated line, scored against every dictionary name (or 1..=100)
//! with the recogniser's own CTC probabilities. Nothing is guessed: a read whose
//! best candidate is not clearly ahead stays empty and carries a warning.
use anyhow::Result;
mod deep;
pub mod scene;
pub use deep::{apply_cell, match_label};
use baocr_core::model::*;
use baocr_core::ocr_api::{text_in, Ocr, TextBox};
use baocr_ocr::ink_to_line;
use baocr_pixels as px;
use image::RgbImage;
use once_cell::sync::Lazy;
use regex::Regex;

/// Names of the frames a student's capture is stored as (`<name>.png` in details/NNN/).
pub const FRAME_DETAIL: &str = "detail";
pub const FRAME_POPUP_TOP: &str = "popup-top";
pub const FRAME_POPUP_BOTTOM: &str = "popup-bottom";
pub const FRAME_LEVEL_UP: &str = "tab-levelup";
pub const FRAME_MYSTIC: &str = "tab-mystic";

/// The four menu frames captured by the menu pass (상세 popup top/bottom,
/// 레벨 업 tab, 신비 해방 tab).
#[derive(Clone)]
pub struct DeepFrames {
    pub popup_top: RgbImage,
    pub popup_bottom: RgbImage,
    pub level_up: RgbImage,
    pub mystic: RgbImage,
}

/// The bottom navigation labels of the main menu (lobby), left to right.
const LOBBY_LABELS: [&str; 8] = ["카페", "스케줄", "학생", "편성", "소설", "제조", "상점", "모집"];

/// Recognise the main menu by its bottom navigation labels and return where to
/// click to enter the student menu (frame pixels of the 1280x960 layout).
///
/// At least four of the eight labels must be read along the bottom bar, so a
/// student list, a profile or the account page is never mistaken for the lobby,
/// whatever the lobby wallpaper looks like. The click lands on the 학생 icon,
/// just above its label.
pub fn lobby_student_button(ocr: &dyn Ocr, img: &RgbImage) -> Option<(i32, i32)> {
    let boxes = ocr.boxes(img).ok()?;
    let bar: Vec<&TextBox> = boxes.iter().filter(|b| b.y0 > 880.0 && b.y1 < 958.0).collect();
    let found = LOBBY_LABELS.iter().filter(|l| bar.iter().any(|b| b.text.trim() == **l)).count();
    if found < 4 {
        return None;
    }
    let b = bar.iter().find(|b| b.text.trim() == "학생")?;
    Some((((b.x0 + b.x1) / 2.0).round() as i32, (b.y0 - 14.0).round() as i32))
}

/// Fewest ink pixels a name strip / bond heart must show before it is read at all.
/// A blank or off-layout frame would otherwise still yield the "most likely"
/// dictionary entry, which is a guess, not a reading.
const MIN_NAME_INK: usize = 40;
const MIN_BOND_INK: usize = 8;

fn ink_pixels(g: &image::GrayImage) -> usize {
    g.pixels().filter(|p| p[0] < 128).count()
}

/// Log-probability lead the best candidate needs over the best *different* one.
const MIN_MARGIN: f32 = 1.0;

#[derive(Debug, Clone, Default)]
pub struct Options {
    /// Also read the fixed groups from pixels and warn on disagreement with the dictionary.
    pub verify_fixed: bool,
}

static INTS: Lazy<Regex> = Lazy::new(|| Regex::new(r"\d+").unwrap());
static UNIQUE_NAMES: Lazy<Vec<String>> = Lazy::new(|| {
    let mut v = baocr_names::all_names();
    v.sort();
    v.dedup();
    v
});
static BOND_CANDIDATES: Lazy<Vec<String>> = Lazy::new(|| (1..=100).map(|n| n.to_string()).collect());

fn last_int(s: &str) -> Option<Int> {
    INTS.find_iter(&s.replace(',', "")).last().and_then(|m| m.as_str().parse().ok())
}

fn all_ints(s: &str) -> Vec<Int> {
    INTS.find_iter(&s.replace(',', "")).filter_map(|m| m.as_str().parse().ok()).collect()
}

/// Best candidate and its lead over the best candidate with a different text.
fn best_with_margin(cands: &[String], scores: &[f32]) -> Option<(usize, f32)> {
    let best = scores.iter().enumerate().filter(|(_, s)| s.is_finite()).max_by(|a, b| a.1.partial_cmp(b.1).unwrap())?.0;
    let runner = scores
        .iter()
        .enumerate()
        .filter(|(i, s)| cands[*i] != cands[best] && s.is_finite())
        .map(|(_, s)| *s)
        .fold(f32::NEG_INFINITY, f32::max);
    Some((best, scores[best] - runner))
}

pub struct Extractor<'a> {
    ocr: &'a dyn Ocr,
}

impl<'a> Extractor<'a> {
    pub fn new(ocr: &'a dyn Ocr) -> Self {
        Self { ocr }
    }

    /// Resolved student name of a frame, or "" when it cannot be read.
    pub fn name_only(&self, img: &RgbImage) -> String {
        let mut s = Student::default();
        match self.read_name(img, &mut s) {
            Ok(()) => s.name,
            Err(_) => String::new(),
        }
    }

    /// Parse the menu-pass frames into `s.deep` and cross-check them with the panel.
    pub fn deep(&self, s: &mut Student, f: &DeepFrames) -> Result<()> {
        deep::extract_deep(self.ocr, s, f)
    }

    /// Read the name strip closed-set against the dictionary.
    fn read_name(&self, img: &RgbImage, s: &mut Student) -> Result<()> {
        let ink = px::name_ink_image(img);
        if ink_pixels(&ink) < MIN_NAME_INK {
            s.name_match = Some(NameMatch { status: "empty".into(), ..Default::default() });
            return Ok(());
        }
        let line = ink_to_line(&ink);
        let (raw, _) = self.ocr.line(&line)?;
        s.raw_ocr.insert("name_line".into(), raw.clone());
        let (best, scores) = self.ocr.choose(&line, &UNIQUE_NAMES)?;
        let mut m = NameMatch { status: "empty".into(), cleaned: raw, ..Default::default() };
        if let Some((i, margin)) = best_with_margin(&UNIQUE_NAMES, &scores) {
            debug_assert_eq!(i, best);
            m.candidate = UNIQUE_NAMES[i].clone();
            m.margin = margin.round() as Int;
            if margin >= MIN_MARGIN {
                m.status = "exact".into();
                s.name = UNIQUE_NAMES[i].clone();
            } else {
                m.status = "ambiguous".into();
            }
        }
        s.name_match = Some(m);
        Ok(())
    }

    /// Read the bond number inside the heart closed-set over 1..=100.
    fn read_bond(&self, img: &RgbImage, s: &mut Student) -> Result<()> {
        let ink = px::heart_ink_image(img);
        if ink_pixels(&ink) < MIN_BOND_INK {
            return Ok(());
        }
        let line = ink_to_line(&ink);
        let (raw, _) = self.ocr.line(&line)?;
        s.raw_ocr.insert("bond".into(), raw);
        let (_, scores) = self.ocr.choose(&line, &BOND_CANDIDATES)?;
        match best_with_margin(&BOND_CANDIDATES, &scores) {
            Some((i, margin)) if margin >= MIN_MARGIN => s.bond_rank = BOND_CANDIDATES[i].parse().ok(),
            Some((i, _)) => s.warnings.push(format!("bond_rank_low_confidence: best={}", BOND_CANDIDATES[i])),
            None => {}
        }
        Ok(())
    }

    /// Read one profile frame.
    pub fn profile(&self, img: &RgbImage, source: &str, opts: &Options) -> Result<Student> {
        let boxes: Vec<TextBox> = self.ocr.boxes(img)?;
        let at = |roi: px::Rect| text_in(&boxes, roi);
        let mut s = Student { source: source.into(), ..Default::default() };

        // Names and bond are read closed-set, which always has a "most likely" answer:
        // only do it on a frame that really is a profile.
        if px::detail_visible(img) {
            self.read_name(img, &mut s)?;
            self.read_bond(img, &mut s)?;
        } else {
            s.warnings.push("not_a_profile_frame".into());
        }

        // basic stats
        let stat_rois = [(684, 350, 860, 374), (890, 350, 1060, 374), (684, 386, 860, 410), (890, 386, 1060, 410)];
        let texts: Vec<String> = stat_rois.iter().map(|&r| at(r)).collect();
        for (k, t) in ["stat_hp", "stat_attack", "stat_defense", "stat_healing"].iter().zip(&texts) {
            s.raw_ocr.insert((*k).into(), t.clone());
        }
        s.stats = Stats { max_hp: last_int(&texts[0]), attack: last_int(&texts[1]), defense: last_int(&texts[2]), healing: last_int(&texts[3]) };

        // exp and level
        let exp_text = at((210, 833, 390, 858));
        s.raw_ocr.insert("exp".into(), exp_text.clone());
        let ints = all_ints(&exp_text);
        if ints.len() >= 2 && ints[1] > 0 && ints[0] <= ints[1] {
            s.exp = Exp { current: Some(ints[0]), required: Some(ints[1]) };
        }
        let level_text = at((30, 827, 104, 865));
        s.raw_ocr.insert("level".into(), level_text.clone());
        s.level = last_int(&level_text).filter(|l| (1..=150).contains(l));

        // skills (locked cards have no level)
        s.skill_locked = px::read_skill_locks(img);
        let skill_rois = [(694, 520, 767, 548), (800, 520, 883, 548), (908, 520, 987, 548), (1010, 520, 1100, 548)];
        let lv = |i: usize, locked: bool, s: &mut Student| -> Option<Int> {
            if locked {
                return None;
            }
            let t = text_in(&boxes, skill_rois[i]);
            s.raw_ocr.insert(["ex", "basic", "passive", "sub"][i].into(), t.clone());
            last_int(&t).filter(|v| (1..=20).contains(v))
        };
        let sl = s.skill_locked.clone();
        s.ex_skill_level = lv(0, sl.ex, &mut s);
        s.normal_skill_level = lv(1, sl.basic, &mut s);
        s.passive_skill_level = lv(2, sl.passive, &mut s);
        s.sub_skill_level = lv(3, sl.sub, &mut s);

        // weapon
        s.unique_weapon_stars = px::weapon_stars(img);
        let wl = at((766, 563, 850, 608));
        s.raw_ocr.insert("weapon_level".into(), wl.clone());
        if wl.contains('L') || wl.contains('v') {
            s.unique_weapon_level = last_int(&wl).filter(|v| (1..=100).contains(v));
        }

        // pixel-only groups
        s.rarity_stars = px::stars(img);
        s.is_pinned = px::pin_state(img);
        s.combat_role = combat_role(img);
        s.badges = px::read_badges(img);
        s.locks = px::read_locks(img);
        let (equipment, raw) = baocr_equipment::detect_equipment(img, self.ocr)?;
        s.equipment = equipment;
        s.raw_ocr.extend(raw);
        s.identity = baocr_names::parse_name(&s.name);

        // fixed stage: dictionary, optionally audited against pixels
        let role_hint = px::role_from_icon(img);
        if opts.verify_fixed {
            s.loadout = self.read_loadout(img, &boxes);
            s.terrain = px::read_terrain(img);
            s.reference = baocr_names::lookup_reference(&s.name, &s.loadout);
            baocr_names::cross_check(&mut s);
            baocr_names::apply_fixed(&mut s, &role_hint);
            if let Some(f) = s.fixed_meta.as_mut() {
                f.source = "pixels+dictionary".into();
            }
        } else {
            baocr_names::apply_fixed(&mut s, &role_hint);
            s.reference = baocr_names::lookup_reference(&s.name, &s.loadout);
        }
        baocr_names::level_from_exp(&mut s);
        Ok(s)
    }

    fn read_loadout(&self, img: &RgbImage, boxes: &[TextBox]) -> Loadout {
        let (attack, armor) = px::attack_armor_types(img);
        Loadout {
            tactic_role: px::role_from_icon(img),
            position: baocr_names::closest_word(&text_in(boxes, (35, 903, 160, 933)), &baocr_names::POSITION_VOCAB),
            attack_type: attack,
            armor_type: armor,
            weapon_class: baocr_names::closest_word(&text_in(boxes, (690, 610, 750, 640)), &baocr_names::WEAPON_VOCAB),
        }
    }
}

/// STRIKER (red lettering) or SPECIAL (blue lettering) on the white role badge.
/// Counts red and blue glyph pixels, so the gold stars beside it never matter.
fn combat_role(img: &RgbImage) -> String {
    let roi = (332, 803, 422, 832);
    let red = px::fraction(img, roi, |c| c[0] > 190 && c[1] < 120 && c[2] < 100);
    let blue = px::fraction(img, roi, |c| c[2] > 190 && c[0] < 100 && c[1] > 90);
    if red > 0.01 && red > blue {
        "STRIKER".into()
    } else if blue > 0.01 && blue > red {
        "SPECIAL".into()
    } else {
        String::new()
    }
}
