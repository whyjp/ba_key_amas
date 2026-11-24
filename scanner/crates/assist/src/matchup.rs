//! The battle matchup as the card shows it: names, outcome words and colours, the offense and
//! defense tables, the line per student. Wiki data in, rows out, no drawing.
//!
//! One colour language everywhere: green is good for the player, slate is even, orange and red
//! are bad, whichever way the damage flows. Every colour comes with a Korean word, so nothing
//! depends on colour alone.
use baocr_overlay::{Chip, FitEntry, Grade, MatrixRow, Row, Tone};
use baocr_wikidb::{attack_multiplier, Fit, MissionBrief, Rating, Share, StudentBrief, SupportRole, TypeTable};

/// Korean name of an attack (bullet) type code.
pub fn attack_ko(code: &str) -> &'static str {
    match code {
        "Explosion" => "폭발",
        "Pierce" => "관통",
        "Mystic" => "신비",
        "Sonic" => "진동",
        "Chemical" => "화학",
        _ => "일반",
    }
}

/// Korean name of an armor type code.
pub fn armor_ko(code: &str) -> &'static str {
    match code {
        "LightArmor" => "경장갑",
        "HeavyArmor" => "중장갑",
        "Unarmed" => "특수장갑",
        "ElasticArmor" => "탄력장갑",
        "CompositeArmor" => "복합장갑",
        "Structure" => "구조물",
        _ => "일반장갑",
    }
}

pub fn grade_of(fit: Fit) -> Grade {
    match fit {
        Fit::Excellent => Grade::Excellent,
        Fit::Good => Grade::Good,
        Fit::Average => Grade::Average,
        Fit::Poor => Grade::Poor,
    }
}

pub fn grade_label(grade: Grade) -> &'static str {
    match grade {
        Grade::Excellent => "강적합",
        Grade::Good => "적합",
        Grade::Average => "보통",
        Grade::Poor => "부적합",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// My attack type against the enemy's armor.
    Offense,
    /// The enemy's attack type against my armor.
    Defense,
}

/// The colour and word for a damage multiplier. What counts is the benefit to the player:
/// the multiplier itself when attacking, its inverse when being attacked.
pub fn outcome(mult: f64, dir: Direction) -> (Tone, &'static str) {
    let benefit = match dir {
        Direction::Offense => mult,
        Direction::Defense => 1.0 / mult.max(0.05),
    };
    match (benefit, dir) {
        (b, Direction::Offense) if b >= 1.75 => (Tone::Excellent, "약점"),
        (b, Direction::Defense) if b >= 1.75 => (Tone::Excellent, "강함"),
        (b, _) if b >= 1.15 => (Tone::Good, "유리"),
        (b, _) if b > 0.85 => (Tone::Even, "보통"),
        (b, _) if b > 0.6 => (Tone::Poor, "불리"),
        (_, Direction::Offense) => (Tone::Bad, "저항"),
        (_, Direction::Defense) => (Tone::Bad, "취약"),
    }
}

/// "×2.0", "×0.5", "×0.75".
pub fn mult_text(m: f64) -> String {
    let t = format!("{m:.2}");
    let t = if t.ends_with('0') { t[..t.len() - 1].to_string() } else { t };
    format!("×{t}")
}

fn cell(mult: f64, dir: Direction) -> Chip {
    let (tone, word) = outcome(mult, dir);
    Chip { text: format!("{word} {}", mult_text(mult)), tone }
}

fn attack_chip(code: &str, suffix: &str) -> Chip {
    Chip { text: format!("{}{suffix}", attack_ko(code)), tone: Tone::from_code(code) }
}

fn armor_chip(code: &str, suffix: &str) -> Chip {
    Chip { text: format!("{}{suffix}", armor_ko(code)), tone: Tone::from_code(code) }
}

/// Most columns a table gets: more would shrink the numbers past reading.
const MAX_COLS: usize = 4;
const MAX_ENEMY_ATTACKS: usize = 3;
/// Most chips on a facts row.
pub const MAX_CHIPS: usize = 4;

/// Korean name of a terrain code.
pub fn terrain_ko(code: &str) -> &str {
    match code {
        "Street" => "시가지",
        "Outdoor" => "야외",
        "Indoor" => "실내",
        other => other,
    }
}

/// Types below this share of the enemy weight are not listed alone: they merge into one "기타" entry.
pub const MIN_SHOWN_SHARE: f64 = 0.05;

/// One listed armor or attack type: a single type, or the merged tail of the small ones (`code` None).
#[derive(Debug, Clone, PartialEq)]
pub struct Listed {
    pub code: Option<String>,
    pub ko: String,
    pub share: f64,
    /// The types behind the entry with their shares (one for a single type).
    pub members: Vec<(String, f64)>,
}

