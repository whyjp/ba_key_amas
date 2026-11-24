//! How much each enemy of an encounter counts toward its armor and attack type mix.
//!
//! The model lives only here, in the collector: the database stores one weight per enemy of an
//! encounter (`formation_enemy_weight`, `raid_difficulty_enemy_weight`) and every reader sums
//! them. Changing the model means changing this file and rebuilding, nothing else.
//!
//! Model (see docs/enemy-weighting.md): an enemy counts as its hit points at the level and grade the
//! encounter gives it, the share of an armor or attack type being its share of the total.
//! No role multiplier is used: nothing researched supports a number.
use serde_json::Value;

/// Ranks of enemies the squad does not fight as a unit: summons, escorted transports and
/// shootable projectiles weigh nothing. The one place this list lives.
pub const UNCOUNTED_RANKS: [&str; 3] = ["Summoned", "Student", "DestructibleProjectile"];

/// Raid levels of the `Raid` kind by difficulty position (SchaleDB's own default, used when the
/// record has no `Level` list). The last value repeats for later positions.
pub const RAID_DEFAULT_LEVELS: [i64; 8] = [17, 25, 35, 50, 70, 80, 90, 90];

/// HP share of the transcendence (grade) steps SchaleDB applies to an enemy of grade n.
const DEFAULT_HP_STEPS: [i64; 5] = [0, 500, 700, 900, 1400];

/// How an enemy's stats grow between level 1 and 100 (SchaleDB `StatLevelUpType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Growth {
    Standard,
    TimeAttack,
}

impl Growth {
    pub fn parse(s: Option<&str>) -> Growth {
        if s == Some("TimeAttack") {
            Growth::TimeAttack
        } else {
            Growth::Standard
        }
    }
}

fn fixed4(x: f64) -> f64 {
    (x * 1e4).round() / 1e4
}

/// Position of a level between the level 1 and level 100 stats, as SchaleDB computes it.
pub fn level_scale(level: i64, growth: Growth) -> f64 {
    let standard = fixed4((level - 1) as f64 / 99.0);
    match growth {
        Growth::Standard => standard,
        Growth::TimeAttack => match level {
            ..=1 => 0.0,
            2 => 0.0101,
            3..=24 => 0.0707,
            25 => 0.0808,
            26..=39 => 0.1919,
            40 => 0.202,
            41..=64 => 0.4444,
            65 => 0.4545,
            66..=77 => 0.7172,
            78 => 0.7273,
            _ => standard,
        },
    }
}

/// An enemy's hit points at `level` and `grade`: SchaleDB's formula, rounded the way it rounds.
/// `steps` are the enemy's own HP transcendence steps when it has them.
pub fn hp_at(hp1: i64, hp100: i64, level: i64, grade: i64, growth: Growth, steps: Option<&[i64]>) -> i64 {
    let raw = hp1 as f64 + (hp100 - hp1) as f64 * level_scale(level, growth);
    let base = fixed4(raw).round();
    let steps = steps.unwrap_or(&DEFAULT_HP_STEPS);
    let extra: i64 = steps.iter().take(grade.max(0) as usize).sum();
    (fixed4(base * (1.0 + extra as f64 / 1e4))).ceil() as i64
}

/// The level and grade an encounter gives its enemies, by enemy rank.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RankedLevels {
    minion: (i64, i64),
    by_rank: Vec<(&'static str, (i64, i64))>,
}

impl RankedLevels {
    pub fn flat(level: i64) -> RankedLevels {
        RankedLevels { minion: (level, 1), by_rank: Vec::new() }
    }

    /// A stage or time-attack formation: `Level<Rank>` and `Grade<Rank>`, else the minion's.
    pub fn from_formation(f: &Value) -> Option<RankedLevels> {
        let minion = (f.get("LevelMinion")?.as_i64()?, f.get("GradeMinion").and_then(Value::as_i64).unwrap_or(1));
        let by_rank = ["Elite", "Champion", "Boss"]
            .into_iter()
            .filter_map(|rank| {
                let level = f.get(format!("Level{rank}"))?.as_i64()?;
                let grade = f.get(format!("Grade{rank}")).and_then(Value::as_i64).unwrap_or(minion.1);
                Some((rank, (level, grade)))
            })
            .collect();
        Some(RankedLevels { minion, by_rank })
    }

    pub fn for_rank(&self, rank: &str) -> (i64, i64) {
        self.by_rank.iter().find(|(r, _)| *r == rank).map_or(self.minion, |(_, lg)| *lg)
    }
}

/// What the weight of one enemy of an encounter is made from.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub rank: String,
    /// HP at the encounter's level and grade; None when the enemy has no HP stats.
    pub hp: Option<i64>,
}

/// Why an entry weighs nothing, for the audit column.
pub const EXCLUDED_RANK: &str = "rank";
pub const EXCLUDED_MIRROR: &str = "mirror";
pub const EXCLUDED_NO_HP: &str = "no-hp";

#[derive(Debug, Clone, PartialEq)]
pub struct Weight {
    pub weight: f64,
    /// None when the entry counts, else why it does not.
    pub excluded: Option<&'static str>,
}

