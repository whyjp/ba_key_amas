//! Times the 64x48 probe of the game window and shows how far consecutive probes are apart.
//!   cargo run --release -p baocr --example probe_lab [seconds]
//! Read-only: it only copies the screen. Leave the game alone for the first half and click around in the second.
use baocr_hcu::probe::{look_at_game, probe_distance, Mode, Prober};
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
    for mode in [Mode::Halftone, Mode::ColorOnColor] {
        let p = Prober::new(mode)?;
        let mut prev = look_at_game(&p)?;
        let mut seen = Vec::new();
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(secs) {
            std::thread::sleep(Duration::from_millis(100));
            let cur = look_at_game(&p)?;
            seen.push((start.elapsed().as_secs_f32(), probe_distance(&prev, &cur)));
            prev = cur;
        }
        let mut d: Vec<f32> = seen.iter().map(|s| s.1).collect();
        d.sort_by(|a, b| a.total_cmp(b));
        let q = |f: f32| d[((d.len() - 1) as f32 * f) as usize];
        println!("{mode:?}: {} steps of 100 ms; distance min {:.3} median {:.3} p90 {:.3} p99 {:.3} max {:.3}", d.len(), q(0.0), q(0.5), q(0.9), q(0.99), q(1.0));
        let line: Vec<String> = seen.iter().map(|(t, x)| format!("{t:.1}:{x:.2}")).collect();
        println!("  {}", line.join(" "));
    }
    Ok(())
}
