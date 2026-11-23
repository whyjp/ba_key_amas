//! Live end-to-end harness against the running game. See docs/live-e2e.md.
//!
//! ```text
//! live_e2e run   --exe <baocr.exe> --label directml|cpu [--only a,b] [--out DIR]
//! live_e2e shot  NAME                 grab the game client + OCR boxes into DIR
//! live_e2e click X Y [EXPECT_SCENE]   one checked click (1280x960 client coordinates)
//! live_e2e dump                       print the add-on's state dump
//! ```
//! Input to the game: navigation clicks and the add-on's own scan only, each click after a fresh
//! screenshot and a check of the scene the add-on reports. Nothing here presses 출격, 소탕, 보상, a
//! purchase or a real 입장.
#[cfg(windows)]
mod rig;
#[cfg(windows)]
mod scenarios;

#[cfg(windows)]
fn main() -> anyhow::Result<()> {
    scenarios::main()
}

#[cfg(not(windows))]
fn main() {
    eprintln!("live_e2e runs on Windows only");
}
