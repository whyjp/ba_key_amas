//! Is a squad a good fit for a mission? A rough guide, not a simulation: it compares the
//! squad's attack and armor types and its terrain adaptation with the mission's enemies,
//! relative to what the average released student would do on the same mission.
use crate::wiki::{MissionBrief, StudentBrief, Tally, Wiki};
use anyhow::Result;
use std::collections::HashMap;

/// Damage multiplier (1.0 = 100 %) of an attack type against an armor type.
#[derive(Debug, Clone, Default)]
pub struct TypeTable(HashMap<(String, String), f64>);

impl TypeTable {
    pub fn new(rows: impl IntoIterator<Item = (String, String, f64)>) -> TypeTable {
        TypeTable(rows.into_iter().map(|(b, a, m)| ((b, a), m)).collect())
    }

    pub fn get(&self, bullet: &str, armor: &str) -> f64 {
        self.0.get(&(bullet.to_string(), armor.to_string())).copied().unwrap_or(1.0)
    }
}

impl Wiki {
    pub fn type_table(&self) -> Result<TypeTable> {
        self.type_rows().map(TypeTable::new)
    }
}

/// Damage factor of a terrain adaptation grade (0 worst .. 4 best, the game's five faces).
/// The grades are approximated as 80 / 90 / 100 / 110 / 120 %.
pub fn adaptation_factor(level: u8) -> f64 {
    match level {
        4 => 1.2,
        3 => 1.1,
        2 => 1.0,
        1 => 0.9,
        _ => 0.8,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Favorable,
    Average,
    Unfavorable,
}

/// Every figure is relative to the average released student on the same mission:
/// 1.0 is average, above is better.
#[derive(Debug, Clone, PartialEq)]
pub struct FormationScore {
    /// Damage dealt: the squad's attack types against the enemies' armor, with terrain.
    pub attack: f64,
    /// Damage taken: the enemies' attack types against the squad's armor (above 1.0 = takes less).
    pub defense: f64,
    /// Terrain adaptation alone.
    pub terrain: f64,
    pub overall: f64,
    /// Students the verdict rests on (see `rates_by_type`).
    pub members: usize,
    /// Specials left out because their skill role, not their types, decides their use.
    pub unrated: usize,
    pub verdict: Verdict,
}

/// How well one student's types and terrain suit the current battle. Type advantage is
/// necessary, not sufficient: skills, cost, range and levels decide the rest (guide 3.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Fit {
    /// 부적합: a type disadvantage, or the terrain and the enemy's attack add up against the student.
    Poor,
    /// 보통
    Average,
    /// 적합
    Good,
    /// 강적합
    Excellent,
}

/// Why a special is not rated: its worth is its skill, which the database cannot score.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SupportRole {
    Healer,
    Supporter,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Rating {
    Rated(Fit),
    NotRated(SupportRole),
}

/// Strikers always have a type to rate. A special does only when it attacks directly
/// (DamageDealer) or summons an attacker (Vehicle, the T.S. units); healers and supporters
/// are not hit and not chosen for their attack type (guide 4 and 4.2). An unknown role is
/// not guessed.
pub fn special_role(s: &StudentBrief) -> Option<SupportRole> {
    if s.squad_type == "Main" {
        return None;
    }
    match s.role.as_str() {
        "DamageDealer" | "Vehicle" => None,
        "Healer" => Some(SupportRole::Healer),
        "Supporter" => Some(SupportRole::Supporter),
        _ => Some(SupportRole::Unknown),
    }
}

pub fn rates_by_type(s: &StudentBrief) -> bool {
    special_role(s).is_none()
}

/// The three figures a fit is made of. `attack` is the damage multiplier of the student's
/// attack type against the mission's enemies (weighted by how many of each armor), `taken`
/// the multiplier of the enemies' attack types against the student's armor (below 1.0 the
/// armor holds), `terrain` the adaptation grade 0..=4 when the terrain is known.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Matchup {
    pub attack: f64,
    pub taken: f64,
    pub terrain: Option<u8>,
}

