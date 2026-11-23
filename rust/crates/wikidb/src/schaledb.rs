//! SchaleDB collector. Reads the site's Korean data files (the Global server in Korean;
//! the GitHub repository is archived, the site data keeps being updated).
//!
//! `fetch` only downloads, `load` only writes the database, so `load` is tested
//! without a network.
use anyhow::{anyhow, bail, Context, Result};
use rusqlite::{params, Connection, Transaction};
use serde_json::Value;

use crate::weight::{self, Entry, Growth, RankedLevels, Weight};
use std::collections::HashMap;

const SITE: &str = "https://schaledb.com/data/kr";
/// The type-effectiveness table lives in the site's common config.
const CONFIG_URLS: [&str; 2] = [
    "https://schaledb.com/data/config.min.json",
    "https://raw.githubusercontent.com/SchaleDB/SchaleDB/main/data/config.min.json",
];
/// Index of the Global server in SchaleDB's per-server arrays ([JP, Global, CN]).
const GLOBAL: usize = 1;
const SOURCE_NAME: &str = "schaledb";
const SOURCE_NOTE: &str = "SchaleDB community data. Local assist use only; the database file is not redistributed.";

pub struct Dataset {
    pub students: Value,
    pub furniture: Value,
    pub enemies: Value,
    pub stages: Value,
    pub raids: Value,
    pub localization: Value,
    pub config: Value,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Counts {
    pub labels: usize,
    pub type_effectiveness: usize,
    pub students: usize,
    pub furniture: usize,
    /// Global interactions stored.
    pub furniture_interactions: usize,
    /// Interactions left out because the student is an alternate style (SchaleDB's own furniture
    /// page leaves them out too: the base student is already listed).
    pub interactions_skipped_style: usize,
    /// Repeated (student, furniture) pairs in the source (same item listed with 0 and with 1):
    /// the first listing is kept, as SchaleDB's page does.
    pub interactions_duplicate: usize,
    pub enemies: usize,
    pub stages: usize,
    pub formations: usize,
    pub formation_enemies: usize,
    pub raids: usize,
    /// Difficulties of all raids and the enemy rows that belong to them.
    pub raid_difficulties: usize,
    pub raid_difficulty_enemies: usize,
    /// Weight rows written: one per enemy of a formation and of a raid difficulty.
    pub formation_weights: usize,
    pub raid_weights: usize,
    pub raid_seasons: usize,
    pub eliminate_seasons: usize,
}

fn get(url: &str) -> Result<Value> {
    ureq::get(url).call().map_err(|e| anyhow!("{url}: {e}"))?.into_json().with_context(|| format!("{url}: not JSON"))
}

pub fn fetch() -> Result<Dataset> {
    let file = |name: &str| get(&format!("{SITE}/{name}.min.json"));
    let config = CONFIG_URLS
        .iter()
        .find_map(|u| get(u).ok().filter(|v| v.get("TypeEffectiveness").is_some()))
        .ok_or_else(|| anyhow!("no config.min.json with TypeEffectiveness"))?;
    Ok(Dataset {
        students: file("students")?,
        furniture: file("furniture")?,
        enemies: file("enemies")?,
        stages: file("stages")?,
        raids: file("raids")?,
        localization: file("localization")?,
        config,
    })
}

fn text(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(|x| x.as_str()).map(str::to_string)
}

fn int(v: &Value, k: &str) -> Option<i64> {
    v.get(k).and_then(|x| x.as_i64())
}

/// A string field that may be a one-element list in some record kinds.
fn text_or_first(v: &Value, k: &str) -> Option<String> {
    match v.get(k)? {
        Value::String(s) => Some(s.clone()),
        Value::Array(a) => a.first().and_then(|x| x.as_str()).map(str::to_string),
        _ => None,
    }
}

fn released_global(v: &Value) -> bool {
    v.get("IsReleased").and_then(|a| a.get(GLOBAL)).and_then(|x| x.as_bool()).unwrap_or(false)
}

fn json_text(v: &Value, k: &str) -> Option<String> {
    v.get(k).filter(|x| !x.is_null()).map(|x| x.to_string())
}

fn objects<'a>(v: &'a Value, what: &str) -> Result<impl Iterator<Item = &'a Value>> {
    match v {
        Value::Object(m) => Ok(m.values()),
        _ => bail!("{what}: expected an object keyed by id"),
    }
}

pub fn load(conn: &mut Connection, d: &Dataset, fetched_at: &str) -> Result<Counts> {
    let tx = conn.transaction()?;
    let mut n = Counts::default();
    tx.execute("INSERT INTO source (id, name, url, fetched_at, version, note) VALUES (1, ?1, ?2, ?3, NULL, ?4)", params![SOURCE_NAME, SITE, fetched_at, SOURCE_NOTE])?;
    labels(&tx, d, &mut n)?;
    type_effectiveness(&tx, d, &mut n)?;
    students(&tx, d, &mut n)?;
    furniture(&tx, d, &mut n)?;
    furniture_interactions(&tx, d, &mut n)?;
    enemies(&tx, d, &mut n)?;
    stages(&tx, d, &mut n)?;
    raids(&tx, d, &mut n)?;
    tx.execute("INSERT INTO meta (key, value) VALUES ('built_at', ?1)", params![fetched_at])?;
    tx.commit()?;
    Ok(n)
}

/// Localization sections that name a closed code set, stored under the kind the schema uses.
const LABEL_KINDS: [(&str, &str); 8] = [
    ("BulletType", "BulletType"),
    ("ArmorType", "ArmorType"),
    ("StageType", "StageType"),
    ("EnemyRank", "EnemyRank"),
    ("SquadType", "SquadType"),
    ("TacticRole", "TacticRole"),
    ("AdaptationType", "Terrain"),
    ("TimeAttackStage", "TimeAttackStage"),
];

fn labels(tx: &Transaction, d: &Dataset, n: &mut Counts) -> Result<()> {
    let mut st = tx.prepare("INSERT INTO label (kind, code, ko) VALUES (?1, ?2, ?3)")?;
    for (section, kind) in LABEL_KINDS {
        let Some(map) = d.localization.get(section).and_then(|x| x.as_object()) else { continue };
        for (code, ko) in map {
            if let Some(ko) = ko.as_str() {
                st.execute(params![kind, code, ko])?;
                n.labels += 1;
            }
        }
    }
    Ok(())
}

