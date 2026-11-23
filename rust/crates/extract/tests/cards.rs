use baocr_extract::read_card_names;
use baocr_ocr::PpOcr;
use std::path::PathBuf;

fn engine() -> Option<PpOcr> {
    let m = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../models"));
    m.join("korean_PP-OCRv5_rec_mobile.onnx").exists().then(|| PpOcr::new(&m).unwrap())
}

fn frame(name: &str) -> image::RgbImage {
    image::open(format!("{}/../../../fixtures/screens/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap().to_rgb8()
}

fn names(name: &str) -> Vec<String> {
    let Some(ocr) = engine() else { return Vec::new() };
    let img = frame(name);
    let free = baocr_core::ocr_api::Ocr::boxes(&ocr, &img).unwrap();
    read_card_names(&ocr, &img, &free).into_iter().map(|c| c.text).collect()
}

#[test]
fn every_card_of_the_list_view_is_read_including_the_ones_free_text_ocr_misses() {
    let n = names("quick-formation.png");
    if n.is_empty() {
        return;
    }
    // the second row has "슌" (free OCR reads it as "순") and the first has "케이"
    for want in ["시로코*테러", "호시노(무장)", "하나코(수영복)", "사오리(드레스)", "리오(무장)", "케이", "슌", "사츠키(수영복)", "미카", "마코토(수영복)", "아리스", "토키(바니걸)"] {
        assert!(n.iter().any(|x| x == want), "{want} in {n:?}");
    }
    assert_eq!(n.len(), 12, "{n:?}");
}

#[test]
fn scrolled_grids_are_read_too() {
    let a = names("quick-formation-scrolled-a.png");
    if a.is_empty() {
        return;
    }
    // "루미" is the card free-text OCR did not read at all
    for want in ["키라라", "사쿠라코", "아루(드레스)", "호시노", "렌게", "루미", "와카모(수영복)", "히나", "시구레", "카스미", "시로코(라이딩)", "모에(수영복)"] {
        assert!(a.iter().any(|x| x == want), "{want} in {a:?}");
    }
    let b = names("quick-formation-scrolled-b.png");
    for want in ["츠쿠요", "마리나(치파오)", "사키(수영복)", "이치카", "메구"] {
        assert!(b.iter().any(|x| x == want), "{want} in {b:?}");
    }
}

#[test]
fn a_live_frame_where_the_closed_set_reader_stops_short_is_corrected_by_the_free_text() {
    let n = names("quick-formation-live-a.png");
    if n.is_empty() {
        return;
    }
    // closed-set alone read "시로코" and "토키" here; the free text spells the whole names
    for want in ["시로코*테러", "토키(바니걸)", "슌", "호시노(무장)"] {
        assert!(n.iter().any(|x| x == want), "{want} in {n:?}");
    }
    assert!(!n.iter().any(|x| x == "시로코" || x == "토키"), "{n:?}");
    assert_eq!(n.len(), 12, "{n:?}");
}
