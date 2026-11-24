//! Story and event lists from arisu-archive/bluearchive-data (the Global server's client tables, decoded).
//!
//! The repository states no licence file and its data is the game's own, so nothing of it is stored in this
//! repository: `fetch` downloads three tables at the commit the `global` branch has right now, `load` keeps only
//! the numbers the add-on needs (ids, volume, chapter and episode numbers, open and close times) and records the
//! commit in `source.version`. The DB file is not distributed. `fetch` only downloads and `load` only writes, so
//! `load` is tested without a network.
use anyhow::{anyhow, bail, Context, Result};
use rusqlite::{params, Connection};
use serde_json::Value;

const REPO: &str = "arisu-archive/bluearchive-data";
const BRANCH: &str = "global";
const SOURCE_NAME: &str = "arisu-archive";
const SOURCE_NOTE: &str = "Client tables decoded by arisu-archive/bluearchive-data (global branch). No licence file in that repository; the data belongs to Nexon and Yostar. Numbers only; the DB file is not redistributed.";

pub struct StoryData {
    /// The commit of the `global` branch the tables were read at.
    pub commit: String,
    pub scenario_mode: Value,
    pub event_season: Value,
    pub event_scenario: Value,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct StoryCounts {
    pub episodes: usize,
    pub events: usize,
    pub event_scenarios: usize,
}

fn get(url: &str) -> Result<Value> {
    ureq::get(url).set("User-Agent", "baocr-wikidb").call().with_context(|| format!("GET {url}"))?.into_json().with_context(|| format!("parsing {url}"))
}

pub fn fetch() -> Result<StoryData> {
    let head = get(&format!("https://api.github.com/repos/{REPO}/commits/{BRANCH}"))?;
    let commit = head.get("sha").and_then(Value::as_str).ok_or_else(|| anyhow!("no commit sha for {REPO}@{BRANCH}"))?.to_string();
    let table = |name: &str| get(&format!("https://raw.githubusercontent.com/{REPO}/{commit}/ExcelDB/{name}.json"));
    Ok(StoryData { scenario_mode: table("ScenarioModeExcel")?, event_season: table("EventContentSeasonExcel")?, event_scenario: table("EventContentScenarioExcel")?, commit })
}

fn rows<'a>(v: &'a Value, what: &str) -> Result<&'a Vec<Value>> {
    v.get("data_list").and_then(Value::as_array).ok_or_else(|| anyhow!("{what}: no data_list"))
}

fn int(r: &Value, k: &str) -> Option<i64> {
    r.get(k).and_then(Value::as_i64)
}

fn flag(r: &Value, k: &str) -> i64 {
    r.get(k).and_then(Value::as_bool).unwrap_or(false) as i64
}

/// A time column: the source writes "" for none.
fn time(r: &Value, k: &str) -> Option<String> {
    r.get(k).and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string)
}

pub fn load(conn: &mut Connection, d: &StoryData, fetched_at: &str) -> Result<StoryCounts> {
    let tx = conn.transaction()?;
    let url = format!("https://github.com/{REPO}/tree/{}", d.commit);
    tx.execute("INSERT INTO source (name, url, fetched_at, version, note) VALUES (?1, ?2, ?3, ?4, ?5)", params![SOURCE_NAME, url, fetched_at, d.commit, SOURCE_NOTE])?;
    let source_id = tx.last_insert_rowid();
    let mut n = StoryCounts::default();
    for r in rows(&d.scenario_mode, "ScenarioModeExcel")? {
        let (Some(id), Some(mt), Some(st)) = (int(r, "mode_id"), int(r, "mode_type"), int(r, "sub_type")) else {
            bail!("ScenarioModeExcel: a row without mode_id, mode_type or sub_type")
        };
        tx.execute(
            "INSERT INTO story_episode (mode_id, mode_type, sub_type, volume_id, chapter_id, episode_id, open_at, source_id) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
            params![id, mt, st, int(r, "volume_id").unwrap_or(0), int(r, "chapter_id").unwrap_or(0), int(r, "episode_id").unwrap_or(0), time(r, "scenario_open_date"), source_id],
        )?;
        n.episodes += 1;
    }
    for r in rows(&d.event_season, "EventContentSeasonExcel")? {
        let (Some(id), Some(t)) = (int(r, "event_content_id"), int(r, "event_content_type")) else {
            bail!("EventContentSeasonExcel: a row without event_content_id or event_content_type")
        };
        tx.execute(
            "INSERT INTO event (event_content_id, event_type, name_key, display, open_at, close_at, source_id) VALUES (?1,?2,?3,?4,?5,?6,?7)",
            params![id, t, r.get("name").and_then(Value::as_str), flag(r, "event_display"), time(r, "event_content_open_time"), time(r, "event_content_close_time"), source_id],
        )?;
        n.events += 1;
    }
    for r in rows(&d.event_scenario, "EventContentScenarioExcel")? {
        let (Some(id), Some(ev)) = (int(r, "id"), int(r, "event_content_id")) else { bail!("EventContentScenarioExcel: a row without id or event_content_id") };
        tx.execute(
            "INSERT INTO event_scenario (id, event_content_id, ordinal, is_recollection, source_id) VALUES (?1,?2,?3,?4,?5)",
            params![id, ev, int(r, "order").unwrap_or(0), flag(r, "is_recollection"), source_id],
        )?;
        n.event_scenarios += 1;
    }
    tx.commit()?;
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn data() -> StoryData {
        StoryData {
            commit: "abc123".into(),
            scenario_mode: json!({"data_list": [
                {"mode_id": 10000, "mode_type": 6, "sub_type": 2, "volume_id": 1, "chapter_id": 1, "episode_id": 1, "scenario_open_date": "2023-05-01 04:00:00"},
                {"mode_id": 10001, "mode_type": 6, "sub_type": 2, "volume_id": 1, "chapter_id": 1, "episode_id": 2, "scenario_open_date": ""}]}),
            event_season: json!({"data_list": [
                {"event_content_id": 701, "name": "Event_Name_701", "event_content_type": 0, "event_display": true, "event_content_open_time": "2024-06-11 11:00:00", "event_content_close_time": "2024-06-25 10:59:59"}]}),
            event_scenario: json!({"data_list": [{"id": 2, "event_content_id": 701, "order": 1, "is_recollection": false}]}),
        }
    }

    #[test]
    fn the_numbers_are_stored_with_the_commit_as_the_source_version() {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::create(&conn).unwrap();
        let n = load(&mut conn, &data(), "t").unwrap();
        assert_eq!(n, StoryCounts { episodes: 2, events: 1, event_scenarios: 1 });
        let v: String = conn.query_row("SELECT version FROM source WHERE name = 'arisu-archive'", [], |r| r.get(0)).unwrap();
        assert_eq!(v, "abc123");
        let open: Option<String> = conn.query_row("SELECT open_at FROM story_episode WHERE mode_id = 10001", [], |r| r.get(0)).unwrap();
        assert_eq!(open, None, "an empty time is null");
        let ep: i64 = conn.query_row("SELECT episode_id FROM story_episode WHERE mode_id = 10000", [], |r| r.get(0)).unwrap();
        assert_eq!(ep, 1);
    }

    #[test]
    fn a_table_of_another_shape_is_refused() {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::create(&conn).unwrap();
        let mut d = data();
        d.scenario_mode = json!({"rows": []});
        assert!(load(&mut conn, &d, "t").is_err());
    }
}
