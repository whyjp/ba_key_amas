//! Makes `data/portrait_templates.bin` from the committed portrait assets.
//!
//!     cargo run -p baocr-assist --release --example portrait_seed
//!
//! Run it when the assets in `web/public/portraits` change, and commit the result. The build never runs it.
use baocr_assist::portrait::{asset_template, Book};
use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let mut entries = Vec::new();
    for item in std::fs::read_dir(root.join("web/public/portraits"))? {
        let path = item?.path();
        let Some(id) = path.file_stem().and_then(|s| s.to_str()).and_then(|s| s.parse::<i64>().ok()) else { continue };
        if path.extension().and_then(|e| e.to_str()) != Some("webp") {
            continue;
        }
        entries.push((id, asset_template(&image::open(&path)?)));
    }
    entries.sort_by_key(|(id, _)| *id);
    let bytes = Book::encode(&entries);
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data/portrait_templates.bin");
    std::fs::create_dir_all(out.parent().unwrap())?;
    std::fs::write(&out, &bytes)?;
    println!("{} templates, {} bytes -> {}", entries.len(), bytes.len(), out.display());
    Ok(())
}
