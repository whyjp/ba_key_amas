//! The 16:9 recordings, read through the assistant: the squad, the portrait marks and the raid encounter.
//! Needs the OCR models (BAOCR_MODELS or ./models) and the wiki DB; skipped without them.
use baocr_assist::widgets::{card_marks_in, slot_marks_in};
use baocr_assist::Observer;
use baocr_core::ocr_api::{Ocr, TextBox};
use baocr_extract::scene::{Layout, Scene};
use baocr_extract::CardName;
use baocr_ocr::PpOcr;
use baocr_wikidb::Wiki;
use std::path::PathBuf;

fn engine() -> Option<PpOcr> {
    let models = std::env::var_os("BAOCR_MODELS").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../models")));
    models.join("korean_PP-OCRv5_rec_mobile.onnx").exists().then(|| PpOcr::new(&models).unwrap())
}

fn wiki() -> Option<Wiki> {
    let path = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../wikidb/wiki.db"));
    path.exists().then(|| Wiki::open(&path).unwrap())
}

fn fixture(name: &str) -> String {
    format!("{}/../../../fixtures/screens/{name}", env!("CARGO_MANIFEST_DIR"))
}

fn recorded_boxes(name: &str) -> Vec<TextBox> {
    std::fs::read_to_string(fixture(&format!("{name}.boxes.txt")))
        .unwrap()
        .lines()
        .filter_map(|l| {
            let (pos, text) = l.split_once('\t')?;
            let v: Vec<f32> = pos.split(',').filter_map(|n| n.parse().ok()).collect();
            (v.len() == 4).then(|| TextBox { x0: v[0], y0: v[1], x1: v[2], y1: v[3], text: text.to_string(), score: 1.0 })
        })
        .collect()
}

/// The grid's cards as the name labels would give them: the card's centre (the grid has six columns 111 px apart,
/// the first centred at x 629) and the top of the label (the name text's top is 3 px below it).
fn cards_from_names(boxes: &[TextBox], rows_y: (f32, f32)) -> Vec<(String, CardName)> {
    boxes
        .iter()
        .filter(|b| b.x0 > 570.0 && b.x1 < 1240.0 && b.y0 > rows_y.0 && b.y0 < rows_y.1 && b.text.chars().any(|c| ('\u{AC00}'..='\u{D7A3}').contains(&c)))
        .map(|b| {
            let column = (((b.x0 + b.x1) / 2.0 - 629.0) / 111.0).round().max(0.0);
            // the one-letter 슌 is read "순" by free text: the real reader settles it by ink width, here it is told
            let text = if b.text == "순" { "슌".to_string() } else { b.text.clone() };
            (text.clone(), CardName { text, x: 629.0 + 111.0 * column, y0: b.y0 - 3.0, bare: false })
        })
        .collect()
}

#[test]
fn the_squad_and_the_portrait_marks_land_on_the_16_9_quick_formation_popup() {
    let (Some(ocr), Some(wiki)) = (engine(), wiki()) else { return };
    let (students, types) = (wiki.students().unwrap(), wiki.type_table().unwrap());
    let brief = wiki.campaign_missions(Some((19, 3)), "설원 지구 제2 지역").unwrap().remove(0);
    let mut img = image::open(fixture("w169-quick-formation.png")).unwrap().to_rgb8();
    assert_eq!((img.width(), img.height()), (1280, 720));
    let boxes = ocr.boxes(&img).unwrap();

    // the squad screen gives the slots, the popup keeps them
    let mut observer = Observer::new(&ocr, &students);
    // (the squad screen has no popup: a frame without its tab pixels, the text and the pixels of one frame agree)
    let squad = observer.observe(&image::RgbImage::new(1280, 720), &recorded_boxes("w169-squad"), Scene::Unknown);
    assert_eq!((squad.scene, squad.layout), (Scene::Squad, Layout::Ratio169));
    let names = |ids: &[i64]| ids.iter().map(|id| students.iter().find(|s| s.id == *id).unwrap().name.clone()).collect::<Vec<_>>();
    assert_eq!(names(&squad.squad), vec!["호시노(무장)", "하나코(수영복)", "시로코*테러", "미카", "아코", "키사키"]);
    let popup = observer.observe(&img, &boxes, Scene::Unknown);
    assert_eq!((popup.scene, popup.layout), (Scene::QuickFormation, Layout::Ratio169));
    assert!(popup.squad.len() >= 2, "the specials under the popup are read: {:?}", names(&popup.squad));

    // two whole rows of six, from the name plates
    let cards: Vec<CardName> = cards_from_names(&boxes, (290.0, 500.0))
        .into_iter()
        .filter_map(|(text, c)| Wiki::match_student_as(&students, &text, Some("Main")).map(|s| CardName { text: s.name, ..c }))
        .collect();
    assert_eq!(cards.len(), 12, "{:?}", cards.iter().map(|c| (&c.text, c.x, c.y0)).collect::<Vec<_>>());
    let team: Vec<_> = squad.squad.iter().filter_map(|id| students.iter().find(|s| s.id == *id).cloned()).collect();
    let mut marks = card_marks_in(&cards, &students, Some(&brief), Some("Outdoor"), &types, Some("Main"), Layout::Ratio169);
    assert_eq!(marks.len(), 12, "every whole card of the two rows gets its icons");
    // and none for a card of the 4:3 frame's rows: the same names two rows lower are out of the 16:9 viewport
    assert!(card_marks_in(&cards.iter().map(|c| CardName { y0: c.y0 + 120.0, ..c.clone() }).collect::<Vec<_>>(), &students, Some(&brief), Some("Outdoor"), &types, Some("Main"), Layout::Ratio169).len() < 12);
    let slots = slot_marks_in(&team, Some(&brief), Some("Outdoor"), &types, Layout::Ratio169);
    assert_eq!(slots.len(), 6);
    marks.extend(slots);

    let Some(fonts) = baocr_overlay::load_fonts() else { return };
    let layer = baocr_overlay::render_marks(1280, 720, &marks, &fonts);
    assert_eq!((layer.width(), layer.height()), (1280, 720));
    // the first card's badge sits on its portrait's right edge, the first slot's on the strip
    let drawn = |x: u32, y: u32| layer.get_pixel(x, y)[3] > 0;
    assert!(drawn(629 + 52, 243 + 10), "first card's column, 68 px above its name label");
    assert!(drawn(111, 580 + 10), "first slot's column");
    assert!(!drawn(300, 300), "the empty left of the popup stays clear");
    if let Some(dir) = std::env::var_os("BANNER_PREVIEW_DIR") {
        let mut base = image::DynamicImage::ImageRgb8(std::mem::take(&mut img)).to_rgba8();
        image::imageops::overlay(&mut base, &layer, 0, 0);
        base.save(std::path::Path::new(&dir).join("placement-169.png")).unwrap();
    }
}
