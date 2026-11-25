//! The game frame from the screen directly against the one through the HCU: time, and how far apart the pixels are.
use baocr_hcu::probe::{grab_game_frame, Mode, Prober};
use baocr_hcu::{detect_region, game_capture, HcuSession};
use std::time::Instant;

fn main() -> anyhow::Result<()> {
    let session = HcuSession::default_backend();
    let hcu = session.view();
    let region = detect_region(hcu)?;
    for mode in [Mode::Halftone, Mode::ColorOnColor] {
        let prober = Prober::new(mode)?;
        let _ = grab_game_frame(&prober)?;
        let t = Instant::now();
        let mut last = None;
        for _ in 0..10 {
            last = Some(grab_game_frame(&prober)?);
        }
        let direct = last.unwrap();
        let ms = t.elapsed().as_secs_f64() * 100.0;
        let t = Instant::now();
        let viaa = game_capture(hcu, region)?;
        let hcu_ms = t.elapsed().as_secs_f64() * 1000.0;
        let n = direct.as_raw().len().min(viaa.as_raw().len());
        let diff: u64 = direct.as_raw()[..n].iter().zip(viaa.as_raw()).map(|(a, b)| (*a as i32 - *b as i32).unsigned_abs() as u64).sum();
        println!("{mode:?}: direct {ms:.1} ms ({}x{}), HCU {hcu_ms:.0} ms ({}x{}), mean abs diff {:.2}", direct.width(), direct.height(), viaa.width(), viaa.height(), diff as f64 / n as f64);
        let ocr = baocr_ocr::PpOcr::new(&baocr_ocr::PpOcr::default_models_dir())?;
        use baocr_core::ocr_api::Ocr;
        let names = |img: &image::RgbImage| baocr_extract::read_card_names(&ocr, img, &ocr.boxes(img).unwrap_or_default()).into_iter().map(|c| c.text).collect::<Vec<_>>();
        let (a, b) = (names(&direct), names(&viaa));
        println!("  cards direct {a:?}
  cards HCU    {b:?}
  same: {}", a == b);
        direct.save(format!("D:/scan-run/hill/direct-{mode:?}.png"))?;
        viaa.save("D:/scan-run/hill/viahcu.png")?;
    }
    Ok(())
}
