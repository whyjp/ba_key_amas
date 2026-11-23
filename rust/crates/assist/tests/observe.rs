use baocr_assist::Observer;
use baocr_core::ocr_api::Ocr;
use baocr_extract::scene::Scene;
use baocr_ocr::PpOcr;
use baocr_wikidb::Wiki;
use std::path::PathBuf;

fn engine() -> Option<PpOcr> {
    let m = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../models"));
    m.join("korean_PP-OCRv5_rec_mobile.onnx").exists().then(|| PpOcr::new(&m).unwrap())
}

#[test]
fn the_quick_formation_popup_gives_its_cards_the_tab_and_the_whole_squad() {
    let wiki_path = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../wikidb/wiki.db"));
    let (Some(ocr), true) = (engine(), wiki_path.exists()) else { return };
    let wiki = Wiki::open(&wiki_path).unwrap();
    let students = wiki.students().unwrap();
    let img = image::open(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../fixtures/screens/quick-formation-live-a.png")).unwrap().to_rgb8();
    let boxes = ocr.boxes(&img).unwrap();
    let mut observer = Observer::new(&ocr, &students);
    let obs = observer.observe(&img, &boxes, Scene::Unknown);
    assert_eq!(obs.scene, Scene::QuickFormation);
    assert_eq!(obs.grid_kind, Some("Main"), "the STRIKER tab is active");
    assert_eq!(obs.cards.len(), 12);
    let names: Vec<&str> = obs.squad.iter().map(|id| students.iter().find(|s| s.id == *id).unwrap().name.as_str()).collect();
    // four lime-outlined strikers in the grid, and the two specials named under the popup
    assert_eq!(names, vec!["시로코*테러", "호시노(무장)", "하나코(수영복)", "사오리(드레스)", "아코", "키사키"], "{names:?}");
}
