//! The battle matchup as the card shows it: names, outcome words and colours, the offense and
//! defense tables, the line per student. Wiki data in, rows out, no drawing.
//!
//! One colour language everywhere: green is good for the player, slate is even, orange and red
//! are bad, whichever way the damage flows. Every colour comes with a Korean word, so nothing
//! depends on colour alone.
use baocr_overlay::{Chip, FitEntry, Grade, MatrixRow, Row, Tone};
use baocr_wikidb::{attack_multiplier, Fit, MissionBrief, Rating, StudentBrief, SupportRole, Tally, TypeTable};

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

fn top(list: &[Tally], n: usize) -> Vec<&Tally> {
    let mut v: Vec<&Tally> = list.iter().collect();
    v.sort_by(|a, b| b.n.cmp(&a.n));
    v.truncate(n);
    v
}

/// What each of `attacks` does to the enemy armor types of the mission (most numerous first).
/// None when the mission's armors are unknown.
pub fn offense_matrix(brief: &MissionBrief, attacks: &[&str], types: &TypeTable) -> Option<Row> {
    let armors = top(&brief.armor, MAX_COLS);
    if armors.is_empty() || attacks.is_empty() {
        return None;
    }
    let cols = armors.iter().map(|a| Chip { text: format!("{} {}", a.ko, a.n), tone: Tone::from_code(&a.code) }).collect();
    let rows = attacks
        .iter()
        .map(|b| MatrixRow { label: attack_chip(b, ""), cells: armors.iter().map(|a| cell(types.get(b, &a.code), Direction::Offense)).collect() })
        .collect();
    Some(Row::Matrix { heading: "공격 · 내 공격 → 적 장갑".into(), cols, rows })
}

/// What the mission's enemy attack types do to each of `armors`. None when the enemies' attack
/// types are unknown.
pub fn defense_matrix(brief: &MissionBrief, armors: &[&str], types: &TypeTable) -> Option<Row> {
    let attacks = top(&brief.attack, MAX_ENEMY_ATTACKS);
    if attacks.is_empty() || armors.is_empty() {
        return None;
    }
    let cols = armors.iter().take(MAX_COLS).map(|a| armor_chip(a, "")).collect();
    let rows = attacks
        .iter()
        .map(|b| MatrixRow { label: Chip { text: format!("{} {}", b.ko, b.n), tone: Tone::from_code(&b.code) }, cells: armors.iter().take(MAX_COLS).map(|a| cell(types.get(&b.code, a), Direction::Defense)).collect() })
        .collect();
    Some(Row::Matrix { heading: "방어 · 적 공격 → 내 장갑".into(), cols, rows })
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
