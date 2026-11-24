//! The Blue Archive reference ("wiki") database: one SQLite file built from public
//! data sources. Each source has its own module, so a source that disappears or
//! changes shape does not touch the others. The schema lives in `wikidb/schema.sql`.
pub mod formation;
pub mod raid;
pub mod schaledb;
pub mod story;
pub mod weight;
pub mod wiki;
pub use formation::{
    adaptation_factor, attack_multiplier, damage_taken, enemy_armors, enemy_attacks, evaluate, fit_tier, matchup, rate, rates_by_type, special_role, Fit, FormationScore, Matchup, Rating, SupportRole, TypeTable,
    Verdict,
};
pub use raid::{pick_season, RaidBoss, RaidCandidate, RaidDifficulty, RaidKind, RaidSeason, SeasonMode, SeasonPick, SeasonRelation};
pub use wiki::{DecodedLabels, FurnitureBrief, FurnitureStudent, MissionBrief, Share, StudentBrief, Tally, Wiki};

use anyhow::{Context, Result};
use rusqlite::Connection;
use std::path::Path;

pub const SCHEMA: &str = include_str!("../../../../wikidb/schema.sql");

/// The schema version this build reads and writes (meta.schema_version). `Wiki::open` refuses
/// a database of another version: rebuild it with the collector.
pub const SCHEMA_VERSION: u32 = 6;

/// A fresh database with the schema applied.
pub fn create(conn: &Connection) -> Result<()> {
    conn.execute_batch(SCHEMA).context("applying wikidb/schema.sql")
}

/// Builds the database at `path`: written beside it first and renamed over it, so a
/// failed run never leaves a half-built file, and running twice gives the same file content.
pub fn build(path: &Path, dataset: &schaledb::Dataset, fetched_at: &str) -> Result<schaledb::Counts> {
    build_with(path, dataset, None, fetched_at).map(|(counts, _)| counts)
}

/// `build` plus the story and event lists when they were fetched.
pub fn build_with(path: &Path, dataset: &schaledb::Dataset, story: Option<&story::StoryData>, fetched_at: &str) -> Result<(schaledb::Counts, Option<story::StoryCounts>)> {
    let tmp = path.with_extension("db.tmp");
    let _ = std::fs::remove_file(&tmp);
    let counts = {
        let mut conn = Connection::open(&tmp)?;
        create(&conn)?;
        let counts = schaledb::load(&mut conn, dataset, fetched_at)?;
        let story_counts = story.map(|s| story::load(&mut conn, s, fetched_at)).transpose()?;
        (counts, story_counts)
    };
    std::fs::rename(&tmp, path).with_context(|| format!("replacing {}", path.display()))?;
    Ok(counts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_schema_file_declares_the_version_the_code_expects() {
        let conn = Connection::open_in_memory().unwrap();
        create(&conn).unwrap();
        let v: String = conn.query_row("SELECT value FROM meta WHERE key = 'schema_version'", [], |r| r.get(0)).unwrap();
        assert_eq!(v, SCHEMA_VERSION.to_string());
    }

    #[test]
    fn a_database_of_another_version_is_refused_with_a_way_out() {
        let dir = std::env::temp_dir().join(format!("wikidb-version-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("old.db");
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch("CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL); INSERT INTO meta VALUES ('schema_version', '2');").unwrap();
        }
        let e = Wiki::open(&path).err().expect("an old database must not open");
        let msg = format!("{e:#}");
        assert!(msg.contains("version 2") && msg.contains("rebuild"), "{msg}");
        let empty = dir.join("empty.db");
        Connection::open(&empty).unwrap();
        assert!(Wiki::open(&empty).is_err(), "a file that is no wiki database");
        let fresh = dir.join("fresh.db");
        create(&Connection::open(&fresh).unwrap()).unwrap();
        assert!(Wiki::open(&fresh).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
