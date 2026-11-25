use baocr_core::ocr_api::{text_in, Ocr};
use baocr_ocr::PpOcr;
use image::RgbImage;
use std::path::PathBuf;

fn engine() -> Option<PpOcr> {
    let m = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../models"));
    if !m.join("korean_PP-OCRv5_rec_mobile.onnx").exists() {
        eprintln!("models missing: skipping");
        return None;
    }
    Some(PpOcr::new(&m).expect("engine"))
}

fn fixture(name: &str) -> RgbImage {
    image::open(format!("{}/../../../fixtures/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap().to_rgb8()
}

#[test]
fn frame_boxes_read_panel_numbers() {
    let Some(e) = engine() else { return };
    let boxes = e.boxes(&fixture("detail-shun.png")).unwrap();
    let hp = text_in(&boxes, (684, 350, 860, 374));
    assert!(hp.contains("20221"), "hp strip read {hp:?}");
    let skill = text_in(&boxes, (694, 520, 767, 548));
    assert!(skill.contains("Lv.2") || skill.contains('2'), "skill read {skill:?}");
}
