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

/// A smaller game window gives a blurrier capture. The reading may then drop cards but must not
/// name a wrong student: a missing icon is a nuisance, an icon of someone else is a lie.
#[test]
fn a_blurred_capture_gives_fewer_names_but_never_a_wrong_one() {
    use baocr_core::ocr_api::Ocr;
    use image::imageops::{resize, FilterType};
    let Some(ocr) = engine() else { return };
    let read = |img: &image::RgbImage| -> Vec<(i32, i32, String)> {
        let free = ocr.boxes(img).unwrap();
        read_card_names(&ocr, img, &free).into_iter().map(|c| (c.x as i32, c.y0 as i32, c.text)).collect()
    };
    for name in ["quick-formation.png", "quick-formation-scrolled-a.png", "quick-formation-scrolled-b.png"] {
        let img = frame(name);
        let truth = read(&img);
        assert_eq!(truth.len(), 12, "{name}");
        for factor in [0.7f32, 0.5] {
            let small = resize(&img, (1280.0 * factor) as u32, (960.0 * factor) as u32, FilterType::CatmullRom);
            let blurred = resize(&small, 1280, 960, FilterType::CatmullRom);
            let got = read(&blurred);
            let wrong: Vec<_> = got.iter().filter(|g| truth.iter().any(|t| t.0 == g.0 && (t.1 - g.1).abs() <= 3 && t.2 != g.2)).collect();
            assert!(wrong.is_empty(), "{name} at {factor}: {wrong:?}");
            if factor >= 0.7 {
                assert!(got.len() >= 9, "{name} at {factor}: only {} of 12 read", got.len());
            }
        }
    }
}
