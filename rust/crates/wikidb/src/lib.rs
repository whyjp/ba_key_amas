//! The Blue Archive reference ("wiki") database: one SQLite file built from public
//! data sources. Each source has its own module, so a source that disappears or
//! changes shape does not touch the others. The schema lives in `wikidb/schema.sql`.
pub mod formation;
pub mod schaledb;
pub mod wiki;
pub use formation::{adaptation_factor, evaluate, FormationScore, TypeTable, Verdict};
pub use wiki::{MissionBrief, StudentBrief, Tally, Wiki};

use anyhow::{Context, Result};
use rusqlite::Connection;
use std::path::Path;

pub const SCHEMA: &str = include_str!("../../../../wikidb/schema.sql");

/// A fresh database with the schema applied.
pub fn create(conn: &Connection) -> Result<()> {
    conn.execute_batch(SCHEMA).context("applying wikidb/schema.sql")
}

/// Builds the database at `path`: written beside it first and renamed over it, so a
/// failed run never leaves a half-built file, and running twice gives the same file content.
pub fn build(path: &Path, dataset: &schaledb::Dataset, fetched_at: &str) -> Result<schaledb::Counts> {
    let tmp = path.with_extension("db.tmp");
    let _ = std::fs::remove_file(&tmp);
    let counts = {
        let mut conn = Connection::open(&tmp)?;
        create(&conn)?;
        schaledb::load(&mut conn, dataset, fetched_at)?
    };
    std::fs::rename(&tmp, path).with_context(|| format!("replacing {}", path.display()))?;
    Ok(counts)
}
