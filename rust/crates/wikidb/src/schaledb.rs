//! SchaleDB collector. Reads the site's Korean data files (the Global server in Korean;
//! the GitHub repository is archived, the site data keeps being updated).
//!
//! `fetch` only downloads, `load` only writes the database, so `load` is tested
//! without a network.
use anyhow::{anyhow, bail, Context, Result};
use rusqlite::{params, Connection, Transaction};
use serde_json::Value;

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
    pub enemies: usize,
    pub stages: usize,
    pub formations: usize,
    pub formation_enemies: usize,
    pub raids: usize,
    pub raid_seasons: usize,
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
    enemies(&tx, d, &mut n)?;
    stages(&tx, d, &mut n)?;
    raids(&tx, d, &mut n)?;
    tx.execute("INSERT INTO meta (key, value) VALUES ('built_at', ?1)", params![fetched_at])?;
    tx.commit()?;
    Ok(n)
}

/// Localization sections that name a closed code set, stored under the kind the schema uses.
const LABEL_KINDS: [(&str, &str); 7] = [
    ("BulletType", "BulletType"),
    ("ArmorType", "ArmorType"),
    ("StageType", "StageType"),
    ("EnemyRank", "EnemyRank"),
    ("SquadType", "SquadType"),
    ("TacticRole", "TacticRole"),
    ("AdaptationType", "Terrain"),
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
            for (j, e) in f.get("EnemyList").and_then(|x| x.as_array()).into_iter().flatten().enumerate() {
                let enemy = e.as_i64().ok_or_else(|| anyhow!("stage {id}: enemy id is not a number"))?;
                member.execute(params![fid, j as i64, enemy]).with_context(|| format!("stage {id} formation {fid}: enemy {enemy}"))?;
                n.formation_enemies += 1;
            }
        }
    }
    Ok(())
}

/// Raid-like encounters, keyed in raids.json by kind.
const RAID_KINDS: [&str; 5] = ["Raid", "TimeAttack", "WorldRaid", "MultiFloorRaid", "InteractiveWorldRaid"];

fn raids(tx: &Transaction, d: &Dataset, n: &mut Counts) -> Result<()> {
    let mut st = tx.prepare(
        "INSERT INTO raid (id, kind, path_name, name_ko, bullet_type, armor_type, terrain, released_global, source_id, raw)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,1,?9)",
    )?;
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
                text_or_first(r, "BulletType"),
                text_or_first(r, "ArmorType"),
                terrain,
                r.get("IsReleased").is_some().then(|| released_global(r)),
                r.to_string(),
            ])
            .with_context(|| format!("{kind} {id}"))?;
            n.raids += 1;
        }
    }
    let seasons = d.raids.get("RaidSeasons").and_then(|x| x.get(GLOBAL)).and_then(|x| x.get("Seasons")).and_then(|x| x.as_array());
    let mut st = tx.prepare("INSERT INTO raid_season (season_id, raid_id, terrain, start_at, end_at) VALUES (?1,?2,?3,?4,?5)")?;
    for s in seasons.into_iter().flatten() {
        st.execute(params![int(s, "SeasonId"), int(s, "RaidId"), text(s, "Terrain"), int(s, "Start"), int(s, "End")])?;
        n.raid_seasons += 1;
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
                      "BulletType": "Explosion", "ArmorType": "LightArmor", "StarGrade": 3, "StreetBattleAdaptation": 3, "OutdoorBattleAdaptation": 2, "IndoorBattleAdaptation": 1},
                "2": {"Id": 2, "IsReleased": [true, true, true], "PathName": "b", "DevName": "b", "Name": "관통학생", "SquadType": "Main",
                      "BulletType": "Pierce", "ArmorType": "LightArmor", "StarGrade": 3, "StreetBattleAdaptation": 1, "OutdoorBattleAdaptation": 1, "IndoorBattleAdaptation": 1},
                "3": {"Id": 3, "IsReleased": [true, false, true], "PathName": "c", "DevName": "c", "Name": "미출시", "SquadType": "Main",
                      "BulletType": "Pierce", "ArmorType": "LightArmor", "StarGrade": 3}
            }),
            enemies: json!({"Enemies": {
                "7000001": {"Id": 7000001, "Name": "불량배", "Rank": "Minion", "BulletType": "Normal", "ArmorType": "LightArmor"},
                "7000013": {"Id": 7000013, "Name": "무자비한 불량배", "Rank": "Champion", "BulletType": "Normal", "ArmorType": "LightArmor"}
            }}),
            stages: json!({
                "1011101": {"Id": 1011101, "Category": "Campaign", "Difficulty": 0, "Area": 1, "Stage": 1, "Name": "아오바 중심구 35번지", "Terrain": "Street", "Level": 1,
                            "EntryCost": [[5, 10]], "Formations": [
                                {"Id": 101110101, "UnitGrade": "Grade1", "EnemyList": [7000001, 7000001]},
                                {"Id": 101110102, "UnitGrade": "Boss", "EnemyList": [7000013]}]},
                "30101": {"Id": 30101, "Category": "Bounty", "Type": "ChaserA", "Stage": 1, "Terrain": "Outdoor", "Formations": []}
            }),
            raids: json!({
                "Raid": [{"Id": 1, "IsReleased": [true, true, false], "PathName": "binah", "Terrain": ["Outdoor", "Street"], "BulletType": "Normal", "ArmorType": "HeavyArmor"}],
                "TimeAttack": [{"Id": 1, "Terrain": "Outdoor", "BulletType": "Normal", "ArmorType": "LightArmor"}],
                "RaidSeasons": [{"Seasons": []}, {"Seasons": [{"SeasonId": 5, "RaidId": 4, "Terrain": "Street", "Start": 10, "End": 20}]}, {"Seasons": []}]
            }),
            localization: json!({"BulletType": {"Explosion": "폭발"}, "StageType": {"ChaserA": "현상수배 - 고가도로"}, "AdaptationType": {"Street": "시가지"}}),
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
            Counts { labels: 3, type_effectiveness: 4, students: 3, enemies: 2, stages: 2, formations: 2, formation_enemies: 3, raids: 2, raid_seasons: 1 }
        );
        let name: String = conn.query_row("SELECT name_ko FROM stage WHERE id = 30101", [], |r| r.get(0)).unwrap();
        assert_eq!(name, "현상수배 - 고가도로", "a bounty is named by its type");
        let terrain: String = conn.query_row("SELECT terrain FROM raid WHERE kind = 'TimeAttack'", [], |r| r.get(0)).unwrap();
        assert_eq!(terrain, r#"["Outdoor"]"#, "a single terrain becomes a one-element list");
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
        assert_eq!(students, 3);
        assert!(!dir.join("wiki.db.tmp").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
