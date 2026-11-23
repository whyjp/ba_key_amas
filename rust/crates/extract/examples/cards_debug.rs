//! Reads the student cards of a saved 빠른 편성 frame: `cards_debug <png>` (set BAOCR_DEBUG_CARDS=1 for scores).
use baocr_core::ocr_api::Ocr;
use baocr_extract::read_card_names;
use baocr_ocr::PpOcr;

fn main() -> anyhow::Result<()> {
    let path = std::env::args().nth(1).expect("png path");
    let ocr = PpOcr::new(&PpOcr::default_models_dir())?;
    let img = image::open(path)?.to_rgb8();
    let free = ocr.boxes(&img)?;
    for c in read_card_names(&ocr, &img, &free) {
        println!("{:>5.0} {:>4.0}  {}", c.x, c.y0, c.text);
    }
    Ok(())
}
