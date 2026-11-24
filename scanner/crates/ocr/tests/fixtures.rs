//! The engine reads the committed fixture frames the way their `.boxes.txt` say (a few boxes of the
//! committed files have drifted since; the bound below is what the CPU path gives today). Run with
//! `--features directml` to hold the GPU path to the same bound. Skipped when the models are absent.
use baocr_core::ocr_api::Ocr;
use baocr_ocr::PpOcr;
use std::path::PathBuf;

#[test]
fn the_engine_reads_the_fixtures_as_committed() {
    let models = PpOcr::default_models_dir();
    if !models.join("PP-OCRv6_det_small.onnx").exists() {
        eprintln!("models not found: skipped");
        return;
    }
    let ocr = PpOcr::new(&models).unwrap();
    eprintln!("OCR device: {}", ocr.device());
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../fixtures/screens");
    let (mut missing, mut extra, mut frames) = (0, 0, 0);
    for e in std::fs::read_dir(&dir).unwrap() {
        let png = e.unwrap().path();
        let boxes = png.with_extension("boxes.txt");
        if png.extension().is_none_or(|x| x != "png") || !boxes.exists() {
            continue;
        }
        frames += 1;
        let want: Vec<(f32, f32, String)> = std::fs::read_to_string(&boxes)
            .unwrap()
            .lines()
            .filter_map(|l| {
                let (b, t) = l.split_once('\t')?;
                let n: Vec<f32> = b.split(',').filter_map(|s| s.parse().ok()).collect();
                Some((n[0], n[1], t.to_string()))
            })
            .collect();
        let got = ocr.boxes(&image::open(&png).unwrap().to_rgb8()).unwrap();
        let mut used = vec![false; got.len()];
        for (x, y, t) in &want {
            match (0..got.len()).find(|&i| !used[i] && got[i].text.trim() == t.trim() && (got[i].x0 - x).abs() <= 3.0 && (got[i].y0 - y).abs() <= 3.0) {
                Some(i) => used[i] = true,
                None => {
                    missing += 1;
                    println!("{}: committed but not read: {t:?} at {x:.0},{y:.0}", png.display());
                }
            }
        }
        for (i, u) in used.iter().enumerate() {
            if !u {
                println!("{}: read but not committed: {:?} at {:.0},{:.0}", png.display(), got[i].text, got[i].x0, got[i].y0);
            }
        }
        extra += used.iter().filter(|u| !**u).count();
    }
    assert!(frames >= 15, "only {frames} fixture frames found");
    assert!(missing <= 2 && extra <= 2, "{missing} committed boxes not read, {extra} boxes not committed (device {})", ocr.device());
}