fn type_effectiveness(tx: &Transaction, d: &Dataset, n: &mut Counts) -> Result<()> {
    let table = d.config.get("TypeEffectiveness").and_then(|x| x.as_object()).ok_or_else(|| anyhow!("config has no TypeEffectiveness"))?;
    let mut st = tx.prepare("INSERT INTO type_effectiveness (bullet_type, armor_type, multiplier_bp) VALUES (?1, ?2, ?3)")?;
    for (bullet, row) in table {
        for (armor, bp) in row.as_object().into_iter().flatten() {
            st.execute(params![bullet, armor, bp.as_i64().ok_or_else(|| anyhow!("{bullet}/{armor}: not a number"))?])?;
            n.type_effectiveness += 1;
        }
    }
    Ok(())
}

fn students(tx: &Transaction, d: &Dataset, n: &mut Counts) -> Result<()> {
    let mut st = tx.prepare(
        "INSERT INTO student (id, path_name, dev_name, name_ko, school, club, star_grade, squad_type, tactic_role, position,
            bullet_type, armor_type, street_adapt, outdoor_adapt, indoor_adapt, weapon_type, released_global, source_id, raw)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,1,?18)",
    )?;
    for r in objects(&d.students, "students")? {
        let id = int(r, "Id").ok_or_else(|| anyhow!("student without Id"))?;
        st.execute(params![
            id,
            text(r, "PathName").unwrap_or_default(),
            text(r, "DevName").unwrap_or_default(),
            text(r, "Name").with_context(|| format!("student {id} has no Name"))?,
            text(r, "School"),
            text(r, "Club"),
            int(r, "StarGrade").unwrap_or(0),
            text(r, "SquadType").unwrap_or_default(),
            text(r, "TacticRole"),
            text(r, "Position"),
            text(r, "BulletType").with_context(|| format!("student {id} has no BulletType"))?,
            text(r, "ArmorType").with_context(|| format!("student {id} has no ArmorType"))?,
            int(r, "StreetBattleAdaptation"),
            int(r, "OutdoorBattleAdaptation"),
            int(r, "IndoorBattleAdaptation"),
            text(r, "WeaponType"),
            released_global(r),
            r.to_string(),
        ])?;
        n.students += 1;
    }
    Ok(())
}

/// Cafe furniture, every item (also the ones nobody interacts with, and the unreleased).
fn furniture(tx: &Transaction, d: &Dataset, n: &mut Counts) -> Result<()> {
    let mut st = tx.prepare(
        "INSERT INTO furniture (id, name_ko, category, sub_category, rarity, size_w, size_h, comfort, set_group_id, released_global, interaction_global, source_id, raw)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,1,?12)",
    )?;
    let flag = |r: &Value, k: &str| r.get(k).and_then(|a| a.get(GLOBAL)).and_then(|x| x.as_bool()).unwrap_or(false);
    for r in objects(&d.furniture, "furniture")? {
        let id = int(r, "Id").ok_or_else(|| anyhow!("furniture without Id"))?;
        let size = |i: usize| r.get("Size").and_then(|a| a.get(i)).and_then(|x| x.as_i64());
        st.execute(params![
            id,
            text(r, "Name").with_context(|| format!("furniture {id} has no Name"))?,
            text(r, "Category").unwrap_or_default(),
            text(r, "SubCategory"),
            text(r, "Rarity"),
            size(0),
            size(1),
            int(r, "ComfortBonus"),
            int(r, "SetGroupId"),
            flag(r, "IsReleased"),
            flag(r, "Interaction"),
            r.to_string(),
        ])?;
        n.furniture += 1;
    }
    Ok(())
}

/// Student to furniture links of the Global server (index 1 of each student's per-server lists).
/// A link to a furniture id or a student that does not exist is an error, not a silent gap.
fn furniture_interactions(tx: &Transaction, d: &Dataset, n: &mut Counts) -> Result<()> {
    let mut st = tx.prepare("INSERT INTO furniture_interaction (furniture_id, student_id, extra) VALUES (?1,?2,?3)")?;
    for r in objects(&d.students, "students")? {
        let id = int(r, "Id").ok_or_else(|| anyhow!("student without Id"))?;
        let Some(list) = r.get("FurnitureInteraction").and_then(|a| a.get(GLOBAL)).and_then(|x| x.as_array()) else { continue };
        let mut seen = std::collections::HashSet::new();
        for pair in list {
            let fid = pair.get(0).and_then(|x| x.as_i64()).ok_or_else(|| anyhow!("student {id}: furniture interaction without a furniture id"))?;
            let extra = pair.get(1).and_then(|x| x.as_i64()).unwrap_or(0);
            if int(r, "StyleId").unwrap_or(0) > 0 {
                n.interactions_skipped_style += 1;
                continue;
            }
            if !seen.insert(fid) {
                n.interactions_duplicate += 1;
                continue;
            }
            st.execute(params![fid, id, extra]).with_context(|| format!("student {id} interacts with furniture {fid}"))?;
            n.furniture_interactions += 1;
        }
    }
    Ok(())
}

/// What an enemy's hit points are made from, read once from the source record.
struct HpStats {
    rank: String,
    hp1: i64,
    hp100: i64,
    growth: Growth,
    steps: Option<Vec<i64>>,
}

fn hp_stats(d: &Dataset) -> HashMap<i64, HpStats> {
    d.enemies
        .get("Enemies")
        .and_then(|x| x.as_object())
        .into_iter()
        .flat_map(|m| m.values())
        .filter_map(|e| {
            let steps = e.get("Transcendence").and_then(|t| t.get(1)).and_then(|a| a.as_array()).map(|a| a.iter().filter_map(Value::as_i64).collect());
            Some((int(e, "Id")?, HpStats { rank: text(e, "Rank")?, hp1: int(e, "MaxHP1")?, hp100: int(e, "MaxHP100")?, growth: Growth::parse(e.get("StatLevelUpType").and_then(|x| x.as_str())), steps }))
        })
        .collect()
}

