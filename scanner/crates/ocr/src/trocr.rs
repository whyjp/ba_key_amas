//! TrOCR (`team-lucid/trocr-small-korean`) reading a student's name label, scored against the closed list of names.
//!
//! The recipe and its numbers come from the OCR research (`SPEC.md` of that work): a label is cropped to its text, enlarged
//! 4x with OpenCV's Lanczos4 and inverted, padded white, squeezed to 384x384 with torchvision's antialiased bicubic, and
//! run through the encoder once; every candidate name is then scored by teacher forcing on the decoder. The two resamplers
//! are integer ports that reproduce OpenCV and torchvision to the bit, so the model sees what it was measured on.
use anyhow::{anyhow, Result};
use image::RgbImage;
use ort::{session::Session, value::Tensor};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

/// Side of the model's square input.
pub const SIDE: usize = 384;
const START: i64 = 2;
const EOS: i64 = 2;
const PAD: i64 = 1;
const VOCAB: usize = 50265;
/// Floats in one image's cross-attention keys (or values): 6 layers x 8 heads x 578 tokens x 32.
const CROSS_LEN: usize = 6 * 8 * 578 * 32;
/// Rows scored at once (the decoder's output is `rows x length x 50265` floats).
const BATCH: usize = 64;
/// Magnification of the label before it is squeezed to the model's input, and the white border around it.
const UPSCALE: usize = 4;
const BORDER: usize = 20 * UPSCALE / 3 + 4;

/// What the encoder says about one label: the decoder's cross-attention keys and values.
pub struct Cross {
    k: Vec<f32>,
    v: Vec<f32>,
}

fn gray(p: &[u8; 3]) -> i32 {
    // OpenCV 5's integer BGR2GRAY, with p = [R, G, B]
    (3735 * p[2] as i32 + 19235 * p[1] as i32 + 9798 * p[0] as i32 + 16384) >> 15
}

/// The label's text box (x0, y0, x1, y1), half open: bright pixels away from the label's rim, with a margin of 3 px.
pub fn tight_box(label: &RgbImage) -> (u32, u32, u32, u32) {
    let (w, h) = label.dimensions();
    let (mut rmin, mut rmax, mut cmin, mut cmax, mut n) = (u32::MAX, 0, u32::MAX, 0, 0);
    for r in 4..h.saturating_sub(4) {
        for c in 8..w.saturating_sub(8) {
            if gray(&label.get_pixel(c, r).0) > 175 {
                n += 1;
                (rmin, rmax, cmin, cmax) = (rmin.min(r), rmax.max(r), cmin.min(c), cmax.max(c));
            }
        }
    }
    if n < 5 {
        return (0, 0, w, h);
    }
    (cmin.saturating_sub(3), rmin.saturating_sub(3), (cmax + 4).min(w), (rmax + 4).min(h))
}

/// Lanczos4 weights (sum 2048) by output index mod 4, for taps `s-3 ..= s+4`.
const LANCZOS4: [[i64; 8]; 4] = [[-17, 98, -277, 923, 1580, -357, 130, -32], [-2, 30, -90, 263, 1992, -194, 70, -21], [-21, 70, -194, 1992, 263, -90, 30, -2], [-32, 130, -357, 1580, 923, -277, 98, -17]];

/// The taps of output index `d` of an axis of `n` source pixels: indices (edge-clamped) and weights.
fn lanczos_taps(d: usize, n: usize) -> ([usize; 8], &'static [i64; 8]) {
    let s = if d % 4 < 2 { (d / 4) as i64 - 1 } else { (d / 4) as i64 };
    (std::array::from_fn(|i| (s - 3 + i as i64).clamp(0, n as i64 - 1) as usize), &LANCZOS4[d % 4])
}

