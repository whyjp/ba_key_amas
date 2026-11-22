//! PP-OCR engine, in process.
//!
//! * `boxes`  : detection + recognition on a whole frame (oar-ocr).
//! * `line`   : recognition only on a pre-cropped text line (greedy CTC).
//! * `choose` : closed-set reading. Every candidate string is scored against the
//!   recognition model's per-timestep probabilities with the CTC forward
//!   algorithm and the most probable candidate wins. With a dictionary of
//!   student names this cannot return text that is not a student, and short
//!   names are decided by likelihood instead of by a lossy text read.
use anyhow::{anyhow, Result};
use baocr_core::ocr_api::{Ocr, TextBox};
use image::{imageops::FilterType, RgbImage};
use oar_ocr::oarocr::{OAROCRBuilder, OAROCR};
use ort::session::Session;
use ort::value::Tensor;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const REC_HEIGHT: u32 = 48;
const MIN_WIDTH: u32 = 16;
const MAX_WIDTH: u32 = 1600;

pub struct PpOcr {
    det_rec: OAROCR,
    rec: Mutex<Session>,
    /// class index -> character (index 0 is the CTC blank).
    classes: Vec<String>,
    char_to_class: HashMap<char, usize>,
    /// Hangul syllables the model knows, with their jamo, for spelling unknown ones.
    syllables: Vec<(char, [u8; 3])>,
}

impl PpOcr {
    /// Load `PP-OCRv6_det_small.onnx`, `korean_PP-OCRv5_rec_mobile.onnx` and
    /// `korean_PP-OCRv5_dict.txt` from `models`.
    pub fn new(models: &Path) -> Result<Self> {
        let det = models.join("PP-OCRv6_det_small.onnx");
        let rec = models.join("korean_PP-OCRv5_rec_mobile.onnx");
        let dict = models.join("korean_PP-OCRv5_dict.txt");
        for p in [&det, &rec, &dict] {
            if !p.exists() {
                return Err(anyhow!("model file missing: {} (see models/README.md)", p.display()));
            }
        }
        let det_rec = OAROCRBuilder::new(det, rec.clone(), dict.clone())
            .build()
            .map_err(|e| anyhow!("building detection+recognition pipeline: {e}"))?;
        let session = oar_ocr::core::load_session(rec.as_path()).map_err(|e| anyhow!("loading recognition model: {e}"))?;
        let text = std::fs::read_to_string(&dict)?;
        // PaddleOCR layout: blank, every dictionary line, then the space char.
        let mut classes = vec![String::new()];
        classes.extend(text.lines().map(|l| l.to_string()));
        classes.push(" ".to_string());
        let mut char_to_class = HashMap::new();
        for (i, c) in classes.iter().enumerate().skip(1) {
            let mut it = c.chars();
            if let (Some(ch), None) = (it.next(), it.next()) {
                char_to_class.entry(ch).or_insert(i);
            }
        }
        let syllables = char_to_class.keys().filter_map(|&c| jamo(c).map(|j| (c, j))).collect();
        Ok(Self { det_rec, rec: Mutex::new(session), classes, char_to_class, syllables })
    }

    /// Default models dir: $BAOCR_MODELS, else ./models, else ../models.
    pub fn default_models_dir() -> PathBuf {
        if let Ok(p) = std::env::var("BAOCR_MODELS") {
            return PathBuf::from(p);
        }
        for c in ["models", "../models", "../../models"] {
            if Path::new(c).join("korean_PP-OCRv5_rec_mobile.onnx").exists() {
                return PathBuf::from(c);
            }
        }
        PathBuf::from("models")
    }

    /// Per-timestep class probabilities of a text line: (timesteps, classes, row-major data).
    pub fn probs(&self, img: &RgbImage) -> Result<(usize, usize, Vec<f32>)> {
        let (w0, h0) = img.dimensions();
        if w0 == 0 || h0 == 0 {
            return Err(anyhow!("empty line image"));
        }
        let w = ((REC_HEIGHT as f32 * w0 as f32 / h0 as f32).ceil() as u32).clamp(MIN_WIDTH, MAX_WIDTH);
        let resized = image::imageops::resize(img, w, REC_HEIGHT, FilterType::CatmullRom);
        // CHW, BGR, (x/255 - 0.5)/0.5 - what the PaddleOCR recognisers were trained with.
        let (w, h) = (w as usize, REC_HEIGHT as usize);
        let mut data = vec![0f32; 3 * h * w];
        for y in 0..h {
            for x in 0..w {
                let p = resized.get_pixel(x as u32, y as u32);
                for (c, v) in [p[2], p[1], p[0]].into_iter().enumerate() {
                    data[c * h * w + y * w + x] = (v as f32 / 255.0 - 0.5) / 0.5;
                }
            }
        }
        let input = Tensor::from_array(([1usize, 3, h, w], data))?;
        let mut session = self.rec.lock().map_err(|_| anyhow!("recognition session poisoned"))?;
        let outputs = session.run(ort::inputs![input])?;
        let (shape, out) = outputs[0].try_extract_tensor::<f32>()?;
        let (t, c) = (shape[1] as usize, shape[2] as usize);
        let mut probs = out.to_vec();
        // Some exports already softmax; normalise only when a row is not a distribution.
        for row in probs.chunks_mut(c) {
            let sum: f32 = row.iter().sum();
            if !(0.98..=1.02).contains(&sum) {
                let m = row.iter().cloned().fold(f32::MIN, f32::max);
                let mut s = 0.0;
                for v in row.iter_mut() {
                    *v = (*v - m).exp();
                    s += *v;
                }
                for v in row.iter_mut() {
                    *v /= s;
                }
            }
        }
        Ok((t, c, probs))
    }

