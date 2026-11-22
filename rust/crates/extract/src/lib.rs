//! Profile extraction: one 1280x960 (4:3) profile frame -> `Student`.
//!
//! One detection+recognition pass over the frame reads the panel numbers; the two
//! small overlaid texts the frame pass misses (name strip, bond heart) are read
//! closed-set: ink-isolated line, scored against every dictionary name (or 1..=100)
//! with the recogniser's own CTC probabilities. Nothing is guessed: a read whose
//! best candidate is not clearly ahead stays empty and carries a warning.
use anyhow::Result;
use baocr_core::model::*;
use baocr_core::ocr_api::{text_in, Ocr, TextBox};
use baocr_ocr::ink_to_line;
use baocr_pixels as px;
use image::RgbImage;
use once_cell::sync::Lazy;
use regex::Regex;

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

    /// Read the name strip closed-set against the dictionary.
    fn read_name(&self, img: &RgbImage, s: &mut Student) -> Result<()> {
        let line = ink_to_line(&px::name_ink_image(img));
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
        let line = ink_to_line(&px::heart_ink_image(img));
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

        self.read_name(img, &mut s)?;
        self.read_bond(img, &mut s)?;

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