/// `cv2.resize(fx=4, fy=4, INTER_LANCZOS4)` on 8-bit pixels, bit for bit.
fn lanczos_x4(src: &RgbImage) -> RgbImage {
    let (w, h) = (src.width() as usize, src.height() as usize);
    let (ow, oh) = (w * UPSCALE, h * UPSCALE);
    let raw = src.as_raw();
    // horizontal pass keeps the exact integer sums
    let mut horiz = vec![0i64; h * ow * 3];
    for y in 0..h {
        for dx in 0..ow {
            let (idx, wt) = lanczos_taps(dx, w);
            for c in 0..3 {
                horiz[(y * ow + dx) * 3 + c] = (0..8).map(|i| wt[i] * raw[(y * w + idx[i]) * 3 + c] as i64).sum();
            }
        }
    }
    let mut out = RgbImage::new(ow as u32, oh as u32);
    for dy in 0..oh {
        let (idx, wt) = lanczos_taps(dy, h);
        for x in 0..ow {
            for c in 0..3 {
                let v: i64 = (0..8).map(|i| wt[i] * horiz[(idx[i] * ow + x) * 3 + c]).sum();
                out.get_pixel_mut(x as u32, dy as u32).0[c] = ((v + (1 << 21)) >> 22).clamp(0, 255) as u8;
            }
        }
    }
    out
}

fn cubic(x: f64) -> f64 {
    const A: f64 = -0.5;
    let x = x.abs();
    if x < 1.0 {
        ((A + 2.0) * x - (A + 3.0)) * x * x + 1.0
    } else if x < 2.0 {
        ((A * x - 5.0 * A) * x + 8.0 * A) * x - 4.0 * A
    } else {
        0.0
    }
}

/// Per output index the first source index and the integer weights, and the shared precision in bits
/// (torchvision's antialiased bicubic for 8-bit images).
fn bicubic_weights(n_in: usize, n_out: usize) -> (Vec<(usize, Vec<i64>)>, u32) {
    let scale = n_in as f64 / n_out as f64;
    let support = if scale >= 1.0 { 2.0 * scale } else { 2.0 };
    let invscale = if scale >= 1.0 { 1.0 / scale } else { 1.0 };
    let rows: Vec<(usize, Vec<f64>)> = (0..n_out)
        .map(|i| {
            let center = scale * (i as f64 + 0.5);
            let xmin = ((center - support + 0.5) as i64).max(0) as usize;
            let xsize = (((center + support + 0.5) as i64).min(n_in as i64) as usize).saturating_sub(xmin);
            let w: Vec<f64> = (0..xsize).map(|j| cubic((j as f64 + xmin as f64 - center + 0.5) * invscale)).collect();
            let total: f64 = w.iter().sum();
            (xmin, w.into_iter().map(|v| if total != 0.0 { v / total } else { v }).collect())
        })
        .collect();
    let max_w = rows.iter().flat_map(|(_, w)| w.iter().copied()).fold(f64::MIN, f64::max);
    let mut prec = 0u32;
    while prec < 22 && ((0.5 + max_w * (1u64 << (prec + 1)) as f64) as i64) < (1 << 15) {
        prec += 1;
    }
    let scaled = |v: f64| if v < 0.0 { (-0.5 + v * (1u64 << prec) as f64) as i64 } else { (0.5 + v * (1u64 << prec) as f64) as i64 };
    (rows.into_iter().map(|(x0, w)| (x0, w.into_iter().map(scaled).collect())).collect(), prec)
}

/// One pass of the resample along x (`horizontal`) or y over an 8-bit RGB image.
fn bicubic_pass(src: &RgbImage, n_out: usize, horizontal: bool) -> RgbImage {
    let (w, h) = (src.width() as usize, src.height() as usize);
    let (weights, prec) = bicubic_weights(if horizontal { w } else { h }, n_out);
    let (ow, oh) = if horizontal { (n_out, h) } else { (w, n_out) };
    let mut out = RgbImage::new(ow as u32, oh as u32);
    for y in 0..oh {
        for x in 0..ow {
            let (x0, wt) = &weights[if horizontal { x } else { y }];
            for c in 0..3 {
                let sum: i64 = wt
                    .iter()
                    .enumerate()
                    .map(|(j, k)| {
                        let (sx, sy) = if horizontal { (x0 + j, y) } else { (x, x0 + j) };
                        k * src.get_pixel(sx as u32, sy as u32).0[c] as i64
                    })
                    .sum();
                out.get_pixel_mut(x as u32, y as u32).0[c] = ((sum + (1 << (prec - 1))) >> prec).clamp(0, 255) as u8;
            }
        }
    }
    out
}