    fn decode(&self, t: usize, c: usize, probs: &[f32]) -> (String, f32) {
        let (mut text, mut last, mut confs) = (String::new(), 0usize, Vec::new());
        for step in 0..t {
            let row = &probs[step * c..(step + 1) * c];
            let (best, p) = row
                .iter()
                .enumerate()
                .fold((0usize, f32::MIN), |a, (i, &v)| if v > a.1 { (i, v) } else { a });
            if best != 0 && best != last {
                text.push_str(&self.classes[best]);
                confs.push(p);
            }
            last = best;
        }
        let score = if confs.is_empty() { 0.0 } else { confs.iter().sum::<f32>() / confs.len() as f32 };
        (text, score)
    }

    /// Spellings of `cand` as class-id sequences, with a flag for "uses a
    /// substitute". A syllable the model cannot spell (rare names such as "슌"
    /// are outside its 11.9k vocabulary) is replaced by each known syllable one
    /// jamo away; the caller keeps whichever spelling the model finds likeliest.
    fn spellings(&self, cand: &str) -> Vec<(Vec<usize>, bool)> {
        const MAX_ALTS: usize = 64;
        let mut seqs: Vec<(Vec<usize>, bool)> = vec![(Vec::new(), false)];
        for ch in cand.chars() {
            let options: Vec<(usize, bool)> = if let Some(&c) = self.char_to_class.get(&ch) {
                vec![(c, false)]
            } else if let Some(j) = jamo(ch) {
                let near: Vec<(usize, bool)> = self
                    .syllables
                    .iter()
                    .filter(|(_, cj)| j.iter().zip(cj).filter(|(a, b)| a != b).count() == 1)
                    .map(|(c, _)| (self.char_to_class[c], true))
                    .collect();
                if near.is_empty() {
                    return Vec::new();
                }
                near
            } else {
                return Vec::new();
            };
            let mut next = Vec::new();
            for (seq, sub) in &seqs {
                for &(c, s) in &options {
                    if next.len() >= MAX_ALTS {
                        break;
                    }
                    let mut q = seq.clone();
                    q.push(c);
                    next.push((q, *sub || s));
                }
            }
            seqs = next;
        }
        seqs.retain(|(s, _)| !s.is_empty());
        seqs
    }

    /// log P(label sequence | probs) by the CTC forward algorithm.
    fn ctc_log_prob(t: usize, c: usize, probs: &[f32], labels: &[usize]) -> f32 {
        if labels.is_empty() || t == 0 {
            return f32::NEG_INFINITY;
        }
        // blank-interleaved extended sequence: _ l1 _ l2 _ ... _
        let ext: Vec<usize> = std::iter::once(0).chain(labels.iter().flat_map(|&l| [l, 0])).collect();
        let n = ext.len();
        let lse = |a: f32, b: f32| {
            if a == f32::NEG_INFINITY {
                b
            } else if b == f32::NEG_INFINITY {
                a
            } else {
                let m = a.max(b);
                m + ((a - m).exp() + (b - m).exp()).ln()
            }
        };
        let lp = |step: usize, k: usize| probs[step * c + k].max(1e-12).ln();
        let mut alpha = vec![f32::NEG_INFINITY; n];
        alpha[0] = lp(0, ext[0]);
        if n > 1 {
            alpha[1] = lp(0, ext[1]);
        }
        for step in 1..t {
            let mut next = vec![f32::NEG_INFINITY; n];
            for s in 0..n {
                let mut a = alpha[s];
                if s >= 1 {
                    a = lse(a, alpha[s - 1]);
                }
                if s >= 2 && ext[s] != 0 && ext[s] != ext[s - 2] {
                    a = lse(a, alpha[s - 2]);
                }
                next[s] = a + lp(step, ext[s]);
            }
            alpha = next;
        }
        lse(alpha[n - 1], if n > 1 { alpha[n - 2] } else { f32::NEG_INFINITY })
    }
}