/// Points per factor, summed. Offense gates the tier: without a type edge nothing else makes
/// a student a good pick, and a type disadvantage cannot be bought back.
///   attack  >= 1.5 : +2   >= 1.2 : +1   <= 0.9 : -1 (and below 0.9 the tier is Poor)
///   terrain >= 3 (A, S) : +1   <= 1 (D, C) : -1
///   taken   <= 0.75 : +1   >= 1.5 : -1
/// Excellent: attack +2 and a total of at least 3 (the edge plus one more reason).
/// Good: attack at least +1 and a total of at least 2. Poor: attack below 0 or total below 0.
pub fn fit_tier(m: Matchup) -> Fit {
    let attack = if m.attack >= 1.5 {
        2
    } else if m.attack >= 1.2 {
        1
    } else if m.attack <= 0.9 {
        -1
    } else {
        0
    };
    let terrain = match m.terrain {
        Some(3..=4) => 1,
        Some(0..=1) => -1,
        _ => 0,
    };
    let taken = if m.taken <= 0.75 {
        1
    } else if m.taken >= 1.5 {
        -1
    } else {
        0
    };
    let total = attack + terrain + taken;
    if attack < 0 || total < 0 {
        Fit::Poor
    } else if attack == 2 && total >= 3 {
        Fit::Excellent
    } else if attack >= 1 && total >= 2 {
        Fit::Good
    } else {
        Fit::Average
    }
}

/// Mission's enemy armor types with their share of the enemies.
pub fn enemy_armors(brief: &MissionBrief) -> Vec<(&str, f64)> {
    shares(&brief.armor)
}

/// Mission's enemy attack types with their share of the enemies.
pub fn enemy_attacks(brief: &MissionBrief) -> Vec<(&str, f64)> {
    shares(&brief.attack)
}

/// Damage multiplier of an attack type against the mission's enemies.
pub fn attack_multiplier(brief: &MissionBrief, bullet: &str, types: &TypeTable) -> f64 {
    enemy_armors(brief).iter().map(|(a, w)| w * types.get(bullet, a)).sum()
}

/// Damage multiplier the mission's enemies deal to an armor type (1.0 when their attacks are unknown).
pub fn damage_taken(brief: &MissionBrief, armor: &str, types: &TypeTable) -> f64 {
    let bullets = enemy_attacks(brief);
    if bullets.is_empty() {
        1.0
    } else {
        bullets.iter().map(|(b, w)| w * types.get(b, armor)).sum()
    }
}

pub fn matchup(s: &StudentBrief, brief: &MissionBrief, terrain: Option<&str>, types: &TypeTable) -> Matchup {
    Matchup { attack: attack_multiplier(brief, &s.bullet_type, types), taken: damage_taken(brief, &s.armor_type, types), terrain: terrain.and_then(|t| s.adaptation(t)) }
}

pub fn rate(s: &StudentBrief, brief: &MissionBrief, terrain: Option<&str>, types: &TypeTable) -> Rating {
    match special_role(s) {
        Some(role) => Rating::NotRated(role),
        // an attacker special is rated on its attack and terrain only: it is not the one being shot at
        None if s.squad_type != "Main" => Rating::Rated(fit_tier(Matchup { taken: 1.0, ..matchup(s, brief, terrain, types) })),
        None => Rating::Rated(fit_tier(matchup(s, brief, terrain, types))),
    }
}

const FAVORABLE_AT: f64 = 1.08;
const UNFAVORABLE_AT: f64 = 0.94;

fn shares(list: &[Tally]) -> Vec<(&str, f64)> {
    let total: i64 = list.iter().map(|t| t.n).sum();
    if total <= 0 {
        return Vec::new();
    }
    list.iter().map(|t| (t.code.as_str(), t.n as f64 / total as f64)).collect()
}

fn mean(xs: impl Iterator<Item = f64>) -> Option<f64> {
    let v: Vec<f64> = xs.collect();
    (!v.is_empty()).then(|| v.iter().sum::<f64>() / v.len() as f64)
}

