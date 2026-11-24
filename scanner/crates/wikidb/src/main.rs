//! wikidb: builds the reference database from public sources.
//! `cargo run -p baocr-wikidb --release [-- <out.db>]` (default `../wikidb/wiki.db`).
use anyhow::Result;
use baocr_wikidb::{build_with, schaledb, story};
use std::path::PathBuf;

fn main() -> Result<()> {
    let out: PathBuf = std::env::args().nth(1).unwrap_or_else(|| "../wikidb/wiki.db".into()).into();
    if let Some(dir) = out.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)?;
    }
    eprintln!("downloading SchaleDB data ...");
    let data = schaledb::fetch()?;
    eprintln!("downloading story and event tables (arisu-archive/bluearchive-data, global) ...");
    // the story lists are optional: without them the DB still serves everything else
    let story = story::fetch().map_err(|e| eprintln!("story tables skipped: {e:#}")).ok();
    let (counts, story_counts) = build_with(&out, &data, story.as_ref(), &chrono::Utc::now().to_rfc3339())?;
    println!("{}\n{counts:#?}\n{story_counts:#?}", out.display());
    Ok(())
}