/// Weights of one encounter's enemies, in order. `levels`: what the encounter gives each rank.
/// The enemy table is read here and nowhere else, so an enemy without HP stats weighs nothing (`no-hp`).
fn weigh(stats: &HashMap<i64, HpStats>, ranks: &HashMap<i64, String>, enemies: &[i64], levels: Option<&RankedLevels>, raid: bool) -> Vec<(Option<(i64, i64)>, Option<i64>, Weight)> {
    let at: Vec<(Option<(i64, i64)>, Option<i64>)> = enemies
        .iter()
        .map(|id| {
            let s = stats.get(id);
            // level unknown: the stats at 100, which is the stat for enemies of a flat HP too
            let lg = s.zip(levels).map(|(s, l)| l.for_rank(&s.rank));
            let hp = s.map(|s| match lg {
                Some((level, grade)) => weight::hp_at(s.hp1, s.hp100, level, grade, s.growth, s.steps.as_deref()),
                None => s.hp100,
            });
            (lg, hp)
        })
        .collect();
    let entries: Vec<Entry> = enemies.iter().zip(&at).map(|(id, (_, hp))| Entry { rank: ranks.get(id).cloned().unwrap_or_default(), hp: *hp }).collect();
    let weights = if raid { weight::raid_weights(&entries) } else { weight::stage_weights(&entries) };
    at.into_iter().zip(weights).map(|((lg, hp), w)| (lg, hp, w)).collect()
}

fn enemies(tx: &Transaction, d: &Dataset, n: &mut Counts) -> Result<()> {
    let all = d.enemies.get("Enemies").ok_or_else(|| anyhow!("enemies.json has no Enemies"))?;
    let mut st = tx.prepare(
        "INSERT INTO enemy (id, dev_name, name_ko, rank, squad_type, bullet_type, armor_type, size, source_id, raw)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,1,?9)",
    )?;
    for r in objects(all, "enemies")? {
        let id = int(r, "Id").ok_or_else(|| anyhow!("enemy without Id"))?;
        st.execute(params![
            id,
            text(r, "DevName"),
            text(r, "Name").unwrap_or_default(),
            text(r, "Rank").unwrap_or_default(),
            text(r, "SquadType"),
            text(r, "BulletType"),
            text(r, "ArmorType"),
            text(r, "Size"),
            r.to_string(),
        ])?;
        n.enemies += 1;
    }
    Ok(())
}

fn stages(tx: &Transaction, d: &Dataset, n: &mut Counts) -> Result<()> {
    let stage_type_label = |t: &str| d.localization.get("StageType").and_then(|m| m.get(t)).and_then(|x| x.as_str()).map(str::to_string);
    let mut stage = tx.prepare(
        "INSERT INTO stage (id, category, type, area, stage_no, difficulty, name_ko, terrain, level, entry_cost, star_condition, challenge_condition, source_id)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,1)",
    )?;
    let mut formation = tx.prepare("INSERT INTO stage_formation (id, stage_id, ordinal, unit_grade, level_minion, grade_minion) VALUES (?1,?2,?3,?4,?5,?6)")?;
    let mut member = tx.prepare("INSERT INTO formation_enemy (formation_id, ordinal, enemy_id) VALUES (?1,?2,?3)")?;
    let mut weight_row = tx.prepare("INSERT INTO formation_enemy_weight (formation_id, ordinal, level, grade, hp, weight, excluded) VALUES (?1,?2,?3,?4,?5,?6,?7)")?;
    let stats = hp_stats(d);
    let ranks: HashMap<i64, String> = stats.iter().map(|(id, s)| (*id, s.rank.clone())).collect();
    for r in objects(&d.stages, "stages")? {
        let id = int(r, "Id").ok_or_else(|| anyhow!("stage without Id"))?;
        let kind = text(r, "Type");
        // bounties and dungeons carry no name of their own: use their type's name
        let name = text(r, "Name").or_else(|| kind.as_deref().and_then(stage_type_label));
        stage.execute(params![
            id,
            text(r, "Category").unwrap_or_default(),
            kind,
            int(r, "Area"),
            int(r, "Stage"),
            int(r, "Difficulty"),
            name,
            text(r, "Terrain"),
            int(r, "Level"),
            json_text(r, "EntryCost"),
            json_text(r, "StarCondition"),
            json_text(r, "ChallengeCondition"),
        ])?;
        n.stages += 1;
        for (i, f) in r.get("Formations").and_then(|x| x.as_array()).into_iter().flatten().enumerate() {
            let fid = int(f, "Id").ok_or_else(|| anyhow!("stage {id}: formation without Id"))?;
            formation
                .execute(params![fid, id, i as i64, text(f, "UnitGrade"), int(f, "LevelMinion"), int(f, "GradeMinion")])
                .with_context(|| format!("stage {id} formation {fid}"))?;
            n.formations += 1;
            let list: Vec<i64> = f.get("EnemyList").and_then(|x| x.as_array()).into_iter().flatten().map(|e| e.as_i64().ok_or_else(|| anyhow!("stage {id}: enemy id is not a number"))).collect::<Result<_>>()?;
            for (j, enemy) in list.iter().enumerate() {
                member.execute(params![fid, j as i64, enemy]).with_context(|| format!("stage {id} formation {fid}: enemy {enemy}"))?;
                n.formation_enemies += 1;
            }
            let levels = RankedLevels::from_formation(f);
            for (j, (lg, hp, w)) in weigh(&stats, &ranks, &list, levels.as_ref(), false).into_iter().enumerate() {
                weight_row.execute(params![fid, j as i64, lg.map(|x| x.0), lg.map(|x| x.1), hp, w.weight, w.excluded])?;
                n.formation_weights += 1;
            }
        }
    }
    Ok(())
}

/// Raid-like encounters, keyed in raids.json by kind.
const RAID_KINDS: [&str; 5] = ["Raid", "TimeAttack", "WorldRaid", "MultiFloorRaid", "InteractiveWorldRaid"];

/// Names of a `Raid` kind's difficulties by array position: the order of SchaleDB's
/// `RaidDifficulty` localization section (a JSON object, so its own key order is not kept).
/// The attack type switches at position 5, which the source's `BulletTypeInsane` calls Insane.
const RAID_DIFFICULTY_NAMES: [&str; 8] = ["Normal", "Hard", "VeryHard", "HardCore", "Extreme", "Insane", "Torment", "Lunatic"];

