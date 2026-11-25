//! Checks the Rust TrOCR path against the research reference: the 384x384 input must equal the reference image, and the
//! label must be the best of all names. Prints the time per label.
//!
//!     cargo run -p baocr-ocr --release --example trocr_probe -- <research>/ref <model dir with encoder.onnx, decoder.onnx> [labels]
use baocr_ocr::trocr::{pixel_values, square_input, TrOcr};
use std::time::Instant;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let (refdir, models) = (std::path::PathBuf::from(&args[1]), std::path::PathBuf::from(&args[2]));
    let count: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(10);
    let reference: serde_json::Value = serde_json::from_slice(&std::fs::read(refdir.join("reference.json"))?)?;
    let plates = reference["plates"].as_array().unwrap();
    let mut names: Vec<String> = serde_json::from_str::<std::collections::HashMap<String, serde_json::Value>>(include_str!("../data/trocr_surfaces.json"))?.into_keys().collect();
    names.sort();
    let trocr = TrOcr::new(&models)?;
    let all: std::collections::HashMap<String, Vec<Vec<i64>>> = serde_json::from_str(include_str!("../data/trocr_surfaces.json"))?;
    let forms: Vec<(usize, Vec<i64>)> = names.iter().enumerate().flat_map(|(k, n)| all[n].iter().map(move |f| (k, f.clone()))).collect();
    let seqs: Vec<&[i64]> = forms.iter().map(|(_, f)| f.as_slice()).collect();
    let (mut same_input, mut right, mut total) = (0, 0, 0);
    let (mut t_prep, mut t_enc, mut t_score) = (0.0, 0.0, 0.0);
    for (i, plate) in plates.iter().enumerate().take(count) {
        let label = plate["label"].as_str().unwrap();
        for cond in ["clean", "blur0.7", "blur0.5"] {
            let png = |kind: &str| image::open(refdir.join(format!("png/{i:03}_{cond}_{kind}.png"))).unwrap().to_rgb8();
            let t0 = Instant::now();
            let square = square_input(&png("plate"));
            t_prep += t0.elapsed().as_secs_f64();
            let expected = png("input384");
            let diff = square.pixels().zip(expected.pixels()).filter(|(a, b)| a != b).count();
            same_input += usize::from(diff == 0);
            let t1 = Instant::now();
            let cross = trocr.encode(pixel_values(&square))?;
            t_enc += t1.elapsed().as_secs_f64();
            let t2 = Instant::now();
            // score through `choose`'s own path: encode again would double the time, so score the names here
            let scores = {
                let c = &cross;
                let s = trocr.score_tokens(c, &seqs)?;
                let mut best = vec![f32::NEG_INFINITY; names.len()];
                for ((sum, n), (k, _)) in s.into_iter().zip(&forms) {
                    best[*k] = best[*k].max(sum / n as f32);
                }
                best
            };
            t_score += t2.elapsed().as_secs_f64();
            let top = (0..names.len()).max_by(|&a, &b| scores[a].total_cmp(&scores[b])).unwrap();
            right += usize::from(names[top] == label);
            total += 1;
            if names[top] != label || diff != 0 {
                eprintln!("{i:03} {cond}: label {label} -> {} (input differs in {diff} pixels)", names[top]);
            }
        }
    }
    println!("input identical {same_input}/{total}, label is best {right}/{total}");
    let n = total as f64;
    println!("per label: prepare {:.0} ms, encode {:.0} ms, score {} names {:.0} ms", t_prep / n * 1e3, t_enc / n * 1e3, names.len(), t_score / n * 1e3);
    Ok(())
}

