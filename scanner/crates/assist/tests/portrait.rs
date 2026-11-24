//! The squad strip's portraits told by the shipped templates, on the recorded 4:3 and 16:9 screens.
//! Needs the wiki DB (students and their kind); skipped without it. Ground truth for the slots is the squad of the
//! recording (read from its squad screen); for the grid cards it is their own name label.
use baocr_assist::portrait::{probe, Book, MIN_MARGIN, MIN_SCORE};
use baocr_core::ocr_api::TextBox;
use baocr_wikidb::Wiki;
use std::collections::HashMap;
use std::path::PathBuf;

/// Right edges of the six slots, and the strip's centre line (4:3 frame); the popup rises by `rise` in 16:9.
const EDGES: [f32; 6] = [111.0, 205.0, 300.0, 392.0, 490.0, 579.0];

const CASES: [(&str, f32, [&str; 6]); 2] = [
    ("quick-formation", 0.0, ["호시노(무장)", "하나코(수영복)", "시로코*테러", "사오리(드레스)", "아코", "키사키"]),
    ("w169-quick-formation", 120.0, ["호시노(무장)", "하나코(수영복)", "시로코*테러", "미카", "아코", "키사키"]),
];

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

#[test]
fn the_slots_and_the_grid_cards_are_told_from_the_shipped_templates() {
    let path = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../wikidb/wiki.db"));
    if !path.exists() {
        return;
    }
    let students = Wiki::open(&path).unwrap().students().unwrap();
    let kind: HashMap<i64, bool> = students.iter().map(|s| (s.id, s.squad_type == "Main")).collect();
    let name_of: HashMap<i64, String> = students.iter().map(|s| (s.id, s.name.clone())).collect();
    let same = |a: i64, b: i64| name_of.get(&a) == name_of.get(&b);
    let by_name: HashMap<String, i64> = students.iter().map(|s| (s.name.replace('*', ""), s.id)).collect();
    let book = Book::bundled();
    assert!(book.len() >= 260, "the shipped templates cover the roster: {}", book.len());

    // (what, truth, found, score, margin)
    let mut slot_rows = Vec::new();
    let mut card_rows = Vec::new();
    for (stem, rise, names) in CASES {
        let img = image::open(fixture(&format!("{stem}.png"))).unwrap().to_rgb8();
        for (k, name) in names.iter().enumerate() {
            let truth = by_name[&name.replace('*', "")];
            let main = k < 4;
            let m = book.identify(&probe(&img, EDGES[k] - 44.0, 716.0 - rise), &|id| kind.get(&id) == Some(&main), &same).unwrap();
            slot_rows.push((format!("{stem} slot{}", k + 1), truth, m));
        }
        for b in recorded_boxes(stem).iter().filter(|b| b.x0 > 570.0 && b.x1 < 1250.0 && b.y0 > 380.0 - rise && b.y0 < 640.0 - rise && b.x1 - b.x0 > 20.0) {
            let text = if b.text == "순" || b.text == "술" { "슌" } else { b.text.as_str() };
            let Some(truth) = by_name.get(text).copied() else { continue };
            let m = book.identify(&probe(&img, (b.x0 + b.x1) / 2.0, b.y0 - 61.0), &|id| kind.get(&id) == Some(&true), &same).unwrap();
            card_rows.push((format!("{stem} card {text}"), truth, m));
        }
    }
    let report = |rows: &[(String, i64, baocr_assist::portrait::Match)]| {
        let hit = rows.iter().filter(|(_, t, m)| *t == m.id || name_of.get(t) == name_of.get(&m.id)).count();
        let sure = rows.iter().filter(|(_, _, m)| m.trusted()).count();
        let sure_wrong = rows.iter().filter(|(_, t, m)| m.trusted() && name_of.get(t) != name_of.get(&m.id)).count();
        (hit, rows.len(), sure, sure_wrong)
    };
    let (sh, sn, ss, sw) = report(&slot_rows);
    let (ch, cn, cs, cw) = report(&card_rows);
    for (what, truth, m) in slot_rows.iter().chain(card_rows.iter()) {
        eprintln!("{what:<44} {} score {:.2} margin {:.2} {}", if name_of.get(truth) == name_of.get(&m.id) { "ok   " } else { "WRONG" }, m.score, m.margin, if m.trusted() { "trusted" } else { "held" });
    }
    eprintln!("slots: {sh}/{sn} right, {ss} trusted, {sw} trusted and wrong | cards: {ch}/{cn} right, {cs} trusted, {cw} trusted and wrong (min score {MIN_SCORE}, min margin {MIN_MARGIN})");
    // a wrong guess shown as a student is worse than none: nothing trusted may be wrong
    assert_eq!((sw, cw), (0, 0), "no trusted match is wrong");
    assert!(sh >= sn - 3, "most slots are right: {sh}/{sn}");
    assert!(ss >= sn / 2, "and most are trusted: {ss}/{sn}");
}