/// One difficulty of a raid, parsed once from the kind-specific arrays of its record.
#[derive(Debug, PartialEq)]
struct RaidDifficultyRow {
    idx: usize,
    label: Option<String>,
    level: Option<i64>,
    floor_start: Option<i64>,
    battle_duration: Option<i64>,
    enemies: Vec<i64>,
    /// The level and grade the encounter gives its enemies by rank; None when the source gives no level.
    levels: Option<RankedLevels>,
    released_global: bool,
}

fn ints(v: &Value, what: &str) -> Result<Vec<i64>> {
    v.as_array().ok_or_else(|| anyhow!("{what}: expected a list"))?.iter().map(|x| x.as_i64().ok_or_else(|| anyhow!("{what}: not a number"))).collect()
}

/// The list at `key` as one list of numbers per difficulty.
fn int_lists(r: &Value, key: &str) -> Result<Vec<Vec<i64>>> {
    let all = r.get(key).and_then(|x| x.as_array()).ok_or_else(|| anyhow!("no {key} list"))?;
    all.iter().map(|l| ints(l, key)).collect()
}

fn int_list(r: &Value, key: &str) -> Result<Vec<i64>> {
    match r.get(key) {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(v) => ints(v, key),
    }
}

fn str_list(r: &Value, key: &str) -> Vec<String> {
    r.get(key).and_then(|x| x.as_array()).into_iter().flatten().filter_map(|x| x.as_str().map(str::to_string)).collect()
}

/// The difficulties of one raid record. The per-difficulty arrays differ by kind (enemies per
/// difficulty, formations per level, enemy tiers per floor range); this is the only place
/// that knows.
fn raid_difficulties(kind: &str, r: &Value) -> Result<Vec<RaidDifficultyRow>> {
    let released = r.get("IsReleased").map_or(true, |_| released_global(r));
    // highest difficulty index open on Global
    let max_open = r.get("MaxDifficulty").and_then(|a| a.get(GLOBAL)).and_then(|x| x.as_i64());
    let levels = int_list(r, "Level")?;
    let formations = r.get("Formations").and_then(|x| x.as_array());
    let (enemies, durations, names, floors): (Vec<Vec<i64>>, Vec<i64>, Vec<String>, Vec<i64>) = match kind {
        "TimeAttack" => {
            let formations = formations.ok_or_else(|| anyhow!("no Formations list"))?;
            let lists = formations.iter().map(|f| ints(f.get("EnemyList").unwrap_or(&Value::Null), "Formations.EnemyList")).collect::<Result<_>>()?;
            (lists, int_list(r, "BattleDuration")?, Vec::new(), Vec::new())
        }
        "MultiFloorRaid" => {
            let lists = int_lists(r, "EnemyList")?;
            let duration = r.get("BattleDuration").and_then(|x| x.as_i64()).ok_or_else(|| anyhow!("no BattleDuration"))?;
            let n = lists.len();
            (lists, vec![duration; n], Vec::new(), int_list(r, "DifficultyStartFloor")?)
        }
        "Raid" => {
            let lists = int_lists(r, "EnemyList")?;
            let names = RAID_DIFFICULTY_NAMES.iter().take(lists.len()).map(|s| s.to_string()).collect();
            (lists, int_list(r, "BattleDuration")?, names, Vec::new())
        }
        _ => (int_lists(r, "EnemyList")?, int_list(r, "BattleDuration")?, str_list(r, "DifficultyName"), Vec::new()),
    };
    Ok(enemies
        .into_iter()
        .enumerate()
        .map(|(idx, enemies)| RaidDifficultyRow {
            idx,
            label: names.get(idx).cloned(),
            level: levels.get(idx).copied(),
            floor_start: floors.get(idx).copied(),
            battle_duration: durations.get(idx).copied(),
            levels: encounter_levels(kind, r, idx, levels.get(idx).copied(), floors.get(idx).copied(), formations),
            enemies,
            released_global: released && max_open.map_or(true, |m| idx as i64 <= m),
        })
        .collect())
}

/// The level (and grade) a raid difficulty gives its enemies, the way SchaleDB's raid page picks it: a
/// time attack's formation by rank, a floor raid's first floor of the tier, else the record's level
/// for the difficulty, else the default of the `Raid` kind.
fn encounter_levels(kind: &str, r: &Value, idx: usize, level: Option<i64>, floor_start: Option<i64>, formations: Option<&Vec<Value>>) -> Option<RankedLevels> {
    match kind {
        "TimeAttack" => formations.and_then(|f| f.get(idx)).and_then(RankedLevels::from_formation).or_else(|| level.map(RankedLevels::flat)),
        "MultiFloorRaid" => {
            let floor = r.get("RaidFloors").and_then(|f| f.get((floor_start? - 1).max(0) as usize))?;
            int(floor, "Level").map(RankedLevels::flat)
        }
        "Raid" => Some(RankedLevels::flat(level.unwrap_or_else(|| weight::RAID_DEFAULT_LEVELS[idx.min(weight::RAID_DEFAULT_LEVELS.len() - 1)]))),
        _ => level.map(RankedLevels::flat),
    }
}

/// The enemy a difficulty is named after: its first Boss, else its first Champion (the
/// time-attack shooting tests have a Champion and no Boss).
fn lead_enemy(enemies: &[i64], rank: impl Fn(i64) -> Option<String>) -> Option<i64> {
    let ranked = |wanted: &str| enemies.iter().copied().find(|&e| rank(e).as_deref() == Some(wanted));
    ranked("Boss").or_else(|| ranked("Champion"))
}

/// What a raid shows as its name. The source names every kind but TimeAttack; a time attack
/// is told apart by its test, terrain and enemy armor ("사격 시험 · 야외 · 경장갑").
fn raid_display_name(d: &Dataset, kind: &str, id: i64, r: &Value) -> String {
    if let Some(name) = text(r, "Name").filter(|n| !n.is_empty()) {
        return name;
    }
    let label = |section: &str, code: &str| d.localization.get(section).and_then(|m| m.get(code)).and_then(|x| x.as_str()).map_or_else(|| code.to_string(), str::to_string);
    if kind == "TimeAttack" {
        let parts: Vec<String> = [("TimeAttackStage", text(r, "DungeonType")), ("AdaptationType", text_or_first(r, "Terrain")), ("ArmorType", text_or_first(r, "ArmorType"))]
            .into_iter()
            .filter_map(|(section, code)| code.map(|c| label(section, &c)))
            .collect();
        if !parts.is_empty() {
            return parts.join(" · ");
        }
    }
    text(r, "PathName").unwrap_or_else(|| format!("{kind} {id}"))
}

