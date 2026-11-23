//! Screen learning helper (console tool). `probe grab OUT.png` saves the game client as a 1280x960
//! frame and the OCR text boxes beside it (OUT.boxes.txt). `probe click X Y` clicks a point of that
//! frame after checking that the game window is the active one. Nothing else is sent.
use baocr_core::ocr_api::Ocr;
use baocr_hcu::{check_aspect, detect_region, game_capture, to_frame, HcuSession};
use baocr_ocr::PpOcr;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let session = HcuSession::default_backend();
    let hcu = session.view();
    let region = detect_region(hcu)?;
    if args.first().map(String::as_str) == Some("region") {
        println!("region {region:?} size {:.0}x{:.0} ratio {:.3}", region[2] - region[0], region[3] - region[1], (region[2] - region[0]) / (region[3] - region[1]));
        return Ok(());
    }
    if std::env::var_os("BAOCR_ANY_ASPECT").is_none() {
        check_aspect(region)?;
    }
    match args.first().map(String::as_str) {
        Some("grab") => {
            let out = args.get(1).ok_or_else(|| anyhow::anyhow!("grab OUT.png"))?;
            let img = game_capture(hcu, region)?;
            img.save(out)?;
            let ocr = PpOcr::new(&PpOcr::default_models_dir())?;
            let mut text = String::new();
            for b in ocr.boxes(&img)? {
                text.push_str(&format!("{:.0},{:.0},{:.0},{:.0}\t{}\n", b.x0, b.y0, b.x1, b.y1, b.text.trim()));
            }
            std::fs::write(std::path::Path::new(out).with_extension("boxes.txt"), text)?;
            println!("saved {out}");
        }
        Some("click") => {
            let (x, y): (i32, i32) = (args[1].parse()?, args[2].parse()?);
            let input = session.exclusive();
            input.activate()?;
            std::thread::sleep(std::time::Duration::from_millis(500));
            input.foreground()?;
            let (fx, fy) = to_frame(region, x, y);
            input.click(fx, fy)?;
            println!("clicked client ({x},{y})");
        }
        Some("scroll") => {
            let (x, y, d): (i32, i32, i32) = (args[1].parse()?, args[2].parse()?, args[3].parse()?);
            let input = session.exclusive();
            input.activate()?;
            std::thread::sleep(std::time::Duration::from_millis(500));
            input.foreground()?;
            let (fx, fy) = to_frame(region, x, y);
            input.scroll(fx, fy, d)?;
            println!("scrolled at client ({x},{y}) by {d}");
        }
        _ => anyhow::bail!("probe grab OUT.png | probe click X Y | probe scroll X Y DELTA"),
    }
    Ok(())
}
