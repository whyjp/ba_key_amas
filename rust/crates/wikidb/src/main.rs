//! wikidb: builds the reference database from public sources.
//! `cargo run -p baocr-wikidb --release [-- <out.db>]` (default `../wikidb/wiki.db`).
use anyhow::Result;
use baocr_wikidb::{build, schaledb};
use std::path::PathBuf;

fn main() -> Result<()> {
    let out: PathBuf = std::env::args().nth(1).unwrap_or_else(|| "../wikidb/wiki.db".into()).into();
    if let Some(dir) = out.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)?;
    }
    eprintln!("downloading SchaleDB data ...");
    let data = schaledb::fetch()?;
    let counts = build(&out, &data, &chrono::Utc::now().to_rfc3339())?;
    println!("{}\n{counts:#?}", out.display());
    Ok(())
}