/// The 384x384 image the encoder is given, from a label crop (RGB): crop to the text, 4x Lanczos, invert, white border,
/// squeeze (the aspect ratio is not kept).
pub fn square_input(label: &RgbImage) -> RgbImage {
    let (x0, y0, x1, y1) = tight_box(label);
    let tight = image::imageops::crop_imm(label, x0, y0, x1 - x0, y1 - y0).to_image();
    let mut big = lanczos_x4(&tight);
    for p in big.pixels_mut() {
        p.0 = p.0.map(|v| 255 - v);
    }
    let mut canvas = RgbImage::from_pixel(big.width() + 2 * BORDER as u32, big.height() + 2 * BORDER as u32, image::Rgb([255, 255, 255]));
    image::imageops::replace(&mut canvas, &big, BORDER as i64, BORDER as i64);
    let mut img = canvas;
    if img.width() as usize != SIDE {
        img = bicubic_pass(&img, SIDE, true);
    }
    if img.height() as usize != SIDE {
        img = bicubic_pass(&img, SIDE, false);
    }
    img
}

/// `pixel_values`: NCHW floats, RGB, `(v - 127.5) / 127.5`.
pub fn pixel_values(square: &RgbImage) -> Vec<f32> {
    let mut data = vec![0f32; 3 * SIDE * SIDE];
    for (x, y, p) in square.enumerate_pixels() {
        for c in 0..3 {
            data[c * SIDE * SIDE + y as usize * SIDE + x as usize] = (p.0[c] as f32 - 127.5) / 127.5;
        }
    }
    data
}

/// The written forms of every name, as token ids (`scripts/trocr_seed.py`).
fn parse_forms(json: &str) -> Result<HashMap<String, Vec<Vec<i64>>>> {
    serde_json::from_str(json).map_err(|e| anyhow!("trocr_surfaces.json: {e}"))
}

pub struct TrOcr {
    encoder: Mutex<Session>,
    decoder: Mutex<Session>,
    forms: HashMap<String, Vec<Vec<i64>>>,
}

impl TrOcr {
    /// Load `encoder.onnx` and `decoder.onnx` (the split pair of the fp16w variant) from `dir`.
    pub fn new(dir: &Path) -> Result<Self> {
        let load = |name: &str| -> Result<Session> {
            let p = dir.join(name);
            if !p.exists() {
                return Err(anyhow!("model file missing: {}", p.display()));
            }
            oar_ocr::core::load_session(p.as_path()).map_err(|e| anyhow!("loading {name}: {e}"))
        };
        Ok(Self { encoder: Mutex::new(load("encoder.onnx")?), decoder: Mutex::new(load("decoder.onnx")?), forms: parse_forms(include_str!("../data/trocr_surfaces.json"))? })
    }

    /// Encode a 384x384 input (see [`pixel_values`]).
    pub fn encode(&self, pixel_values: Vec<f32>) -> Result<Cross> {
        let input = Tensor::from_array(([1usize, 3, SIDE, SIDE], pixel_values))?;
        let mut session = self.encoder.lock().map_err(|_| anyhow!("encoder session poisoned"))?;
        let outputs = session.run(ort::inputs!["pixel_values" => input])?;
        let (_, k) = outputs["cross_k"].try_extract_tensor::<f32>()?;
        let (_, v) = outputs["cross_v"].try_extract_tensor::<f32>()?;
        if k.len() != CROSS_LEN || v.len() != CROSS_LEN {
            return Err(anyhow!("encoder output has {} / {} values, expected {CROSS_LEN}", k.len(), v.len()));
        }
        Ok(Cross { k: k.to_vec(), v: v.to_vec() })
    }

