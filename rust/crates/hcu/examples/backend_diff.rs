//! Compares the in-process desktop with the `computer-use` helper on the live screen (screenshots
//! only, no input): frame size, the detected game region, the 1280x960 game capture, and speed.
use baocr_hcu::{detect_region, game_capture, Hcu};
use std::time::Instant;

fn main() -> anyhow::Result<()> {
    let bin = std::env::args().nth(1).unwrap_or_else(|| "../vendor/hcu/computer-use.exe".into());
    let (native, process) = (Hcu::native(), Hcu::new(bin));
    let mut caps = Vec::new();
    for (name, h) in [("native", &native), ("process", &process)] {
        let t = Instant::now();
        let region = match std::env::var("REGION") { Ok(r) => baocr_hcu::parse_frame_region(&r)?, Err(_) => detect_region(h)? };
        let t_region = t.elapsed().as_secs_f64();
        let t = Instant::now();
        let img = game_capture(h, region)?;
        let t_cap = t.elapsed().as_secs_f64();
        let t = Instant::now();
        let img2 = game_capture(h, region)?;
        println!("{name:8} frame={:?} region={:.2?} detect {t_region:.2}s first capture {t_cap:.2}s next {:.2}s", h.frame_size(), region, t.elapsed().as_secs_f64());
        img.save(format!("{}-{name}.png", std::env::var("OUT").unwrap_or_else(|_| "cap".into())))?;
        caps.push((img, img2));
    }
    let (a, b) = (&caps[0].0, &caps[1].0);
    let (mut sum, mut max) = (0u64, 0u32);
    for (p, q) in a.pixels().zip(b.pixels()) {
        let d: u32 = (0..3).map(|i| (p[i] as i32 - q[i] as i32).unsigned_abs()).sum();
        sum += d as u64;
        max = max.max(d);
    }
    println!("native vs process: mean abs diff per pixel {:.2} (of 765), max {max}", sum as f64 / (a.width() * a.height()) as f64);
    Ok(())
}
