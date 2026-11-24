//! How well names survive a smaller game window: shrink saved frames by a factor, scale back
//! to 1280x960 (what a small window's capture looks like) and compare with the full-size reading.
use baocr_core::ocr_api::Ocr;
use baocr_extract::read_card_names;
use baocr_ocr::PpOcr;
use image::imageops::{resize, FilterType};

fn main() -> anyhow::Result<()> {
    let ocr = PpOcr::new(&PpOcr::default_models_dir())?;
    let factors: Vec<f32> = std::env::var("FACTORS").unwrap_or("1.0,0.7,0.5,0.4,0.3".into()).split(',').map(|f| f.parse().unwrap()).collect();
    for path in std::env::args().skip(1) {
        let img = image::open(&path)?.to_rgb8();
        let read = |im: &image::RgbImage| -> Vec<(i32, i32, String)> {
            let free = ocr.boxes(im).unwrap();
            read_card_names(&ocr, im, &free).into_iter().map(|c| (c.x as i32, c.y0 as i32, c.text)).collect()
        };
        let truth = read(&img);
        for f in &factors {
            let small = resize(&img, (1280.0 * f) as u32, (960.0 * f) as u32, FilterType::CatmullRom);
            let back = resize(&small, 1280, 960, FilterType::CatmullRom);
            let got = read(&back);
            let right = truth.iter().filter(|t| got.iter().any(|g| g.0 == t.0 && (g.1 - t.1).abs() <= 3 && g.2 == t.2)).count();
            let wrong: Vec<String> = got.iter().filter(|g| truth.iter().any(|t| t.0 == g.0 && (g.1 - t.1).abs() <= 3 && t.2 != g.2)).map(|g| format!("{}@{}", g.2, g.0)).collect();
            println!("{}  factor {f}: {right}/{} same, {} read, wrong: {wrong:?}", path.rsplit('/').next().unwrap(), truth.len(), got.len());
        }
    }
    Ok(())
}