/// The types worth showing, biggest first, at most `max` entries: those with at least `MIN_SHOWN_SHARE`,
/// the rest merged into "기타". One small type alone keeps its own name.
pub fn tidy(list: &[Share], max: usize) -> Vec<Listed> {
    let mut sorted: Vec<&Share> = list.iter().collect();
    sorted.sort_by(|a, b| b.share.total_cmp(&a.share));
    let mut keep = sorted.iter().take_while(|s| s.share >= MIN_SHOWN_SHARE).count().max(1).min(max).min(sorted.len());
    if keep < sorted.len() {
        // the tail takes one place of the `max`
        keep = keep.min(max.saturating_sub(1)).max(1);
    }
    let (head, tail) = sorted.split_at(keep);
    let one = |s: &Share| Listed { code: Some(s.code.clone()), ko: s.ko.clone(), share: s.share, members: vec![(s.code.clone(), s.share)] };
    let mut out: Vec<Listed> = head.iter().map(|s| one(s)).collect();
    match tail {
        [] => {}
        [only] => out.push(one(only)),
        many => out.push(Listed { code: None, ko: "기타".into(), share: many.iter().map(|s| s.share).sum(), members: many.iter().map(|s| (s.code.clone(), s.share)).collect() }),
    }
    out
}

/// Whole percents of `shares` that add up to 100 (largest remainder), so a list reads as a whole.
pub fn percents(shares: &[f64]) -> Vec<u32> {
    let total: f64 = shares.iter().sum();
    if total <= 0.0 {
        return vec![0; shares.len()];
    }
    let exact: Vec<f64> = shares.iter().map(|s| s / total * 100.0).collect();
    let mut out: Vec<u32> = exact.iter().map(|x| x.floor() as u32).collect();
    let mut order: Vec<usize> = (0..exact.len()).collect();
    order.sort_by(|&a, &b| (exact[b] - exact[b].floor()).total_cmp(&(exact[a] - exact[a].floor())));
    let missing = 100u32.saturating_sub(out.iter().sum());
    for &i in order.iter().take(missing as usize) {
        out[i] += 1;
    }
    out
}

/// "경장갑 64%" for each listed entry.
pub fn share_labels(listed: &[Listed]) -> Vec<(String, Option<String>)> {
    let pct = percents(&listed.iter().map(|l| l.share).collect::<Vec<_>>());
    listed.iter().zip(pct).map(|(l, p)| (format!("{} {p}%", l.ko), l.code.clone())).collect()
}

/// The weighted mean of `mult` over the members of an entry.
fn mean_over(l: &Listed, mult: impl Fn(&str) -> f64) -> f64 {
    let total: f64 = l.members.iter().map(|(_, s)| s).sum();
    l.members.iter().map(|(c, s)| s * mult(c)).sum::<f64>() / total.max(f64::MIN_POSITIVE)
}

/// What each of `attacks` does to the enemy armor types of the mission (biggest share of the enemy HP
/// first; the small ones merged into one "기타" column). None when the mission's armors are unknown.
pub fn offense_matrix(brief: &MissionBrief, attacks: &[&str], types: &TypeTable) -> Option<Row> {
    let armors = tidy(&brief.armor, MAX_COLS);
    if armors.is_empty() || attacks.is_empty() {
        return None;
    }
    let cols = share_labels(&armors).into_iter().map(|(text, code)| Chip { text, tone: code.map_or(Tone::Even, |c| Tone::from_code(&c)) }).collect();
    let rows = attacks
        .iter()
        .map(|b| MatrixRow { label: attack_chip(b, ""), cells: armors.iter().map(|a| cell(mean_over(a, |armor| types.get(b, armor)), Direction::Offense)).collect() })
        .collect();
    Some(Row::Matrix { heading: "공격 · 내 공격 → 적 장갑 (HP 비중)".into(), cols, rows })
}

/// What the mission's enemy attack types do to each of `armors`. None when the enemies' attack
/// types are unknown.
pub fn defense_matrix(brief: &MissionBrief, armors: &[&str], types: &TypeTable) -> Option<Row> {
    let attacks = tidy(&brief.attack, MAX_ENEMY_ATTACKS);
    if attacks.is_empty() || armors.is_empty() {
        return None;
    }
    let cols = armors.iter().take(MAX_COLS).map(|a| armor_chip(a, "")).collect();
    let labels = share_labels(&attacks);
    let rows = attacks
        .iter()
        .zip(labels)
        .map(|(b, (text, code))| MatrixRow {
            label: Chip { text, tone: code.map_or(Tone::Even, |c| Tone::from_code(&c)) },
            cells: armors.iter().take(MAX_COLS).map(|a| cell(mean_over(b, |bullet| types.get(bullet, a)), Direction::Defense)).collect(),
        })
        .collect();
    Some(Row::Matrix { heading: "방어 · 적 공격 → 내 장갑 (HP 비중)".into(), cols, rows })
}

