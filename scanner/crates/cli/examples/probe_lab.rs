//! Times the 64x48 probe of the game window and shows how far consecutive probes are apart.
//!   cargo run --release -p baocr --example probe_lab [seconds]
//! Read-only: it only copies the screen. Leave the game alone for the first half and click around in the second.
use baocr_hcu::probe::{look_at_game, probe_changed_points, probe_distance, Mode, Prober};
use std::time::{Duration, Instant};

fn main() -> anyhow::Result<()> {
    let secs: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(20);
    for mode in [Mode::Halftone, Mode::ColorOnColor, Mode::BlackOnWhite] {
        let p = Prober::new(mode)?;
        look_at_game(&p)?;
        let mut v: Vec<f64> = (0..15).map(|_| {
            let t = Instant::now();
            std::hint::black_box(look_at_game(&p).unwrap());
            t.elapsed().as_secs_f64() * 1000.0
        }).collect();
        v.sort_by(|a, b| a.total_cmp(b));
        println!("{mode:?}: probe {:.1} ms (median of 15)", v[v.len() / 2]);
    }
    // consecutive probes 100 ms apart: the mean difference and how many points changed by more than 30 and by more than 60
    let p = Prober::new(Mode::Halftone)?;
    let mut prev = look_at_game(&p)?;
    let start = Instant::now();
    let mut rows = Vec::new();
    while start.elapsed() < Duration::from_secs(secs) {
        std::thread::sleep(Duration::from_millis(100));
        let cur = look_at_game(&p)?;
        rows.push((start.elapsed().as_secs_f32(), probe_distance(&prev, &cur), probe_changed_points(&prev, &cur, 30), probe_changed_points(&prev, &cur, 60)));
        prev = cur;
    }
    println!("t: mean/points>30/points>60 (only steps that are not quiet are listed; quiet is mean<0.8 and points>30 <3)");
    let quiet = rows.iter().filter(|r| r.1 < 0.8 && r.2 < 3).count();
    for (t, m, a, b) in rows.iter().filter(|r| !(r.1 < 0.8 && r.2 < 3)) {
        println!("  {t:5.1}s mean {m:7.2} points>30 {a:4} points>60 {b:4}");
    }
    let (qm, qa) = (rows.iter().filter(|r| r.1 < 0.8 && r.2 < 3).map(|r| r.1).fold(0.0f32, f32::max), rows.iter().filter(|r| r.1 < 0.8 && r.2 < 3).map(|r| r.2).max().unwrap_or(0));
    println!("{} quiet steps of {}; the loudest quiet one: mean {qm:.2}, points>30 {qa}", quiet, rows.len());
    Ok(())
}