/// `terrain`: the mission's terrain code (Street | Outdoor | Indoor), when known.
///
/// Only students rated by type count (`rates_by_type`); defense only counts strikers, the
/// ones the enemy shoots at.
pub fn evaluate(brief: &MissionBrief, terrain: Option<&str>, team: &[StudentBrief], pool: &[StudentBrief], types: &TypeTable) -> Option<FormationScore> {
    let (team_rated, pool_rated): (Vec<&StudentBrief>, Vec<&StudentBrief>) = (team.iter().filter(|s| rates_by_type(s)).collect(), pool.iter().filter(|s| rates_by_type(s)).collect());
    if team_rated.is_empty() || pool_rated.is_empty() || brief.armor.is_empty() {
        return None;
    }
    let adapt = |s: &StudentBrief| terrain.and_then(|t| s.adaptation(t)).map_or(1.0, adaptation_factor);
    let attack = |s: &StudentBrief| attack_multiplier(brief, &s.bullet_type, types) * adapt(s);
    // damage taken as a multiplier; its inverse is how well the armor holds
    let defense = |s: &StudentBrief| 1.0 / damage_taken(brief, &s.armor_type, types).max(0.05);
    let rel = |who: &[&StudentBrief], base: &[&StudentBrief], f: &dyn Fn(&StudentBrief) -> f64| -> Option<f64> { Some(mean(who.iter().map(|s| f(s)))? / mean(base.iter().map(|s| f(s)))?) };
    let (team_strikers, pool_strikers): (Vec<&StudentBrief>, Vec<&StudentBrief>) =
        (team_rated.iter().copied().filter(|s| s.squad_type == "Main").collect(), pool_rated.iter().copied().filter(|s| s.squad_type == "Main").collect());
    let (a, t) = (rel(&team_rated, &pool_rated, &attack)?, rel(&team_rated, &pool_rated, &adapt)?);
    let d = if team_strikers.is_empty() || pool_strikers.is_empty() { 1.0 } else { rel(&team_strikers, &pool_strikers, &defense)? };
    let overall = a.powf(0.6) * d.powf(0.4);
    let verdict = if overall >= FAVORABLE_AT {
        Verdict::Favorable
    } else if overall <= UNFAVORABLE_AT {
        Verdict::Unfavorable
    } else {
        Verdict::Average
    };
    Some(FormationScore { attack: a, defense: d, terrain: t, overall, members: team_rated.len(), unrated: team.len() - team_rated.len(), verdict })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn student(id: i64, bullet: &str, armor: &str, outdoor: u8) -> StudentBrief {
        StudentBrief { id, name: format!("s{id}"), bullet_type: bullet.into(), armor_type: armor.into(), street: 2, outdoor, indoor: 2, squad_type: "Main".into(), role: "DamageDealer".into() }
    }

    fn special(id: i64, bullet: &str, role: &str) -> StudentBrief {
        StudentBrief { squad_type: "Support".into(), role: role.into(), ..student(id, bullet, "LightArmor", 2) }
    }

    fn m(attack: f64, terrain: Option<u8>, taken: f64) -> Matchup {
        Matchup { attack, taken, terrain }
    }

    #[test]
    fn fit_tiers_need_a_type_edge_and_say_no_more_than_the_data_does() {
        // type edge alone is Good, not Excellent: skills and levels still decide
        assert_eq!(fit_tier(m(2.0, Some(2), 1.0)), Fit::Good);
        assert_eq!(fit_tier(m(2.0, Some(4), 1.0)), Fit::Excellent);
        assert_eq!(fit_tier(m(2.0, Some(2), 0.5)), Fit::Excellent, "the armor holds against the enemy's attack");
        assert_eq!(fit_tier(m(1.5, None, 1.0)), Fit::Good);
        assert_eq!(fit_tier(m(1.3, Some(4), 1.0)), Fit::Good, "a small edge with the best terrain");
        assert_eq!(fit_tier(m(1.3, Some(2), 1.0)), Fit::Average);
        // without an edge, great terrain and armor do not make a good pick
        assert_eq!(fit_tier(m(1.0, Some(4), 0.5)), Fit::Average);
        // a disadvantage cannot be bought back; neither can a bad stack of terrain and incoming damage
        assert_eq!(fit_tier(m(0.5, Some(4), 0.5)), Fit::Poor);
        assert_eq!(fit_tier(m(1.0, Some(0), 2.0)), Fit::Poor);
        // a type edge survives bad terrain or heavy incoming damage as Average, never as Good
        assert_eq!(fit_tier(m(2.0, Some(0), 1.0)), Fit::Average);
        assert_eq!(fit_tier(m(2.0, Some(0), 2.0)), Fit::Average);
    }

    #[test]
    fn healers_and_supporters_are_not_rated_but_attackers_and_summoners_are() {
        let (mission, types) = (mission(), types());
        assert_eq!(special_role(&student(1, "Explosion", "LightArmor", 2)), None, "strikers are always rated");
        assert_eq!(rate(&special(2, "Mystic", "Healer"), &mission, None, &types), Rating::NotRated(SupportRole::Healer));
        assert_eq!(rate(&special(3, "Pierce", "Supporter"), &mission, None, &types), Rating::NotRated(SupportRole::Supporter));
        assert_eq!(rate(&special(4, "Pierce", ""), &mission, None, &types), Rating::NotRated(SupportRole::Unknown), "no role, no guess");
        assert_eq!(rate(&special(5, "Explosion", "DamageDealer"), &mission, None, &types), Rating::Rated(Fit::Good));
        assert_eq!(rate(&special(6, "Pierce", "Vehicle"), &mission, None, &types), Rating::Rated(Fit::Poor));
    }

    #[test]
    fn unrated_specials_do_not_move_the_verdict() {
        let (mission, types) = (mission(), types());
        let strikers = vec![student(1, "Explosion", "LightArmor", 2), student(2, "Explosion", "LightArmor", 2)];
        let mut with = strikers.clone();
        with.push(special(3, "Pierce", "Healer"));
        let plain = evaluate(&mission, None, &strikers, &pool(), &types).unwrap();
        let padded = evaluate(&mission, None, &with, &pool(), &types).unwrap();
        assert_eq!((padded.attack, padded.defense, padded.verdict), (plain.attack, plain.defense, plain.verdict));
        assert_eq!((padded.members, padded.unrated), (2, 1));
        let only_healers = [special(3, "Pierce", "Healer")];
        assert_eq!(evaluate(&mission, None, &only_healers, &pool(), &types), None);
    }

    fn mission() -> MissionBrief {
        let tally = |code: &str, n: i64| Tally { code: code.into(), ko: code.into(), n };
        MissionBrief {
            stage_id: 1,
            name: "m".into(),
            area: Some(1),
            stage_no: Some(1),
            difficulty: 0,
            terrain: None,
            recommended_level: None,
            attack: vec![tally("Explosion", 4)],
            armor: vec![tally("LightArmor", 4)],
            best_attack: vec![],
        }
    }

    fn types() -> TypeTable {
        let row = |b: &str, a: &str, m: f64| (b.to_string(), a.to_string(), m);
        TypeTable::new([row("Explosion", "LightArmor", 2.0), row("Pierce", "LightArmor", 0.5), row("Explosion", "HeavyArmor", 1.0), row("Pierce", "HeavyArmor", 2.0)])
    }

    fn pool() -> Vec<StudentBrief> {
        vec![student(1, "Explosion", "LightArmor", 2), student(2, "Pierce", "HeavyArmor", 2), student(3, "Explosion", "HeavyArmor", 2), student(4, "Pierce", "LightArmor", 2)]
    }

    #[test]
    fn the_right_attack_type_is_favorable_and_the_wrong_one_is_not() {
        let m = mission();
        let good = evaluate(&m, None, &[student(1, "Explosion", "LightArmor", 2)], &pool(), &types()).unwrap();
        let bad = evaluate(&m, None, &[student(2, "Pierce", "HeavyArmor", 2)], &pool(), &types()).unwrap();
        // against light armor: explosion 200 %, the pool averages (200 + 50 + 200 + 50) / 4 = 125 %
        assert!((good.attack - 1.6).abs() < 1e-9, "{good:?}");
        assert_eq!(good.verdict, Verdict::Favorable);
        assert_eq!(bad.verdict, Verdict::Unfavorable);
        assert!(good.members == 1);
    }

    #[test]
    fn an_average_squad_is_average() {
        let m = mission();
        let team = vec![student(1, "Explosion", "LightArmor", 2), student(2, "Pierce", "HeavyArmor", 2)];
        let s = evaluate(&m, None, &team, &pool(), &types()).unwrap();
        assert_eq!(s.verdict, Verdict::Average, "{s:?}");
    }

    #[test]
    fn terrain_adaptation_moves_the_score_when_the_terrain_is_known() {
        let m = mission();
        let best = student(1, "Explosion", "LightArmor", 4);
        let worst = student(1, "Explosion", "LightArmor", 0);
        let a = evaluate(&m, Some("Outdoor"), &[best.clone()], &pool(), &types()).unwrap();
        let b = evaluate(&m, Some("Outdoor"), &[worst], &pool(), &types()).unwrap();
        assert!(a.terrain > 1.0 && b.terrain < 1.0 && a.attack > b.attack);
        // an unknown terrain leaves adaptation out
        let c = evaluate(&m, None, &[best], &pool(), &types()).unwrap();
        assert!((c.terrain - 1.0).abs() < 1e-9);
    }

    #[test]
    fn armor_that_resists_the_enemies_attack_type_raises_defense() {
        let m = mission();
        let types = TypeTable::new([("Explosion".to_string(), "LightArmor".to_string(), 2.0), ("Explosion".to_string(), "HeavyArmor".to_string(), 0.5)]);
        let heavy = evaluate(&m, None, &[student(1, "Pierce", "HeavyArmor", 2)], &pool(), &types).unwrap();
        let light = evaluate(&m, None, &[student(1, "Pierce", "LightArmor", 2)], &pool(), &types).unwrap();
        assert!(heavy.defense > 1.0 && light.defense < 1.0, "{heavy:?} {light:?}");
    }

    #[test]
    fn nothing_to_judge_gives_none() {
        let m = mission();
        assert_eq!(evaluate(&m, None, &[], &pool(), &types()), None);
        let mut empty = mission();
        empty.armor.clear();
        assert_eq!(evaluate(&empty, None, &pool(), &pool(), &types()), None);
    }
}