fn raids(tx: &Transaction, d: &Dataset, n: &mut Counts) -> Result<()> {
    let enemy_rank: std::collections::HashMap<i64, String> = d
        .enemies
        .get("Enemies")
        .and_then(|x| x.as_object())
        .into_iter()
        .flat_map(|m| m.values())
        .filter_map(|e| Some((int(e, "Id")?, text(e, "Rank")?)))
        .collect();
    let mut st = tx.prepare(
        "INSERT INTO raid (id, kind, path_name, name_ko, display_ko, dungeon_type, terrain, released_global, source_id, raw)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,1,?9)",
    )?;
    let mut difficulty = tx.prepare(
        "INSERT INTO raid_difficulty (raid_kind, raid_id, idx, label, level, floor_start, battle_duration, boss_enemy_id, released_global)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
    )?;
    let mut member = tx.prepare("INSERT INTO raid_difficulty_enemy (raid_kind, raid_id, idx, ordinal, enemy_id) VALUES (?1,?2,?3,?4,?5)")?;
    let mut weight_row = tx.prepare(
        "INSERT INTO raid_difficulty_enemy_weight (raid_kind, raid_id, idx, ordinal, level, grade, hp, weight, excluded) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
    )?;
    let stats = hp_stats(d);
    for kind in RAID_KINDS {
        for r in d.raids.get(kind).and_then(|x| x.as_array()).into_iter().flatten() {
            let id = int(r, "Id").ok_or_else(|| anyhow!("{kind} without Id"))?;
            // terrain is a list for some kinds and a single value for others: always a JSON list
            let terrain = match r.get("Terrain") {
                Some(Value::Array(a)) => Some(Value::Array(a.clone()).to_string()),
                Some(Value::String(s)) => Some(Value::Array(vec![Value::String(s.clone())]).to_string()),
                _ => None,
            };
            st.execute(params![
                id,
                kind,
                text(r, "PathName"),
                text(r, "Name"),
                raid_display_name(d, kind, id, r),
                text(r, "DungeonType"),
                terrain,
                r.get("IsReleased").is_some().then(|| released_global(r)),
                r.to_string(),
            ])
            .with_context(|| format!("{kind} {id}"))?;
            n.raids += 1;
            for row in raid_difficulties(kind, r).with_context(|| format!("{kind} {id}"))? {
                let lead = lead_enemy(&row.enemies, |e| enemy_rank.get(&e).cloned());
                difficulty
                    .execute(params![kind, id, row.idx as i64, row.label, row.level, row.floor_start, row.battle_duration, lead, row.released_global])
                    .with_context(|| format!("{kind} {id} difficulty {}", row.idx))?;
                n.raid_difficulties += 1;
                for (ordinal, enemy) in row.enemies.iter().enumerate() {
                    member.execute(params![kind, id, row.idx as i64, ordinal as i64, enemy]).with_context(|| format!("{kind} {id} difficulty {}: enemy {enemy}", row.idx))?;
                    n.raid_difficulty_enemies += 1;
                }
                for (ordinal, (lg, hp, w)) in weigh(&stats, &enemy_rank, &row.enemies, row.levels.as_ref(), true).into_iter().enumerate() {
                    weight_row.execute(params![kind, id, row.idx as i64, ordinal as i64, lg.map(|x| x.0), lg.map(|x| x.1), hp, w.weight, w.excluded])?;
                    n.raid_weights += 1;
                }
            }
        }
    }
    let server = d.raids.get("RaidSeasons").and_then(|x| x.get(GLOBAL));
    let mut season = tx.prepare(
        "INSERT INTO raid_season (mode, season_id, season_display, raid_id, terrain, start_at, end_at, open_difficulty) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
    )?;
    for (mode, key) in [("Raid", "Seasons"), ("Eliminate", "EliminateSeasons")] {
        for s in server.and_then(|x| x.get(key)).and_then(|x| x.as_array()).into_iter().flatten() {
            let field = |k: &str| int(s, k).ok_or_else(|| anyhow!("{mode} season without {k}"));
            let season_id = field("SeasonId")?;
            let display = match s.get("SeasonDisplay") {
                Some(Value::String(t)) => Some(t.clone()),
                Some(Value::Number(num)) => Some(num.to_string()),
                _ => None,
            };
            season
                .execute(params![
                    mode,
                    season_id,
                    display,
                    field("RaidId")?,
                    text(s, "Terrain").ok_or_else(|| anyhow!("{mode} season {season_id} without Terrain"))?,
                    field("Start")?,
                    field("End")?,
                    json_text(s, "OpenDifficulty"),
                ])
                .with_context(|| format!("{mode} season {season_id}"))?;
            if mode == "Raid" {
                n.raid_seasons += 1;
            } else {
                n.eliminate_seasons += 1;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn dataset() -> Dataset {
        Dataset {
            students: json!({
                "1": {"Id": 1, "IsReleased": [true, true, true], "PathName": "a", "DevName": "a", "Name": "폭발학생", "SquadType": "Main",
                      "BulletType": "Explosion", "ArmorType": "LightArmor", "StarGrade": 3, "StreetBattleAdaptation": 3, "OutdoorBattleAdaptation": 2, "IndoorBattleAdaptation": 1,
                      "FurnitureInteraction": [[[100, 0]], [[100, 3], [101, 0], [101, 1]], [[100, 0]]]},
                "2": {"Id": 2, "IsReleased": [true, true, true], "PathName": "b", "DevName": "b", "Name": "관통학생", "SquadType": "Main",
                      "BulletType": "Pierce", "ArmorType": "LightArmor", "StarGrade": 3, "StreetBattleAdaptation": 1, "OutdoorBattleAdaptation": 1, "IndoorBattleAdaptation": 1,
                      "FurnitureInteraction": [[], [[100, 1]], []]},
                "3": {"Id": 3, "IsReleased": [true, false, true], "PathName": "c", "DevName": "c", "Name": "미출시", "SquadType": "Main",
                      "BulletType": "Pierce", "ArmorType": "LightArmor", "StarGrade": 3,
                      "FurnitureInteraction": [[[103, 0]], [], []]},
                "4": {"Id": 4, "StyleId": 1, "IsReleased": [true, false, true], "PathName": "d", "DevName": "d", "Name": "폭발학생(스타일)", "SquadType": "Main",
                      "BulletType": "Explosion", "ArmorType": "LightArmor", "StarGrade": 3, "FurnitureInteraction": [[], [[100, 0]], []]}
            }),
            furniture: json!({
                "100": {"Id": 100, "Name": "스타 테이블", "Category": "Furnitures", "SubCategory": "Table", "Rarity": "SR", "Size": [4, 3], "ComfortBonus": 120, "SetGroupId": 7,
                        "IsReleased": [true, true, true], "Interaction": [true, true, true]},
                "101": {"Id": 101, "Name": "소파", "Category": "Furnitures", "SubCategory": "Sofa", "Size": [2, 2], "IsReleased": [true, true, false], "Interaction": [true, true, false]},
                "102": {"Id": 102, "Name": "화분", "Category": "Decorations", "Size": [1, 1], "IsReleased": [true, true, true], "Interaction": [false, false, false]},
                "103": {"Id": 103, "Name": "일본 한정 의자", "Category": "Furnitures", "Size": [1, 1], "IsReleased": [true, false, false], "Interaction": [true, false, false]}
            }),
            enemies: json!({"Enemies": {
                "7000001": {"Id": 7000001, "Name": "불량배", "Rank": "Minion", "BulletType": "Normal", "ArmorType": "LightArmor", "MaxHP1": 100, "MaxHP100": 10100},
                "7000013": {"Id": 7000013, "Name": "무자비한 불량배", "Rank": "Champion", "BulletType": "Normal", "ArmorType": "LightArmor", "MaxHP1": 1000, "MaxHP100": 100000},
                "7300100": {"Id": 7300100, "Name": "비나", "Rank": "Boss", "BulletType": "Normal", "ArmorType": "HeavyArmor", "MaxHP1": 300000, "MaxHP100": 300000},
                "7300600": {"Id": 7300600, "Name": "비나", "Rank": "Boss", "BulletType": "Pierce", "ArmorType": "HeavyArmor", "MaxHP1": 900000, "MaxHP100": 900000},
                "7300900": {"Id": 7300900, "Name": "소환물", "Rank": "Summoned", "BulletType": "Normal", "ArmorType": "LightArmor", "MaxHP1": 5000, "MaxHP100": 5000},
                "7300901": {"Id": 7300901, "Name": "비나의 몸", "Rank": "Minion", "BulletType": "Normal", "ArmorType": "HeavyArmor", "MaxHP1": 300000, "MaxHP100": 300000},
                "7300902": {"Id": 7300902, "Name": "비나의 부위", "Rank": "Minion", "BulletType": "Normal", "ArmorType": "LightArmor", "MaxHP1": 100000, "MaxHP100": 100000}
            }}),
            stages: json!({
                "1011101": {"Id": 1011101, "Category": "Campaign", "Difficulty": 0, "Area": 1, "Stage": 1, "Name": "아오바 중심구 35번지", "Terrain": "Street", "Level": 1,
                            "EntryCost": [[5, 10]], "Formations": [
                                {"Id": 101110101, "UnitGrade": "Grade1", "LevelMinion": 1, "GradeMinion": 1, "EnemyList": [7000001, 7000001]},
                                {"Id": 101110102, "UnitGrade": "Boss", "LevelMinion": 1, "GradeMinion": 1, "EnemyList": [7000013]}]},
                "30101": {"Id": 30101, "Category": "Bounty", "Type": "ChaserA", "Stage": 1, "Terrain": "Outdoor", "Formations": []}
            }),
            raids: json!({
                "Raid": [{"Id": 1, "IsReleased": [true, true, false], "MaxDifficulty": [7, 4, 7], "PathName": "binah", "Name": "비나", "Terrain": ["Outdoor", "Street"],
                          "BulletType": "Normal", "BulletTypeInsane": "Pierce", "ArmorType": "HeavyArmor",
                          "EnemyList": [[7300100, 7300901, 7300902], [7300100], [7300100], [7300100], [7300900, 7300100], [7300600]], "BattleDuration": [180, 180, 180, 180, 180, 240]}],
                "TimeAttack": [{"Id": 1, "IsReleased": [true, true, true], "MaxDifficulty": [1, 1, 1], "DungeonType": "Shooting", "Terrain": "Outdoor", "BulletType": "Normal", "ArmorType": "LightArmor",
                                "Level": [25, 40], "BattleDuration": [180, 240],
                                "Formations": [{"Id": 911000001, "EnemyList": [7000013]}, {"Id": 911000002, "EnemyList": [7000001, 7000013]}]}],
                "WorldRaid": [{"Id": 814000, "IsReleased": [true, true, true], "MaxDifficulty": [1, 1, 1], "PathName": "w", "Name": "와카모", "Terrain": ["Indoor"], "DifficultyName": ["HardCore", "Extreme"],
                               "Level": [60, 70], "EnemyList": [[7300100], [7300600]], "BattleDuration": [240, 240]}],
                "MultiFloorRaid": [{"Id": 1000000, "IsReleased": [true, true, true], "MaxDifficulty": [1, 1, 1], "DifficultyStartFloor": [1, 25, 50], "PathName": "set", "Name": "세트", "Terrain": ["Outdoor"],
                                    "EnemyList": [[7300100, 7300900], [7300600]], "BattleDuration": 270}],
                "RaidSeasons": [{"Seasons": []},
                                {"Seasons": [{"SeasonId": 5, "SeasonDisplay": 1, "RaidId": 1, "Terrain": "Street", "Start": 10, "End": 20}],
                                 "EliminateSeasons": [{"SeasonId": 5, "SeasonDisplay": "BETA", "RaidId": 1, "Terrain": "Outdoor", "Start": 30, "End": 40, "OpenDifficulty": {"LightArmor": 5}}]},
                                {"Seasons": []}]
            }),
            localization: json!({"BulletType": {"Explosion": "폭발"}, "StageType": {"ChaserA": "현상수배 - 고가도로"}, "AdaptationType": {"Street": "시가지", "Outdoor": "야외"},
                                "ArmorType": {"LightArmor": "경장갑"}, "TimeAttackStage": {"Shooting": "사격 시험"}}),
            config: json!({"TypeEffectiveness": {
                "Explosion": {"LightArmor": 20000, "HeavyArmor": 10000},
                "Pierce": {"LightArmor": 5000, "HeavyArmor": 20000}}}),
        }
    }

    fn db() -> (Connection, Counts) {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::create(&conn).unwrap();
        let n = load(&mut conn, &dataset(), "2026-11-23T00:00:00Z").unwrap();
        (conn, n)
    }

    #[test]
    fn rows_are_loaded_and_counted() {
        let (conn, n) = db();
        assert_eq!(
            n,
            Counts {
                labels: 6,
                type_effectiveness: 4,
                students: 4,
                furniture: 4,
                furniture_interactions: 3,
                interactions_skipped_style: 1,
                interactions_duplicate: 1,
                enemies: 7,
                stages: 2,
                formations: 2,
                formation_enemies: 3,
                formation_weights: 3,
                raids: 4,
                raid_difficulties: 12,
                raid_difficulty_enemies: 17,
                raid_weights: 17,
                raid_seasons: 1,
                eliminate_seasons: 1,
            }
        );
        let name: String = conn.query_row("SELECT name_ko FROM stage WHERE id = 30101", [], |r| r.get(0)).unwrap();
        assert_eq!(name, "현상수배 - 고가도로", "a bounty is named by its type");
        let terrain: String = conn.query_row("SELECT terrain FROM raid WHERE kind = 'TimeAttack'", [], |r| r.get(0)).unwrap();
        assert_eq!(terrain, r#"["Outdoor"]"#, "a single terrain becomes a one-element list");
    }

    #[test]
    fn enemies_are_weighted_by_hp_at_the_encounters_level_and_the_mix_follows() {
        let (conn, _) = db();
        // stage 1011101: formation 1 has two level-1 minions (100 HP), formation 2 a level-1 champion (1000) and two minions
        let share = |armor: &str| -> f64 { conn.query_row("SELECT share FROM v_stage_armor_share WHERE stage_id = 1011101 AND armor_type = ?1", [armor], |r| r.get(0)).unwrap() };
        assert!((share("LightArmor") - 1.0).abs() < 1e-9);
        let weights: Vec<(f64, Option<String>)> =
            conn.prepare("SELECT weight, excluded FROM raid_difficulty_enemy_weight WHERE raid_kind = 'Raid' AND raid_id = 1 AND idx = 0 ORDER BY ordinal").unwrap().query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().collect::<Result<_, _>>().unwrap();
        // the Boss (300k), a body at the Boss's HP that is not counted again, a smaller light part
        assert_eq!(weights, vec![(300_000.0, None), (0.0, Some("mirror".into())), (100_000.0, None)]);
        let mix: Vec<(String, f64)> = conn
            .prepare("SELECT armor_type, share FROM v_raid_armor_share WHERE raid_kind = 'Raid' AND raid_id = 1 AND idx = 0 ORDER BY share DESC")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(mix.iter().map(|(a, _)| a.as_str()).collect::<Vec<_>>(), ["HeavyArmor", "LightArmor"]);
        assert!((mix[0].1 - 0.75).abs() < 1e-9 && (mix[1].1 - 0.25).abs() < 1e-9, "{mix:?}");
        // a summon weighs nothing: Raid 1 idx 4 holds one and its armor is not in the mix
        let summoned: f64 = conn.query_row("SELECT weight FROM raid_difficulty_enemy_weight WHERE raid_id = 1 AND idx = 4 AND ordinal = 0", [], |r| r.get(0)).unwrap();
        assert_eq!(summoned, 0.0);
        // the champion of the second formation at level 1 weighs its level-1 HP
        let hp: i64 = conn.query_row("SELECT hp FROM v_stage_enemy WHERE formation_id = 101110102 AND rank = 'Champion'", [], |r| r.get(0)).unwrap();
        assert_eq!(hp, 1000);
    }

    #[test]
    fn furniture_and_global_interactions_are_loaded() {
        let (conn, _) = db();
        let (name, w, h, comfort, released, flag): (String, i64, i64, i64, i64, i64) = conn
            .query_row("SELECT name_ko, size_w, size_h, comfort, released_global, interaction_global FROM furniture WHERE id = 100", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)))
            .unwrap();
        assert_eq!((name.as_str(), w, h, comfort, released, flag), ("스타 테이블", 4, 3, 120, 1, 1));
        let rows: Vec<(i64, i64, i64)> = conn
            .prepare("SELECT furniture_id, student_id, extra FROM furniture_interaction ORDER BY furniture_id, student_id")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        // only index 1 (Global) counts: student 3's link exists on JP only, the alternate style (4) is skipped
        assert_eq!(rows, vec![(100, 1, 3), (100, 2, 1), (101, 1, 0)]);
        let fk: i64 = conn.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |r| r.get(0)).unwrap();
        assert_eq!(fk, 0);
    }

    #[test]
    fn an_interaction_with_unknown_furniture_is_an_error() {
        let mut d = dataset();
        d.students["2"]["FurnitureInteraction"] = json!([[], [[999, 0]], []]);
        let mut conn = Connection::open_in_memory().unwrap();
        crate::create(&conn).unwrap();
        let e = load(&mut conn, &d, "t").unwrap_err();
        assert!(format!("{e:#}").contains("furniture 999"), "{e:#}");
    }

    #[test]
    fn the_better_matching_student_ranks_first_for_a_stage() {
        let (conn, _) = db();
        // stage 1011101 holds three light-armored enemies: explosion (200 %) beats pierce (50 %)
        let rows: Vec<(i64, i64, Option<i64>)> = conn
            .prepare("SELECT student_id, effectiveness_bp, terrain_adapt FROM v_student_vs_stage WHERE stage_id = 1011101 ORDER BY effectiveness_bp DESC")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(rows, vec![(1, 20000, Some(3)), (2, 5000, Some(1))], "student 3 is not released on Global and is left out");
    }

    #[test]
    fn a_unknown_enemy_in_a_formation_is_an_error_not_a_silent_gap() {
        let mut d = dataset();
        d.stages["1011101"]["Formations"][0]["EnemyList"] = json!([999]);
        let mut conn = Connection::open_in_memory().unwrap();
        crate::create(&conn).unwrap();
        let e = load(&mut conn, &d, "t").unwrap_err();
        assert!(format!("{e:#}").contains("enemy 999"), "{e:#}");
    }

    type BossRow = (i64, Option<String>, Option<String>, Option<String>, bool);

    fn boss_rows(conn: &Connection, kind: &str, id: i64) -> Vec<BossRow> {
        conn.prepare("SELECT idx, label, boss_bullet_type, boss_armor_type, released_global FROM v_raid_difficulty WHERE raid_kind = ?1 AND raid_id = ?2 ORDER BY idx")
            .unwrap()
            .query_map(params![kind, id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    }

    #[test]
    fn a_raids_attack_type_is_kept_per_difficulty() {
        let (conn, _) = db();
        let rows = boss_rows(&conn, "Raid", 1);
        let attack: Vec<_> = rows.iter().map(|r| r.2.as_deref().unwrap()).collect();
        assert_eq!(attack, ["Normal", "Normal", "Normal", "Normal", "Normal", "Pierce"], "Insane (position 5) switches to the Insane attack type");
        let labels: Vec<_> = rows.iter().map(|r| r.1.as_deref().unwrap()).collect();
        assert_eq!(labels, ["Normal", "Hard", "VeryHard", "HardCore", "Extreme", "Insane"]);
        let open: Vec<_> = rows.iter().map(|r| r.4).collect();
        assert_eq!(open, [true, true, true, true, true, false], "Global MaxDifficulty is 4 here: Insane is closed");
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM raid_difficulty_enemy WHERE raid_kind = 'Raid' AND idx = 4", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 2, "summons stay linked, enemy.rank tells them apart");
        let boss: i64 = conn.query_row("SELECT boss_enemy_id FROM raid_difficulty WHERE raid_kind = 'Raid' AND idx = 4", [], |r| r.get(0)).unwrap();
        assert_eq!(boss, 7300100, "the lead enemy is the first Boss, not the first listed enemy");
    }

    #[test]
    fn a_time_attack_is_named_by_its_test_terrain_and_armor() {
        let (conn, _) = db();
        let (name, source_name, dungeon): (String, Option<String>, String) =
            conn.query_row("SELECT display_ko, name_ko, dungeon_type FROM raid WHERE kind = 'TimeAttack'", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap();
        assert_eq!((name.as_str(), source_name, dungeon.as_str()), ("사격 시험 · 야외 · 경장갑", None, "Shooting"));
        let (levels, boss): (String, i64) = conn
            .query_row("SELECT GROUP_CONCAT(level), MIN(boss_enemy_id) FROM raid_difficulty WHERE raid_kind = 'TimeAttack'", [], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap();
        assert_eq!((levels.as_str(), boss), ("25,40", 7000013), "a time attack's formations are its difficulties; the Champion leads");
        let named: String = conn.query_row("SELECT display_ko FROM raid WHERE kind = 'Raid'", [], |r| r.get(0)).unwrap();
        assert_eq!(named, "비나", "a source name is used as it is");
    }

    #[test]
    fn world_raids_and_floor_raids_get_their_own_difficulty_names() {
        let (conn, _) = db();
        let labels: Vec<_> = boss_rows(&conn, "WorldRaid", 814000).into_iter().map(|r| (r.1, r.2)).collect();
        assert_eq!(labels, [(Some("HardCore".into()), Some("Normal".into())), (Some("Extreme".into()), Some("Pierce".into()))], "the source's own names, not positions");
        let floors: Vec<(i64, Option<i64>, Option<i64>)> = conn
            .prepare("SELECT idx, floor_start, battle_duration FROM raid_difficulty WHERE raid_kind = 'MultiFloorRaid' ORDER BY idx")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(floors, [(0, Some(1), Some(270)), (1, Some(25), Some(270))]);
    }

    #[test]
    fn both_season_lists_are_stored_with_their_terrain() {
        let (conn, _) = db();
        type Season = (String, i64, Option<String>, String, Option<String>);
        let rows: Vec<Season> = conn
            .prepare("SELECT mode, season_id, season_display, terrain, open_difficulty FROM raid_season ORDER BY mode")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(
            rows,
            [("Eliminate".into(), 5, Some("BETA".into()), "Outdoor".into(), Some(r#"{"LightArmor":5}"#.into())), ("Raid".into(), 5, Some("1".into()), "Street".into(), None)],
            "the same season id can exist in both modes"
        );
    }

    #[test]
    fn a_raid_enemy_that_does_not_exist_is_an_error() {
        let mut d = dataset();
        d.raids["Raid"][0]["EnemyList"][0] = json!([999]);
        let mut conn = Connection::open_in_memory().unwrap();
        crate::create(&conn).unwrap();
        let e = load(&mut conn, &d, "t").unwrap_err();
        assert!(format!("{e:#}").contains("enemy 999"), "{e:#}");
    }

    #[test]
    fn a_season_of_an_unknown_raid_is_an_error() {
        let mut d = dataset();
        d.raids["RaidSeasons"][1]["Seasons"][0]["RaidId"] = json!(99);
        let mut conn = Connection::open_in_memory().unwrap();
        crate::create(&conn).unwrap();
        assert!(load(&mut conn, &d, "t").is_err());
    }

    #[test]
    fn lead_enemy_prefers_a_boss_then_a_champion() {
        let rank = |e: i64| match e {
            1 => Some("Minion".to_string()),
            2 => Some("Champion".to_string()),
            3 => Some("Boss".to_string()),
            _ => None,
        };
        assert_eq!(lead_enemy(&[1, 2, 3], rank), Some(3));
        assert_eq!(lead_enemy(&[1, 2], rank), Some(2));
        assert_eq!(lead_enemy(&[1], rank), None);
        assert_eq!(lead_enemy(&[], rank), None);
    }

    #[test]
    fn building_twice_replaces_the_file_with_the_same_content() {
        let dir = std::env::temp_dir().join(format!("wikidb-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("wiki.db");
        let a = crate::build(&path, &dataset(), "t").unwrap();
        let b = crate::build(&path, &dataset(), "t").unwrap();
        assert_eq!(a, b);
        let conn = Connection::open(&path).unwrap();
        let students: i64 = conn.query_row("SELECT COUNT(*) FROM student", [], |r| r.get(0)).unwrap();
        assert_eq!(students, 4);
        assert!(!dir.join("wiki.db.tmp").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
