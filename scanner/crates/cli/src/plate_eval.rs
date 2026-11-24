//! `--eval-plates <dir>`: scores the duty-list plate reading on labelled plate pictures with the real engine.
//!
//! `<dir>` holds `labels.json` (file name -> {"label": ...}) and the PNG crops. Two readings are scored by exact match of the
//! raw text (spaces removed, no dictionary): what the detector alone reads, and what the scan's plate path reads.
use crate::progress_scan::{detect_plate, read_plate};
use anyhow::{Context, Result};
use baocr_core::ocr_api::Ocr;
use std::path::Path;
use std::time::Instant;

struct Row {
    file: String,
    label: String,
}

fn load(dir: &Path) -> Result<Vec<Row>> {
    let raw = std::fs::read(dir.join("labels.json")).with_context(|| format!("{}/labels.json", dir.display()))?;
    let v: serde_json::Value = serde_json::from_slice(&raw)?;
    let mut rows: Vec<Row> = v
        .as_object()
        .context("labels.json is not an object")?
        .iter()
        .filter_map(|(f, e)| Some(Row { file: f.clone(), label: e.get("label")?.as_str()?.to_string() }))
        .collect();
    rows.sort_by(|a, b| a.file.cmp(&b.file));
    Ok(rows)
}

fn score(name: &str, rows: &[Row], reads: &[String], ms: f64) -> String {
    let ok = rows.iter().zip(reads).filter(|(r, t)| r.label == **t).count();
    let mut out = format!("{name}: {ok}/{} exact, {:.0} ms per plate\n", rows.len(), ms / rows.len() as f64);
    for (r, t) in rows.iter().zip(reads).filter(|(r, t)| r.label != **t) {
        out.push_str(&format!("  wrong {} label {:?} read {:?}\n", r.file, r.label, t));
    }
    out
}

pub fn run(ocr: &dyn Ocr, dir: &Path) -> Result<String> {
    let rows = load(dir)?;
    let images: Vec<image::RgbImage> =
        rows.iter().map(|r| Ok(image::open(dir.join(&r.file))?.to_rgb8())).collect::<Result<_>>()?;
    // one unmeasured read warms the model sessions
    if let Some(first) = images.first() {
        let _ = detect_plate(ocr, first);
    }
    let mut report = String::new();
    type Reader = fn(&dyn Ocr, &image::RgbImage) -> String;
    let modes: [(&str, Reader); 2] = [("detector alone", detect_plate), ("scan plate path", read_plate)];
    for (name, read) in modes {
        let t = Instant::now();
        let reads: Vec<String> = images.iter().map(|img| read(ocr, img)).collect();
        report.push_str(&score(name, &rows, &reads, t.elapsed().as_secs_f64() * 1000.0));
    }
    Ok(report)
}
