# Go에서 Rust로 이식: 모듈 계약

**상태: 완료.** Rust 이식이 라이브에서 Go와 일치한 뒤 Go 프로그램을 삭제했다. 아래에서 말하는 Go 소스는 git 기록
(커밋 `2847447`, `cmd/baocr/`)에 있다.

목표: Rust 워크스페이스가 `cmd/baocr`(Go)를 대체한다. 동작은 Go 코드와 같아야 한다(Go 코드가 기준이다.
`../cmd/baocr/*.go`, 테스트 `*_test.go`). 스키마는 `crates/core/src/model.rs`이며 Go JSON과 맞는다. 픽스처는
`../fixtures/*.png`(1280x960 4:3 프로필 프레임, 값은 Go 테스트에서 눈으로 확인함)이다.

크레이트는 각자 Go 파일을 이식하고 아래 API를 내보낸다. 다른 크레이트의 파일은 고치지 않는다.

| 크레이트 | 이식 대상 | 공개 API(Rust 이름, Go 이름의 snake_case) |
|---|---|---|
| baocr-core | model, ocr trait | `model::*`, `ocr_api::{Ocr, TextBox, text_in}` (완료) |
| baocr-names | namematch.go, meta.go/fixed.go/deep.go의 일부 | 아래 참고 |
| baocr-pixels | meta.go 픽셀 판독기, capture.go, roster_detect.go, arrow_scan.go 지문, ppocr.go 잉크 이미지 | 아래 참고 |
| baocr-equipment | equipment.go | `detect_equipment(img:&RgbImage, ocr:&dyn Ocr) -> Result<(Vec<Equipment>, BTreeMap<String,String>)>` |
| baocr-export | main.go 내보내기/CSV/HTML, roster_scan.go 보고서, store.go, pipeline.go Index | 아래 참고 |

## baocr-names
`entries() -> &'static [DictEntry]` (`data/student_names_kr.json`을 내장. DictEntry는 Go `dictEntry`를 따른다),
`to_jamo`, `edit_distance`, `clean_name`, `resolve(raw:&str)->(String, NameMatch)` (창, maxCorrection, 모호/미등재 규칙),
`best_name(&[&str])->String`, `parse_name(&str)->NameParts` (+ 의상 키 표),
`pick_entry(name, role_hint)->Option<(DictEntry, bool ambiguous)>`, `lookup_reference(name,&Loadout)->Option<Reference>`,
`apply_fixed(&mut Student, role_hint:&str)` (fixed.go applyFixed), `cross_check(&mut Student)` (meta.go crossCheck),
`deep_cross_check(&mut Student)` (deep.go deepCrossCheck), `level_from_exp(&mut Student)` + `EXP_TO_LEVEL`,
`closest_word(raw, vocab)->String` (동점에도 안전), `all_names()->Vec<String>`.

## baocr-pixels  (모두 1280x960 프레임의 `&image::RgbImage`를 받는다)
ROI 상수는 `image::math::Rect` 비슷한 튜플 `(x0,y0,x1,y1)`이다. `detail_visible`, `panel_delta(a,b)`,
`frame_key/frame_distance`, `detail_key/detail_distance`, `read_badges`, `read_locks`, `read_skill_locks`,
`read_terrain`, `role_from_icon` (`data/roleicons/*.png`를 내장. 라벨은 파일 이름 줄기), `attack_armor_types(img)->(String,String)`,
`stars`, `weapon_stars`, `pin_state`, `heart_ink_image`, `name_ink_image` (-> `GrayImage`, 흰 바탕에 검은 잉크),
`text_touches_right_edge`,
로스터: `scroll_thumb`, `roster_cards`, `roster_visible_pixels` (rosterVisible의 픽셀 부분이며 detailVisible 부정 포함).

## baocr-export
`meta_columns(&Student)->Vec<MetaCol{key,label,value}>`, `student_csv(&Student)->(Vec<String>,Vec<String>)`,
`export_student(&Student, dir)` (student.json/csv/html. 프레임은 여기서 쓰지 않는다),
`save_report(&RosterReport, dir)` (students.json/csv/html, roster_scan.go의 필터 적용),
`Store` (`open`, `upsert(&Student, DateTime)->Result<(Vec<Change>,bool)>`, `all`), `diff_variable`, `student_key`,
`Index` (`new/load/update/sorted/save`)와 `rebuild_report(dir,&Index,RosterReport)->RosterReport`.
