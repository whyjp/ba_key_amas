//! Does screen recognition survive a game window of another size? Every fixture screen is shrunk or
//! enlarged as a window of that size would show it, then brought back to the 1280x960 frame the way the
//! capture does, and read again. The scene found must equal the scene found at the original size.
//!
//!     scale_check [fixtures/screens]
//!
//! It checks size only. Another aspect ratio (16:9) changes the game's own layout, which no resize can
//! imitate: that needs screenshots of the game in that mode.
use baocr_core::ocr_api::Ocr;
use baocr_extract::scene::classify;
use baocr_ocr::PpOcr;
use image::{imageops, imageops::FilterType, RgbImage};

const SCALES: [f64; 5] = [0.5, 0.67, 0.8, 1.25, 1.5];

fn window_view(img: &RgbImage, s: f64) -> RgbImage {
    let (w, h) = ((1280.0 * s).round() as u32, (960.0 * s).round() as u32);
    let window = imageops::resize(img, w, h, FilterType::Lanczos3);
    imageops::resize(&window, 1280, 960, FilterType::CatmullRom)
}

fn main() -> anyhow::Result<()> {
    let dir = std::env::args().nth(1).unwrap_or_else(|| "fixtures/screens".into());
    let ocr = PpOcr::new(&PpOcr::default_models_dir())?;
    let mut names: Vec<_> = std::fs::read_dir(&dir)?.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "png")).collect();
    names.sort();
    let (mut total, mut same) = (0, vec![0; SCALES.len()]);
    for p in names {
        let img = image::open(&p)?.to_rgb8();
        if img.dimensions() != (1280, 960) {
            continue;
        }
        let base = classify(&ocr.boxes(&img)?, None);
        let mut row = String::new();
        for (i, s) in SCALES.iter().enumerate() {
            let got = classify(&ocr.boxes(&window_view(&img, *s))?, None);
            if got == base {
                same[i] += 1;
            } else {
                row.push_str(&format!(" x{s}:{got:?}"));
            }
        }
        total += 1;
        println!("{:34} {:?}{}", p.file_stem().unwrap().to_string_lossy(), base, if row.is_empty() { String::new() } else { format!("  DIFFERS{row}") });
    }
    println!("\nscreens {total}");
    for (s, n) in SCALES.iter().zip(&same) {
        println!("window scale {s:<5} same scene as at full size: {n} of {total}");
    }
    Ok(())
}
