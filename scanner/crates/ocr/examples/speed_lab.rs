//! OCR speed lab: times the detection + recognition pipeline under different ONNX Runtime and
//! pipeline options and checks every variant's text against the committed fixtures.
//!   cargo run --release -p baocr-ocr --example speed_lab -- [variant-name-substring ...]
//! Env: BAOCR_MODELS (models dir), RUNS (timed runs per frame, default 3).
//! Frames are `fixtures/screens/*.png` that have a sibling `*.boxes.txt`. No game, no desktop.
use baocr_core::ocr_api::{Ocr, TextBox};
use oar_ocr::core::config::{OrtGraphOptimizationLevel as Og, OrtSessionConfig};
use oar_ocr::domain::tasks::TextDetectionConfig;
use oar_ocr::oarocr::OAROCRBuilder;
use oar_ocr::processors::LimitType;
use image::RgbImage;
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Clone, Default)]
struct Variant {
    name: &'static str,
    intra: Option<usize>,
    inter: Option<usize>,
    parallel: Option<bool>,
    opt: Option<Og>,
    mem: Option<bool>,
    rec_batch: Option<usize>,
    limit: Option<u32>,
    directml: bool,
    /// an explicit session config is passed even when every field above is None
    explicit: bool,
}

fn variants() -> Vec<Variant> {
    let v = |name| Variant { name, ..Default::default() };
    vec![
        v("baseline (library defaults)"),
        Variant { explicit: true, ..v("explicit-empty-config") },
        Variant { intra: Some(1), explicit: true, ..v("intra=1") },
        Variant { intra: Some(2), explicit: true, ..v("intra=2") },
        Variant { intra: Some(4), explicit: true, ..v("intra=4") },
        Variant { intra: Some(6), explicit: true, ..v("intra=6") },
        Variant { intra: Some(8), explicit: true, ..v("intra=8") },
        Variant { intra: Some(12), explicit: true, ..v("intra=12") },
        Variant { intra: Some(16), explicit: true, ..v("intra=16") },
        Variant { opt: Some(Og::DisableAll), explicit: true, ..v("opt=off") },
        Variant { opt: Some(Og::Level2), explicit: true, ..v("opt=L2") },
        Variant { opt: Some(Og::Level3), explicit: true, ..v("opt=L3(all)") },
        Variant { mem: Some(true), explicit: true, ..v("mem-pattern=on") },
        Variant { mem: Some(false), explicit: true, ..v("mem-pattern=off") },
        Variant { parallel: Some(true), inter: Some(2), explicit: true, ..v("parallel-exec inter=2") },
        Variant { rec_batch: Some(1), ..v("rec-batch=1") },
        Variant { rec_batch: Some(2), ..v("rec-batch=2") },
        Variant { rec_batch: Some(8), ..v("rec-batch=8") },
        Variant { rec_batch: Some(16), ..v("rec-batch=16") },
        Variant { rec_batch: Some(32), ..v("rec-batch=32") },
        Variant { rec_batch: Some(64), ..v("rec-batch=64") },
        Variant { limit: Some(960), ..v("det-limit=960 (explicit)") },
        Variant { limit: Some(800), ..v("det-limit=800") },
        Variant { limit: Some(736), ..v("det-limit=736") },
        Variant { limit: Some(640), ..v("det-limit=640") },
        Variant { limit: Some(512), ..v("det-limit=512") },
        Variant { limit: Some(1280), ..v("det-limit=1280 (full res)") },
        #[cfg(feature = "directml")]
        Variant { directml: true, explicit: true, ..v("directml (det+rec on GPU)") },
        #[cfg(feature = "directml")]
        Variant { directml: true, explicit: true, rec_batch: Some(8), ..v("directml rec-batch=8") },
        #[cfg(feature = "directml")]
        Variant { directml: true, explicit: true, rec_batch: Some(4), ..v("directml rec-batch=4") },
        #[cfg(feature = "directml")]
        Variant { directml: true, explicit: true, rec_batch: Some(1), ..v("directml rec-batch=1") },
    ]
}

