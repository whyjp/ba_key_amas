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
    pub members: usize,
    pub verdict: Verdict,
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
pub fn evaluate(brief: &MissionBrief, terrain: Option<&str>, team: &[StudentBrief], pool: &[StudentBrief], types: &TypeTable) -> Option<FormationScore> {
    let (armors, bullets) = (shares(&brief.armor), shares(&brief.attack));
    if team.is_empty() || pool.is_empty() || armors.is_empty() {
        return None;
    }
    let adapt = |s: &StudentBrief| terrain.and_then(|t| s.adaptation(t)).map_or(1.0, adaptation_factor);
    let attack = |s: &StudentBrief| armors.iter().map(|(a, w)| w * types.get(&s.bullet_type, a)).sum::<f64>() * adapt(s);
    // damage taken as a multiplier; its inverse is how well the armor holds
    let defense = |s: &StudentBrief| {
        let taken: f64 = if bullets.is_empty() { 1.0 } else { bullets.iter().map(|(b, w)| w * types.get(b, &s.armor_type)).sum() };
        1.0 / taken.max(0.05)
    };
    let rel = |f: &dyn Fn(&StudentBrief) -> f64| -> Option<f64> { Some(mean(team.iter().map(f))? / mean(pool.iter().map(f))?) };
    let (a, d, t) = (rel(&attack)?, rel(&defense)?, rel(&adapt)?);
    let overall = a.powf(0.6) * d.powf(0.4);
    let verdict = if overall >= FAVORABLE_AT {
        Verdict::Favorable
    } else if overall <= UNFAVORABLE_AT {
        Verdict::Unfavorable
    } else {
        Verdict::Average
    };
    Some(FormationScore { attack: a, defense: d, terrain: t, overall, members: team.len(), verdict })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn student(id: i64, bullet: &str, armor: &str, outdoor: u8) -> StudentBrief {
        StudentBrief { id, name: format!("s{id}"), bullet_type: bullet.into(), armor_type: armor.into(), street: 2, outdoor, indoor: 2, squad_type: "Main".into() }
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
