# Go -> Rust port: module contracts

Goal: the Rust workspace replaces `cmd/baocr` (Go). Behaviour must match the Go
code (it is the reference: `../cmd/baocr/*.go`, tests `*_test.go`). Schema =
`crates/core/src/model.rs` (matches Go JSON). Fixtures: `../fixtures/*.png`
(1280x960 4:3 profile frames, values verified by eye in the Go tests).

Crates (each ports specific Go files and exposes the API below; no crate edits another's files):

| crate | ports | public API (Rust names, snake_case of the Go ones) |
|---|---|---|
| baocr-core | model, ocr trait | `model::*`, `ocr_api::{Ocr, TextBox, text_in}` (done) |
| baocr-names | namematch.go, parts of meta.go/fixed.go/deep.go | see below |
| baocr-pixels | meta.go pixel readers, capture.go, roster_detect.go, arrow_scan.go fingerprints, ppocr.go ink images | see below |
| baocr-equipment | equipment.go | `detect_equipment(img:&RgbImage, ocr:&dyn Ocr) -> Result<(Vec<Equipment>, BTreeMap<String,String>)>` |
| baocr-export | main.go export/CSV/HTML, roster_scan.go report, store.go, pipeline.go Index | see below |

## baocr-names
`entries() -> &'static [DictEntry]` (embed `data/student_names_kr.json`; DictEntry mirrors Go `dictEntry`),
`to_jamo`, `edit_distance`, `clean_name`, `resolve(raw:&str)->(String, NameMatch)` (windows, maxCorrection, ambiguous/unlisted rules),
`best_name(&[&str])->String`, `parse_name(&str)->NameParts` (+ outfit key table),
`pick_entry(name, role_hint)->Option<(DictEntry, bool ambiguous)>`, `lookup_reference(name,&Loadout)->Option<Reference>`,
`apply_fixed(&mut Student, role_hint:&str)` (fixed.go applyFixed), `cross_check(&mut Student)` (meta.go crossCheck),
`deep_cross_check(&mut Student)` (deep.go deepCrossCheck), `level_from_exp(&mut Student)` + `EXP_TO_LEVEL`,
`closest_word(raw, vocab)->String` (tie-safe), `all_names()->Vec<String>`.

## baocr-pixels  (all take `&image::RgbImage` of the 1280x960 frame)
ROI consts as `image::math::Rect`-like tuples `(x0,y0,x1,y1)`; `detail_visible`, `panel_delta(a,b)`, `frame_key/frame_distance`,
`detail_key/detail_distance`, `read_badges`, `read_locks`, `read_skill_locks`, `read_terrain`, `role_from_icon`
(embed `data/roleicons/*.png`; label = file stem), `attack_armor_types(img)->(String,String)`, `stars`, `weapon_stars`, `pin_state`,
`heart_ink_image`, `name_ink_image` (-> `GrayImage`, black ink on white), `text_touches_right_edge`,
roster: `scroll_thumb`, `roster_cards`, `roster_visible_pixels` (the pixel part of rosterVisible incl. the detailVisible negative).

## baocr-export
`meta_columns(&Student)->Vec<MetaCol{key,label,value}>`, `student_csv(&Student)->(Vec<String>,Vec<String>)`,
`export_student(&Student, dir)` (student.json/csv/html; frames are not written here),
`save_report(&RosterReport, dir)` (students.json/csv/html with the filters in roster_scan.go),
`Store` (`open`, `upsert(&Student, DateTime)->Result<(Vec<Change>,bool)>`, `all`), `diff_variable`, `student_key`,
`Index` (`new/load/update/sorted/save`) and `rebuild_report(dir,&Index,RosterReport)->RosterReport`.