fn build(models: &Path, v: &Variant) -> anyhow::Result<oar_ocr::oarocr::OAROCR> {
    let mut b = OAROCRBuilder::new(models.join("PP-OCRv6_det_small.onnx"), models.join("korean_PP-OCRv5_rec_mobile.onnx"), models.join("korean_PP-OCRv5_dict.txt"));
    if v.explicit || v.directml || v.intra.is_some() || v.opt.is_some() || v.mem.is_some() || v.parallel.is_some() {
        let mut c = OrtSessionConfig::new();
        if let Some(n) = v.intra {
            c = c.with_intra_threads(n);
        }
        if let Some(n) = v.inter {
            c = c.with_inter_threads(n);
        }
        if let Some(p) = v.parallel {
            c = c.with_parallel_execution(p);
        }
        if let Some(o) = v.opt {
            c = c.with_optimization_level(o);
        }
        if let Some(m) = v.mem {
            c = c.with_memory_pattern(m);
        }
        #[cfg(feature = "directml")]
        if v.directml {
            c = c.with_execution_providers(vec![oar_ocr::core::config::OrtExecutionProvider::DirectML { device_id: Some(0) }, oar_ocr::core::config::OrtExecutionProvider::CPU]).with_parallel_execution(false).with_memory_pattern(false);
        }
        b = b.ort_session(c);
    }
    if let Some(n) = v.rec_batch {
        b = b.region_batch_size(n);
    }
    if let Some(l) = v.limit {
        // an explicit detection config replaces the library's general-text defaults: restate them
        b = b.text_detection_config(TextDetectionConfig { score_threshold: 0.3, box_threshold: 0.6, unclip_ratio: 2.0, max_candidates: 1000, limit_side_len: Some(l), limit_type: Some(LimitType::Max), max_side_len: Some(4000) });
    }
    b.build().map_err(|e| anyhow::anyhow!("{e}"))
}

fn run(ocr: &oar_ocr::oarocr::OAROCR, img: &RgbImage) -> Vec<TextBox> {
    let res = ocr.predict(vec![img.clone()]).expect("predict");
    let mut out = Vec::new();
    for r in res.iter().flat_map(|p| p.text_regions.iter()) {
        if let Some(text) = &r.text {
            let b = &r.bounding_box;
            out.push(TextBox { x0: b.x_min(), y0: b.y_min(), x1: b.x_max(), y1: b.y_max(), text: text.to_string(), score: r.confidence.unwrap_or(0.0) });
        }
    }
    out
}

struct Expected {
    x0: f32,
    y0: f32,
    text: String,
}

fn read_expected(p: &Path) -> Vec<Expected> {
    std::fs::read_to_string(p)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| {
            let (b, t) = l.split_once('\t')?;
            let n: Vec<f32> = b.split(',').filter_map(|s| s.parse().ok()).collect();
            (n.len() == 4).then(|| Expected { x0: n[0], y0: n[1], text: t.to_string() })
        })
        .collect()
}

/// (matched expected boxes, expected total, extra boxes)
fn compare(got: &[TextBox], want: &[Expected]) -> (usize, usize, usize) {
    let mut used = vec![false; got.len()];
    let mut hit = 0;
    for w in want {
        if let Some(i) = (0..got.len()).find(|&i| !used[i] && got[i].text == w.text && (got[i].x0 - w.x0).abs() <= 3.0 && (got[i].y0 - w.y0).abs() <= 3.0) {
            used[i] = true;
            hit += 1;
        }
    }
    (hit, want.len(), got.len() - used.iter().filter(|u| **u).count())
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.total_cmp(b));
    v[v.len() / 2]
}