impl Ocr for PpOcr {
    fn boxes(&self, img: &RgbImage) -> Result<Vec<TextBox>> {
        let res = self.det_rec.predict(vec![img.clone()]).map_err(|e| anyhow!("OCR: {e}"))?;
        let mut out = Vec::new();
        for r in res.iter().flat_map(|p| p.text_regions.iter()) {
            if let Some(text) = &r.text {
                let b = &r.bounding_box;
                out.push(TextBox {
                    x0: b.x_min(),
                    y0: b.y_min(),
                    x1: b.x_max(),
                    y1: b.y_max(),
                    text: text.to_string(),
                    score: r.confidence.unwrap_or(0.0),
                });
            }
        }
        Ok(out)
    }

    fn line(&self, img: &RgbImage) -> Result<(String, f32)> {
        let (t, c, probs) = self.probs(img)?;
        Ok(self.decode(t, c, &probs))
    }

    fn choose(&self, img: &RgbImage, candidates: &[String]) -> Result<(usize, Vec<f32>)> {
        let (t, c, probs) = self.probs(img)?;
        let mut scores = Vec::with_capacity(candidates.len());
        for cand in candidates {
            // Substituted spellings pay a small penalty so a real spelling wins ties.
            let score = self
                .spellings(cand)
                .iter()
                .map(|(l, sub)| Self::ctc_log_prob(t, c, &probs, l) - if *sub { 0.5 } else { 0.0 })
                .fold(f32::NEG_INFINITY, f32::max);
            scores.push(score); // NEG_INFINITY: a candidate the model cannot spell is never chosen
        }
        let best = scores
            .iter()
            .enumerate()
            .fold((0usize, f32::NEG_INFINITY), |a, (i, &v)| if v > a.1 { (i, v) } else { a })
            .0;
        Ok((best, scores))
    }
}

/// (initial, vowel, final) indices of a precomposed Hangul syllable.
fn jamo(c: char) -> Option<[u8; 3]> {
    let v = c as u32;
    if !(0xAC00..=0xD7A3).contains(&v) {
        return None;
    }
    let s = v - 0xAC00;
    Some([(s / 588) as u8, ((s % 588) / 28) as u8, (s % 28) as u8])
}

/// Turn an ink-isolated image (black glyphs on white) into a tight text line:
/// specks are removed and the page is cropped to the glyphs plus a margin, so
/// the recogniser sees one clean line and every timestep is either text or blank.
pub fn ink_to_line(g: &image::GrayImage) -> RgbImage {
    const MIN_AREA: usize = 10;
    const MARGIN: i64 = 8;
    let (w, h) = (g.width() as i64, g.height() as i64);
    let ink = |x: i64, y: i64| x >= 0 && y >= 0 && x < w && y < h && g.get_pixel(x as u32, y as u32)[0] < 128;
    let mut seen = vec![false; (w * h) as usize];
    let mut keep: Vec<(i64, i64)> = Vec::new();
    for y0 in 0..h {
        for x0 in 0..w {
            if !ink(x0, y0) || seen[(y0 * w + x0) as usize] {
                continue;
            }
            let (mut stack, mut comp) = (vec![(x0, y0)], Vec::new());
            seen[(y0 * w + x0) as usize] = true;
            while let Some((x, y)) = stack.pop() {
                comp.push((x, y));
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        let (nx, ny) = (x + dx, y + dy);
                        if ink(nx, ny) && !seen[(ny * w + nx) as usize] {
                            seen[(ny * w + nx) as usize] = true;
                            stack.push((nx, ny));
                        }
                    }
                }
            }
            if comp.len() >= MIN_AREA {
                keep.extend(comp);
            }
        }
    }
    if keep.is_empty() {
        return RgbImage::from_pixel(32, 48, image::Rgb([255, 255, 255]));
    }
    let (x0, x1) = (keep.iter().map(|p| p.0).min().unwrap(), keep.iter().map(|p| p.0).max().unwrap());
    let (y0, y1) = (keep.iter().map(|p| p.1).min().unwrap(), keep.iter().map(|p| p.1).max().unwrap());
    let mut out = RgbImage::from_pixel((x1 - x0 + 1 + 2 * MARGIN) as u32, (y1 - y0 + 1 + 2 * MARGIN) as u32, image::Rgb([255, 255, 255]));
    for (x, y) in keep {
        out.put_pixel((x - x0 + MARGIN) as u32, (y - y0 + MARGIN) as u32, image::Rgb([0, 0, 0]));
    }
    out
}
