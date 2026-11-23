//! Prints the watcher's stability metrics for consecutive live frames (no input).
use baocr_hcu::{game_capture, parse_frame_region, Hcu};
use baocr_pixels::{detail_visible, frame_distance, frame_key, panel_delta};

fn main() -> anyhow::Result<()> {
    let hcu = Hcu::default_backend();
    let region = parse_frame_region("230.36,91.73,685.39,433.07")?;
    let mut prev: Option<image::RgbImage> = None;
    for i in 0..8 {
        let t = std::time::Instant::now();
        let img = game_capture(&hcu, region)?;
        let ms = t.elapsed().as_millis();
        let (pd, fd) = prev
            .as_ref()
            .map(|p| (panel_delta(p, &img), frame_distance(&frame_key(p), &frame_key(&img))))
            .unwrap_or((-1.0, -1.0));
        println!("frame {i}: capture {ms} ms, detail_visible={}, panel_delta={pd:.5}, frame_distance={fd:.3}", detail_visible(&img));
        prev = Some(img);
        std::thread::sleep(std::time::Duration::from_millis(1000));
    }
    Ok(())
}