    /// Log-probability of each token sequence (token ids without the start and end marks) as the text of the
    /// encoded image: (sum over the tokens and the end mark, how many of them).
    pub fn score_tokens(&self, cross: &Cross, seqs: &[&[i64]]) -> Result<Vec<(f32, usize)>> {
        let mut order: Vec<usize> = (0..seqs.len()).collect();
        order.sort_by_key(|&i| seqs[i].len());
        let mut out = vec![(0.0f32, 0usize); seqs.len()];
        for chunk in order.chunks(BATCH) {
            let len = chunk.iter().map(|&i| seqs[i].len() + 1).max().unwrap_or(1);
            let mut ids = vec![PAD; chunk.len() * len];
            for (row, &i) in chunk.iter().enumerate() {
                ids[row * len] = START;
                for (k, t) in seqs[i].iter().enumerate() {
                    ids[row * len + 1 + k] = *t;
                }
            }
            let ids_t = Tensor::from_array(([chunk.len(), len], ids))?;
            let k_t = Tensor::from_array(([6usize, 1, 8, 578, 32], cross.k.clone()))?;
            let v_t = Tensor::from_array(([6usize, 1, 8, 578, 32], cross.v.clone()))?;
            let mut session = self.decoder.lock().map_err(|_| anyhow!("decoder session poisoned"))?;
            let outputs = session.run(ort::inputs!["input_ids" => ids_t, "cross_k" => k_t, "cross_v" => v_t])?;
            let (_, lp) = outputs["log_probs"].try_extract_tensor::<f32>()?;
            for (row, &i) in chunk.iter().enumerate() {
                let t = seqs[i];
                let n = t.len() + 1;
                let at = |k: usize, target: i64| lp[(row * len + k) * VOCAB + target as usize];
                let sum: f32 = (0..n).map(|k| at(k, if k < t.len() { t[k] } else { EOS })).sum();
                out[i] = (sum, n);
            }
        }
        Ok(out)
    }

    /// Score every name as the text of a label crop: the mean log-probability per token of the best of the name's
    /// written forms (`NEG_INFINITY` for a name the seed lacks). Higher is better.
    pub fn choose(&self, label: &RgbImage, names: &[String]) -> Result<Vec<f32>> {
        let cross = self.encode(pixel_values(&square_input(label)))?;
        let mut seqs: Vec<&[i64]> = Vec::new();
        let mut owner = Vec::new();
        for (k, name) in names.iter().enumerate() {
            for form in self.forms.get(name).into_iter().flatten() {
                seqs.push(form);
                owner.push(k);
            }
        }
        let scores = self.score_tokens(&cross, &seqs)?;
        let mut best = vec![f32::NEG_INFINITY; names.len()];
        for ((sum, n), k) in scores.into_iter().zip(owner) {
            best[k] = best[k].max(sum / n as f32);
        }
        Ok(best)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lanczos_weights_sum_to_one() {
        assert!(LANCZOS4.iter().all(|w| w.iter().sum::<i64>() == 2048));
    }

    #[test]
    fn a_flat_label_stays_flat_through_the_squeeze() {
        let img = square_input(&RgbImage::from_pixel(94, 35, image::Rgb([67, 82, 102])));
        assert_eq!(img.dimensions(), (SIDE as u32, SIDE as u32));
        // no text: the whole label is used, inverted, bordered in white
        let mid = img.get_pixel(192, 192).0;
        assert!(mid.iter().zip([188, 173, 153]).all(|(a, b)| (*a as i32 - b).abs() <= 2), "{mid:?}");
    }

    #[test]
    fn the_seed_covers_five_forms_for_a_bracketed_name() {
        let forms = parse_forms(include_str!("../data/trocr_surfaces.json")).unwrap();
        assert_eq!(forms.len(), 280);
        assert_eq!(forms["아루(드레스)"].len(), 5);
        assert_eq!(forms["아루"].len(), 1);
    }
}
