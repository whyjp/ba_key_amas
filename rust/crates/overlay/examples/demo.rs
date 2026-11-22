//! Shows the banner for ~20 s over the window named by BAOCR_OVERLAY_TITLE (default: the game).
use baocr_overlay::Overlay;

fn main() {
    let o = Overlay::start();
    o.counting();
    std::thread::sleep(std::time::Duration::from_secs(4));
    for i in 0..=106 {
        o.scanning(i, Some(106));
        std::thread::sleep(std::time::Duration::from_millis(150));
    }
    o.finished(106, Some(106));
    o.close_after(3);
}