fn main() -> anyhow::Result<()> {
    let models = std::env::var("BAOCR_MODELS").map(PathBuf::from).unwrap_or_else(|_| baocr_ocr::PpOcr::default_models_dir());
    let runs: usize = std::env::var("RUNS").ok().and_then(|s| s.parse().ok()).unwrap_or(3);
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../fixtures/screens");
    let mut frames: Vec<(String, RgbImage, Vec<Expected>)> = Vec::new();
    let mut names: Vec<_> = std::fs::read_dir(&dir)?.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "png")).collect();
    names.sort();
    for p in names {
        let boxes = p.with_extension("boxes.txt");
        if boxes.exists() {
            let name = p.file_stem().unwrap().to_string_lossy().to_string();
            frames.push((name, image::open(&p)?.to_rgb8(), read_expected(&boxes)));
        }
    }
    println!("{} fixture frames with committed boxes; {runs} timed runs each (median)", frames.len());
    let filters: Vec<String> = std::env::args().skip(1).collect();
    let mut reference: Option<Vec<Vec<TextBox>>> = None;
    println!("{:<30} {:>8} {:>9} {:>9} {:>9}   {:>14} {:>14}   per-frame ms (first 6)", "variant", "build ms", "sum ms", "quick-f", "lobby", "vs committed", "vs baseline");
    for v in variants() {
        if !filters.is_empty() && v.name != "baseline (library defaults)" && !filters.iter().any(|f| v.name.contains(f.as_str())) {
            continue;
        }
        let t = Instant::now();
        let ocr = build(&models, &v)?;
        let build_ms = t.elapsed().as_secs_f64() * 1000.0;
        let mut outs = Vec::new();
        let mut times = Vec::new();
        let mut cold = Vec::new();
        for (_, img, _) in &frames {
            let t = Instant::now();
            outs.push(run(&ocr, img)); // warm-up and the output that is judged
            cold.push(t.elapsed().as_secs_f64() * 1000.0);
            times.push(median((0..runs).map(|_| {
                let t = Instant::now();
                std::hint::black_box(run(&ocr, img));
                t.elapsed().as_secs_f64() * 1000.0
            }).collect()));
        }
        let (mut hit, mut total, mut extra, mut same) = (0, 0, 0, 0);
        for (o, (_, _, want)) in outs.iter().zip(&frames) {
            let (h, t, e) = compare(o, want);
            (hit, total, extra) = (hit + h, total + t, extra + e);
            same += (h == t && e == 0) as usize;
        }
        let vs_base = match &reference {
            None => "-".to_string(),
            Some(r) => {
                let (mut miss, mut ex, mut ident) = (0, 0, 0);
                for (o, b) in outs.iter().zip(r) {
                    let want: Vec<Expected> = b.iter().map(|t| Expected { x0: t.x0, y0: t.y0, text: t.text.clone() }).collect();
                    let (h, t, e) = compare(o, &want);
                    miss += t - h;
                    ex += e;
                    ident += (h == t && e == 0) as usize;
                }
                format!("{ident}/{} ident -{miss} +{ex}", frames.len())
            }
        };
        if reference.is_none() {
            reference = Some(outs.clone());
        }
        let at = |n: &str| frames.iter().position(|f| f.0 == n).map(|i| times[i]).unwrap_or(f64::NAN);
        let firsts: Vec<String> = times.iter().take(6).map(|t| format!("{t:.0}")).collect();
        println!(
            "{:<30} {:>8.0} {:>9.0} {:>9.0} {:>9.0}   {:>14} {:>14}   {}",
            v.name,
            build_ms,
            times.iter().sum::<f64>(),
            at("quick-formation"),
            at("lobby-idle"),
            format!("{same}/{} ident -{}+{extra}", frames.len(), total - hit),
            vs_base,
            firsts.join(" ")
        );
        println!("{:<30} first (cold) run per frame, ms: {}   (sum {:.0})", "", cold.iter().map(|t| format!("{t:.0}")).collect::<Vec<_>>().join(" "), cold.iter().sum::<f64>());
    }
    Ok(())
}
