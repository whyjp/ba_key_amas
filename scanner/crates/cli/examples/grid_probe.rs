//! Shows whether the fingerprint of the 빠른 편성 grid is the same from one capture of the live game to the next.
//!   cargo run --release -p baocr --example grid_probe [captures]
//! Read-only. Open 빠른 편성 first; put students in the squad and take them out while it runs to see what moves.
use baocr_assist::observe::grid_snapshot;
use baocr_hcu::{detect_region, game_capture, HcuSession};
use std::time::Duration;

fn main() -> anyhow::Result<()> {
    let n: usize = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(30);
    let session = HcuSession::native();
    let hcu = session.view();
    let region = detect_region(hcu)?;
    let mut first: Option<Vec<(i32, i32, u64)>> = None;
    for i in 0..n {
        let img = game_capture(hcu, region)?;
        let snap = grid_snapshot(&img);
        let same = first.as_ref().map_or(0, |f| f.iter().zip(&snap).filter(|(a, b)| a == b).count());
        let moved = first.as_ref().map_or(0, |f| f.iter().zip(&snap).filter(|(a, b)| (a.0, a.1) != (b.0, b.1)).count());
        println!("capture {i:2}: {} cards, {} identical to the first capture's, {} at another place", snap.len(), same, moved);
        if first.is_none() {
            first = Some(snap);
        }
        std::thread::sleep(Duration::from_millis(300));
    }
    Ok(())
}
