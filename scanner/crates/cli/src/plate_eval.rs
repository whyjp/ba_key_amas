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
    report.push_str(&trocr_report(ocr, &rows, &images)?);
    Ok(report)
}


/// The TrOCR comparison (cargo feature `trocr`, folder with encoder.onnx and decoder.onnx in `BAOCR_TROCR`).
#[cfg(feature = "trocr")]
mod trocr_eval {
    use super::*;
    use baocr_ocr::trocr::TrOcr;

    /// The plate as the game would show it from further away or blurred: shrunk by `factor` and enlarged again (the research's
    /// stress conditions, 0.7 and 0.5).
    fn blurred(img: &image::RgbImage, factor: f32) -> image::RgbImage {
        let (w, h) = img.dimensions();
        let small = image::imageops::resize(img, ((w as f32 * factor).round() as u32).max(1), ((h as f32 * factor).round() as u32).max(1), image::imageops::FilterType::Triangle);
        image::imageops::resize(&small, w, h, image::imageops::FilterType::Triangle)
    }

    /// The names TrOCR is scored against: every name of the add-on's dictionary.
    fn names() -> Vec<String> {
        let mut v = baocr_names::all_names();
        v.sort();
        v.dedup();
        v
    }



    pub fn report(ocr: &dyn Ocr, rows: &[Row], images: &[image::RgbImage]) -> Result<String> {
        let mut report = String::new();
            // BAOCR_TROCR=<folder with encoder.onnx and decoder.onnx>: the same plates read by TrOCR against the dictionary, and both
        // engines on blurred copies (what the add-on would make of the name after the dictionary has had its say)
        if let Some(dir) = std::env::var_os("BAOCR_TROCR") {
            let trocr = TrOcr::new(Path::new(&dir))?;
            let all = names();
            let _ = trocr.choose(&images[0], &all)?;
            for (label, factor) in [("clean", 1.0f32), ("blur 0.7", 0.7), ("blur 0.5", 0.5), ("blur 0.4", 0.4)] {
                let imgs: Vec<image::RgbImage> = if factor >= 1.0 { images.to_vec() } else { images.iter().map(|i| blurred(i, factor)).collect() };
                let t = Instant::now();
                let pp: Vec<String> = imgs.iter().map(|img| baocr_names::best_name(&[read_plate(ocr, img).as_str()])).collect();
                let pp_ms = t.elapsed().as_secs_f64() * 1000.0;
                let t = Instant::now();
                let tr: Vec<String> = imgs
                    .iter()
                    .map(|img| {
                        let s = trocr.choose(img, &all).unwrap_or_default();
                        s.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1)).map(|(i, _)| all[i].clone()).unwrap_or_default()
                    })
                    .collect();
                let tr_ms = t.elapsed().as_secs_f64() * 1000.0;
                let pp_ok = rows.iter().zip(&pp).filter(|(r, t)| r.label == **t).count();
                let tr_ok = rows.iter().zip(&tr).filter(|(r, t)| r.label == **t).count();
                let both = rows.iter().zip(pp.iter().zip(&tr)).filter(|(r, (a, b))| r.label == **a || r.label == **b).count();
                report.push_str(&format!("{label}: PP-OCR + dictionary {pp_ok}/{n} ({:.0} ms per plate), TrOCR over the dictionary {tr_ok}/{n} ({:.0} ms per plate), either right {both}/{n}\n", pp_ms / rows.len() as f64, tr_ms / rows.len() as f64, n = rows.len()));
                if factor >= 1.0 || factor == 0.5 {
                    for (r, (a, b)) in rows.iter().zip(pp.iter().zip(&tr)).filter(|(r, (a, b))| r.label != **a || r.label != **b) {
                        report.push_str(&format!("    {} label {:?}: PP {:?}{} TrOCR {:?}{}\n", r.file, r.label, a, if *a == r.label { " ok" } else { " WRONG" }, b, if *b == r.label { " ok" } else { " WRONG" }));
                    }
                }
            }
        }
        Ok(report)
    }
}

#[cfg(feature = "trocr")]
fn trocr_report(ocr: &dyn Ocr, rows: &[Row], images: &[image::RgbImage]) -> Result<String> {
    trocr_eval::report(ocr, rows, images)
}

#[cfg(not(feature = "trocr"))]
fn trocr_report(_: &dyn Ocr, _: &[Row], _: &[image::RgbImage]) -> Result<String> {
    Ok(if std::env::var_os("BAOCR_TROCR").is_some() { "BAOCR_TROCR is set but this build has no TrOCR (build with --features trocr)
".into() } else { String::new() })
}
