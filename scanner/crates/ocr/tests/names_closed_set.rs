use baocr_core::ocr_api::Ocr;
use baocr_ocr::PpOcr;
use image::RgbImage;
use std::path::PathBuf;

fn engine() -> Option<PpOcr> {
    let m = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../models"));
    m.join("korean_PP-OCRv5_rec_mobile.onnx").exists().then(|| PpOcr::new(&m).unwrap())
}

fn fixture(name: &str) -> RgbImage {
    image::open(format!("{}/../../../fixtures/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap().to_rgb8()
}

#[test]
fn closed_set_reads_every_labelled_name() {
    let Some(e) = engine() else { return };
    let names = baocr_names::all_names();
    let gold = [
        ("detail-shun.png", "슌"),
        ("detail-rio-armed.png", "리오(무장)"),
        ("detail-saori.png", "사오리(드레스)"),
        ("detail-satsuki-swimsuit.png", "사츠키(수영복)"),
        ("detail-shiroko-terror.png", "시로코*테러"),
        ("detail-rio-special.png", "리오"),
        ("detail-hoshino-armed.png", "호시노(무장)"),
        ("detail-hoshino-plain.png", "호시노"),
        ("detail-serika-newyear-lv1.png", "세리카(새해)"),
        ("detail-tsurugi-swimsuit-lv1-locked.png", "츠루기(수영복)"),
        ("detail-rumi-terrain1.png", "루미"),
    ];
    let mut wrong = vec![];
    for (file, want) in gold {
        let line = baocr_pixels::ink_to_line(&baocr_pixels::name_ink_image(&fixture(file)));
        let (i, _) = e.choose(&line, &names).unwrap();
        if names[i] != want {
            wrong.push(format!("{file}: got {} want {want}", names[i]));
        }
    }
    assert!(wrong.is_empty(), "closed-set misses: {wrong:?}");
}