fn counted(hp: i64) -> Weight {
    Weight { weight: hp as f64, excluded: None }
}

fn left_out(why: &'static str) -> Weight {
    Weight { weight: 0.0, excluded: Some(why) }
}

/// Weights of the enemies of a stage formation: HP, but for the uncounted ranks.
pub fn stage_weights(entries: &[Entry]) -> Vec<Weight> {
    entries
        .iter()
        .map(|e| match (UNCOUNTED_RANKS.contains(&e.rank.as_str()), e.hp) {
            (true, _) => left_out(EXCLUDED_RANK),
            (false, None) => left_out(EXCLUDED_NO_HP),
            (false, Some(hp)) => counted(hp),
        })
        .collect()
}

/// Weights of the enemies of a raid difficulty: as a stage, and an entry that is not a Boss but
/// has exactly a Boss's HP is that Boss's own body (a phase or a skin: the multi-part bosses list
/// them at the full HP of the shared bar, e.g. 드럼통 게 and its "(1 페이즈)" body), so it weighs
/// nothing of its own. Parts with less HP than the bar (claws, hulls, pincers) keep theirs.
pub fn raid_weights(entries: &[Entry]) -> Vec<Weight> {
    let boss_hp: Vec<i64> = entries.iter().filter(|e| e.rank == "Boss").filter_map(|e| e.hp).collect();
    stage_weights(entries)
        .into_iter()
        .zip(entries)
        .map(|(w, e)| match e.hp {
            Some(hp) if w.excluded.is_none() && e.rank != "Boss" && boss_hp.contains(&hp) => left_out(EXCLUDED_MIRROR),
            _ => w,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(rank: &str, hp: i64) -> Entry {
        Entry { rank: rank.into(), hp: Some(hp) }
    }

    #[test]
    fn hp_follows_schaledbs_formula() {
        // 드럼통 게 style flat HP does not move with the level
        assert_eq!(hp_at(400_000, 400_000, 17, 1, Growth::Standard, None), 400_000);
        // level 1 and level 100 are the two stats; level 40 of 1..100 sits at 39/99 = 0.3939
        assert_eq!(hp_at(210, 21_050, 1, 1, Growth::Standard, None), 210);
        assert_eq!(hp_at(210, 21_050, 100, 1, Growth::Standard, None), 21_050);
        assert_eq!(hp_at(210, 21_050, 40, 1, Growth::Standard, None), 8_419);
        // grade 3 adds 5 % + 7 %
        assert_eq!(hp_at(1000, 1000, 1, 3, Growth::Standard, None), 1120);
        assert_eq!(hp_at(1000, 1000, 1, 3, Growth::Standard, Some(&[0, 0, 0, 0, 0])), 1000);
        // time attack growth is stepped
        assert_eq!(hp_at(0, 10_000, 25, 1, Growth::TimeAttack, None), 808);
    }

    #[test]
    fn levels_come_from_the_formation_by_rank_else_the_minions() {
        let f: Value = serde_json::json!({"LevelMinion": 75, "GradeMinion": 1, "LevelChampion": 59, "LevelBoss": 59});
        let l = RankedLevels::from_formation(&f).unwrap();
        assert_eq!((l.for_rank("Minion"), l.for_rank("Elite"), l.for_rank("Champion"), l.for_rank("Boss")), ((75, 1), (75, 1), (59, 1), (59, 1)));
        assert_eq!(RankedLevels::flat(90).for_rank("Boss"), (90, 1));
        assert!(RankedLevels::from_formation(&serde_json::json!({})).is_none());
    }

    #[test]
    fn a_stage_counts_hp_and_leaves_out_summons_transports_and_projectiles() {
        let w = stage_weights(&[entry("Champion", 5000), entry("Minion", 100), entry("Summoned", 9000), entry("Student", 10), Entry { rank: "Elite".into(), hp: None }]);
        assert_eq!(w.iter().map(|w| w.weight).collect::<Vec<_>>(), vec![5000.0, 100.0, 0.0, 0.0, 0.0]);
        assert_eq!(w.iter().map(|w| w.excluded).collect::<Vec<_>>(), vec![None, None, Some("rank"), Some("rank"), Some("no-hp")]);
    }

    #[test]
    fn a_boss_bodies_at_the_bars_full_hp_are_not_counted_again_but_its_smaller_parts_are() {
        // 드럼통 게 Normal: Boss 400k, bodies 400k, claw 3k, hull 25k (light), pincer 200k (light)
        let w = raid_weights(&[entry("Boss", 400_000), entry("Minion", 400_000), entry("Minion", 3_000), entry("Minion", 25_000), entry("Minion", 200_000), entry("Elite", 400_000)]);
        assert_eq!(w.iter().map(|w| w.weight).collect::<Vec<_>>(), vec![400_000.0, 0.0, 3_000.0, 25_000.0, 200_000.0, 0.0]);
        assert_eq!(w[1].excluded, Some("mirror"));
        // an elite with more HP than the Boss is a real enemy (the Kaitenger of KAITEN FX Mk.0)
        let k = raid_weights(&[entry("Boss", 120), entry("Elite", 230)]);
        assert_eq!(k[1].weight, 230.0);
    }
}