/// The colour scale and what the numbers mean.
pub fn legend() -> Row {
    let chip = |text: &str, tone| Chip { text: text.into(), tone };
    Row::Legend {
        chips: vec![chip("매우 유리", Tone::Excellent), chip("유리", Tone::Good), chip("보통", Tone::Even), chip("불리", Tone::Poor), chip("매우 불리", Tone::Bad)],
        text: "숫자는 피해 배율입니다 (×2.0 = 2배, ×0.5 = 절반). 초록은 내게 유리, 빨강은 불리합니다".into(),
    }
}

/// Distinct codes in the order first seen.
pub fn distinct<'a>(codes: impl IntoIterator<Item = &'a str>) -> Vec<&'a str> {
    let mut out: Vec<&str> = Vec::new();
    for c in codes {
        if !out.contains(&c) {
            out.push(c);
        }
    }
    out
}

/// The squad's attack types, the best against this mission's enemies first.
pub fn attack_types_by_edge<'a>(brief: &MissionBrief, codes: impl IntoIterator<Item = &'a str>, types: &TypeTable) -> Vec<&'a str> {
    let mut v = distinct(codes);
    v.sort_by(|a, b| attack_multiplier(brief, b, types).total_cmp(&attack_multiplier(brief, a, types)));
    v
}

/// The four attack types of the game, best against the enemies first.
pub fn all_attack_types_by_edge(brief: &MissionBrief, types: &TypeTable) -> Vec<&'static str> {
    let mut v = vec!["Explosion", "Pierce", "Mystic", "Sonic"];
    v.sort_by(|a, b| attack_multiplier(brief, b, types).total_cmp(&attack_multiplier(brief, a, types)));
    v
}

fn role_note(role: SupportRole) -> &'static str {
    match role {
        SupportRole::Healer => "회복형 · 스킬로 판단",
        SupportRole::Supporter => "지원형 · 스킬로 판단",
        SupportRole::Unknown => "역할 미상 · 스킬로 판단",
    }
}

/// A student's line on the card: the grade, the name and the two types behind it; for a special
/// that is not rated, "SP" and why.
pub fn fit_entry(s: &StudentBrief, rating: Rating) -> FitEntry {
    match rating {
        Rating::Rated(fit) => {
            let grade = grade_of(fit);
            FitEntry { grade: Some(grade), badge: grade_label(grade).into(), name: s.name.clone(), tags: vec![attack_chip(&s.bullet_type, ""), armor_chip(&s.armor_type, "")], note: String::new() }
        }
        Rating::NotRated(role) => FitEntry { grade: None, badge: "SP".into(), name: s.name.clone(), tags: Vec::new(), note: role_note(role).into() },
    }
}

/// The armor types of the squad's strikers: the ones the enemy shoots at.
pub fn striker_armors(team: &[StudentBrief]) -> Vec<&str> {
    distinct(team.iter().filter(|s| s.squad_type == "Main").map(|s| s.armor_type.as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn share(code: &str, share: f64) -> Share {
        Share { code: code.into(), ko: armor_ko(code).into(), share, heads: 1 }
    }

    #[test]
    fn percents_add_up_to_one_hundred() {
        assert_eq!(percents(&[1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0]).iter().sum::<u32>(), 100);
        assert_eq!(percents(&[0.642, 0.358]), vec![64, 36]);
        assert_eq!(percents(&[]), Vec::<u32>::new());
    }

    #[test]
    fn small_types_merge_into_one_other_entry_and_a_lone_small_type_keeps_its_name() {
        let list = [share("LightArmor", 0.58), share("HeavyArmor", 0.30), share("Unarmed", 0.08), share("ElasticArmor", 0.03), share("Structure", 0.01)];
        let t = tidy(&list, 4);
        assert_eq!(t.iter().map(|l| (l.ko.as_str(), (l.share * 100.0).round() as i32)).collect::<Vec<_>>(), vec![("경장갑", 58), ("중장갑", 30), ("특수장갑", 8), ("기타", 4)]);
        assert_eq!(t[3].members.len(), 2);
        let lone = tidy(&[share("LightArmor", 0.97), share("HeavyArmor", 0.03)], 4);
        assert_eq!(lone.iter().map(|l| l.ko.as_str()).collect::<Vec<_>>(), vec!["경장갑", "중장갑"]);
        // many equal types: the merged entry takes the last of the `max` places
        let even: Vec<Share> = ["LightArmor", "HeavyArmor", "Unarmed", "ElasticArmor", "Structure"].iter().map(|c| share(c, 0.2)).collect();
        assert_eq!(tidy(&even, 4).len(), 4);
        assert!(tidy(&[], 4).is_empty());
    }

    #[test]
    fn a_merged_column_shows_the_weighted_mean_of_its_members() {
        let types = TypeTable::new([("Explosion".into(), "ElasticArmor".into(), 0.5), ("Explosion".into(), "Structure".into(), 1.5)]);
        let l = Listed { code: None, ko: "기타".into(), share: 0.04, members: vec![("ElasticArmor".into(), 0.03), ("Structure".into(), 0.01)] };
        assert!((mean_over(&l, |a| types.get("Explosion", a)) - 0.75).abs() < 1e-9);
    }
}
