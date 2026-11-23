//! `--assist`: a passive guide. It only looks at the game (screenshots, no input):
//! it recognises the screen, remembers the mission picked on the 임무 정보 popup, and
//! shows the screen and the mission's terrain and enemy types on a card beside the
//! game window.
use anyhow::{bail, Result};
use baocr_core::ocr_api::Ocr;
use baocr_extract::scene::{classify, quick_formation_cards, squad_names, CardName, MissionRef, MissionTracker, Scene};
use baocr_hcu::{detect_region, game_capture, screen_kind, Hcu, Screen};
use baocr_overlay::{Chip, Icon, Mark, Overlay, PanelState, Row, Tone};
use baocr_pixels::{frame_distance, frame_key, quick_card_selected};
use baocr_wikidb::{evaluate, FormationScore, MissionBrief, StudentBrief, Tally, Verdict, Wiki};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Frames this alike are the same view; two such frames in a row mean the screen has settled.
const STILL: f64 = 1.5;
/// Re-detect the game window this often (loops), so moving or resizing it is followed.
const REGION_EVERY: u32 = 25;

fn chips(list: &[Tally], suffix: &str) -> Vec<Chip> {
    list.iter().map(|t| Chip { text: format!("{} {}{suffix}", t.ko, t.n), tone: Tone::from_code(&t.code) }).collect()
}

/// The card for what the player is looking at.
pub fn panel_for(scene: Scene, mission: Option<&MissionRef>, wiki: &Wiki) -> Option<PanelState> {
    if scene == Scene::Unknown {
        return None;
    }
    let mut p = PanelState { scene: scene.label().to_string(), ..Default::default() };
    let on_mission_screen = matches!(scene, Scene::MissionInfo | Scene::EnemyInfo | Scene::MissionMap | Scene::Squad | Scene::QuickFormation);
    let Some(m) = mission.filter(|_| on_mission_screen) else {
        return Some(p);
    };
    p.title = m.display();
    let found = match (m.stage_no, m.place.as_deref()) {
        (Some(n), Some(place)) => wiki.dungeon_missions(place, n),
        (Some(_), None) => {
            p.rows.push(Row::Note("장소를 알 수 없습니다. 스테이지 목록 화면부터 열어 주세요".into()));
            return Some(p);
        }
        _ => wiki.campaign_missions(m.number, &m.name),
    };
    match found {
        Ok(found) if !found.is_empty() => rows_for(&mut p, &found),
        Ok(_) => p.rows.push(Row::Note("위키DB에서 이 임무를 찾지 못했습니다".into())),
        Err(e) => p.rows.push(Row::Note(format!("위키DB 오류: {e}"))),
    }
    Some(p)
}

/// Where the card is placed relative to a student card's name label (frame pixels): the
/// icon row sits on the portrait, above the label, clear of the level and the stars.
const ICONS_ABOVE_NAME: f32 = 58.0;

/// The missions the game's mission screen can stand for (empty when unknown).
fn briefs(m: &MissionRef, wiki: &Wiki) -> Vec<MissionBrief> {
    match (m.stage_no, m.place.as_deref()) {
        (Some(n), Some(place)) => wiki.dungeon_missions(place, n),
        (Some(_), None) => Ok(Vec::new()),
        _ => wiki.campaign_missions(m.number, &m.name),
    }
    .unwrap_or_default()
}

/// Icons for the student cards on screen: attack and armor type, and, when the mission's
/// terrain is known, how well the student fits it (the game's own face).
pub fn marks_for(cards: &[CardName], students: &[StudentBrief], terrain: Option<&str>) -> Vec<Mark> {
    cards
        .iter()
        .filter_map(|c| {
            let s = Wiki::match_student(students, &c.text)?;
            let mut icons = Vec::new();
            if let Some(level) = terrain.and_then(|t| s.adaptation(t)) {
                icons.push(Icon::Face(level));
            }
            icons.push(Icon::Attack(s.bullet_type.clone()));
            icons.push(Icon::Armor(s.armor_type.clone()));
            Some(Mark { cx: c.x.round() as i32, cy: (c.y0 - ICONS_ABOVE_NAME).round() as i32, icons })
        })
        .collect()
}

/// "+42%" / "-13%" / "평균" for a figure relative to the average (1.0).
fn relative_text(ratio: f64) -> String {
    let pct = ((ratio - 1.0) * 100.0).round() as i32;
    if pct.abs() < 3 {
        "평균".to_string()
    } else {
        format!("{pct:+}%")
    }
}

/// The verdict and the three bars for a squad on a mission.
pub fn formation_rows(score: &FormationScore, terrain_known: bool) -> Vec<Row> {
    let (text, tone) = match score.verdict {
        Verdict::Favorable => ("유리", Tone::Good),
        Verdict::Average => ("평균", Tone::Mid),
        Verdict::Unfavorable => ("불리", Tone::Bad),
    };
    let permille = |r: f64| (r * 1000.0).round() as i32;
    let mut rows = vec![
        Row::Verdict { label: "편성 판정".into(), text: text.into(), tone },
        Row::Meter { label: "공격 상성".into(), permille: permille(score.attack), text: relative_text(score.attack) },
        Row::Meter { label: "방어 상성".into(), permille: permille(score.defense), text: relative_text(score.defense) },
    ];
    if terrain_known {
        rows.push(Row::Meter { label: "지형 적응".into(), permille: permille(score.terrain), text: relative_text(score.terrain) });
    }
    rows.push(Row::Note(format!("인식한 학생 {}명 기준 · 전체 학생 평균 대비 참고용", score.members)));
    rows
}

fn rows_for(p: &mut PanelState, found: &[MissionBrief]) {
    let b = &found[0];
    if b.area.is_some() {
        p.rows.push(Row::Info { label: "난이도".into(), value: if b.difficulty == 0 { "Normal" } else { "Hard" }.into() });
    }
    if let Some(t) = &b.terrain {
        p.rows.push(Row::Info { label: "지형".into(), value: t.ko.clone() });
    }
    if let Some(l) = b.recommended_level {
        p.rows.push(Row::Info { label: "추천 Lv".into(), value: l.to_string() });
    }
    p.rows.push(Row::Chips { label: "적 공격".into(), chips: chips(&b.attack, "") });
    p.rows.push(Row::Chips { label: "적 방어".into(), chips: chips(&b.armor, "") });
    p.rows.push(Row::Chips { label: "추천 공격".into(), chips: chips(&b.best_attack.iter().take(3).cloned().collect::<Vec<_>>(), "%") });
    if found.len() > 1 {
        p.rows.push(Row::Note("Normal 기준입니다. Hard는 적 레벨이 다릅니다".into()));
    }
}

pub fn run(ocr: &dyn Ocr, hcu: &Hcu, mut region: [f64; 4], auto_region: bool, wiki_path: &Path, interval_ms: u64) -> Result<()> {
    if !wiki_path.exists() {
        bail!("wiki database not found at {}: build it with `cargo run -p baocr-wikidb --release`", wiki_path.display());
    }
    let wiki = Wiki::open(wiki_path)?;
    let students = wiki.students()?;
    let types = wiki.type_table()?;
    let overlay = Overlay::start_hidden();
    // ids of the students in the squad being edited, as far as they have been seen
    let mut squad: Vec<i64> = Vec::new();
    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    ctrlc::set_handler(move || flag.store(true, Ordering::Relaxed))?;
    println!("assist: watching the game (screenshots only, no input); Ctrl-C to stop");

    let mut tracker = MissionTracker::default();
    let (mut prev, mut handled, mut still, mut loops) = (None, None, 0u32, 0u32);
    while !stop.load(Ordering::Relaxed) {
        loops += 1;
        if auto_region && loops % REGION_EVERY == 0 {
            if let Ok(r) = detect_region(hcu) {
                region = r;
            }
        }
        let img = match game_capture(hcu, region) {
            Ok(i) => i,
            Err(e) => {
                eprintln!("assist: capture failed: {e:#}");
                std::thread::sleep(Duration::from_millis(1000));
                continue;
            }
        };
        let key = frame_key(&img);
        still = if prev.as_ref().is_some_and(|p| frame_distance(p, &key) < STILL) { still + 1 } else { 0 };
        prev = Some(key);
        let new_view = handled.as_ref().map_or(true, |h| frame_distance(h, &key) > STILL);
        if still >= 1 && new_view {
            // Text first: the pixel check for the student list/profile also fires on other
            // busy screens (the 업무 hub), so it only answers when the text names nothing.
            let boxes = ocr.boxes(&img).unwrap_or_default();
            let mut scene = classify(&boxes, None);
            if scene == Scene::Unknown {
                scene = match screen_kind(&img) {
                    Screen::Profile => Scene::StudentProfile,
                    Screen::Roster => Scene::StudentList,
                    Screen::Other => Scene::Unknown,
                };
            }
            let mission = tracker.update(scene, &boxes).cloned();
            // who is in the squad: the names on the 편성 screen, then the lime-outlined cards of the popup
            match scene {
                Scene::Squad => {
                    squad = squad_names(&boxes)
                        .iter()
                        .filter_map(|(n, special)| Wiki::match_student_as(&students, n, Some(if *special { "Support" } else { "Main" })))
                        .map(|st| st.id)
                        .collect();
                }
                Scene::QuickFormation => {
                    for c in quick_formation_cards(&boxes) {
                        if let Some(st) = Wiki::match_student(&students, &c.text) {
                            squad.retain(|id| *id != st.id);
                            if quick_card_selected(&img, c.x.round() as i32, c.y0.round() as i32) {
                                squad.push(st.id);
                            }
                        }
                    }
                }
                _ => {}
            }
            let brief = if matches!(scene, Scene::Squad | Scene::QuickFormation) { mission.as_ref().and_then(|m| briefs(m, &wiki).into_iter().next()) } else { None };
            let terrain = brief.as_ref().and_then(|b| b.terrain.as_ref().map(|t| t.code.clone()));
            let chart = brief.as_ref().and_then(|b| {
                let team: Vec<StudentBrief> = squad.iter().filter_map(|id| students.iter().find(|st| st.id == *id).cloned()).collect();
                evaluate(b, terrain.as_deref(), &team, &students, &types).map(|sc| formation_rows(&sc, terrain.is_some()))
            });
            // icons over the portraits of the 빠른 편성 grid, only while that popup is up
            let marks = if scene == Scene::QuickFormation { marks_for(&quick_formation_cards(&boxes), &students, terrain.as_deref()) } else { Vec::new() };
            overlay.marks(marks);
            // The grid shows names only in the list view; the compact view has none to read.
            let compact_grid = scene == Scene::QuickFormation && quick_formation_cards(&boxes).is_empty();
            match panel_for(scene, mission.as_ref(), &wiki) {
                Some(mut p) => {
                    if let Some(rows) = chart {
                        p.rows.extend(rows);
                    }
                    if compact_grid {
                        p.rows.push(Row::Note("학생 이름이 보이는 보기(목록 오른쪽 위 버튼)에서 아이콘이 표시됩니다".into()));
                    }
                    overlay.panel(p)
                }
                None => overlay.hide(),
            }
            println!("assist: {:?}{}", scene, mission.as_ref().map(|m| format!("  mission: {}", m.display())).unwrap_or_default());
            handled = Some(key);
        }
        std::thread::sleep(Duration::from_millis(interval_ms));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn wiki() -> Wiki {
        let conn = Connection::open_in_memory().unwrap();
        baocr_wikidb::create(&conn).unwrap();
        conn.execute_batch(
            "INSERT INTO source VALUES (1,'t','u','2026-11-23',NULL,'n');
             INSERT INTO label VALUES ('Terrain','Outdoor','야외'),('ArmorType','HeavyArmor','중장갑'),('BulletType','Mystic','신비'),('BulletType','Normal','일반');
             INSERT INTO type_effectiveness VALUES ('Mystic','HeavyArmor',20000);
             INSERT INTO enemy VALUES (1,'a','중장갑병','Minion','Main','Normal','HeavyArmor','S',1,'{}');
             INSERT INTO stage VALUES (10,'Campaign',NULL,19,3,0,'설원 지구 제2 지역','Outdoor',85,NULL,NULL,NULL,1);
             INSERT INTO stage_formation VALUES (100,10,0,'Grade1',1,1);
             INSERT INTO formation_enemy VALUES (100,0,1);
             INSERT INTO student VALUES (1,'a','a','s1','','',3,'Main','','','Mystic','Normal',1,1,1,'',1,1,'{}');",
        )
        .unwrap();
        Wiki::from_connection(conn)
    }

    fn mission() -> MissionRef {
        MissionRef { number: Some((19, 3)), stage_no: None, place: None, name: "설원 지구 제2 지역".into() }
    }

    #[test]
    fn the_squad_screen_shows_the_remembered_mission_with_terrain_and_enemy_types() {
        let p = panel_for(Scene::Squad, Some(&mission()), &wiki()).unwrap();
        assert_eq!(p.scene, "편성");
        assert_eq!(p.title, "19-3 설원 지구 제2 지역");
        assert!(p.rows.contains(&Row::Info { label: "난이도".into(), value: "Normal".into() }));
        assert!(p.rows.contains(&Row::Info { label: "지형".into(), value: "야외".into() }));
        assert!(p.rows.contains(&Row::Info { label: "추천 Lv".into(), value: "85".into() }));
        let armor = p.rows.iter().find_map(|r| match r {
            Row::Chips { label, chips } if label == "적 방어" => Some(chips.clone()),
            _ => None,
        });
        assert_eq!(armor, Some(vec![Chip { text: "중장갑 1".into(), tone: Tone::HeavyArmor }]));
    }

    #[test]
    fn screens_without_a_mission_show_only_where_the_player_is() {
        let p = panel_for(Scene::Lobby, None, &wiki()).unwrap();
        assert_eq!((p.scene.as_str(), p.title.as_str(), p.rows.len()), ("메인 메뉴", "", 0));
        // a remembered mission is not shown on screens that have nothing to do with it
        assert!(panel_for(Scene::WorkHub, Some(&mission()), &wiki()).unwrap().title.is_empty());
        // the squad screen reached from the main menu (no mission picked) is just 편성
        assert!(panel_for(Scene::Squad, None, &wiki()).unwrap().title.is_empty());
    }

    #[test]
    fn a_dungeon_mission_shows_its_place_and_says_so_when_the_place_is_unknown() {
        let conn = Connection::open_in_memory().unwrap();
        baocr_wikidb::create(&conn).unwrap();
        conn.execute_batch(
            "INSERT INTO source VALUES (1,'t','u','2026-11-23',NULL,'n');
             INSERT INTO label VALUES ('StageType','ChaserA','현상수배 - 고가도로'),('Terrain','Street','시가지');
             INSERT INTO stage VALUES (30103,'Bounty','ChaserA',NULL,3,NULL,'현상수배 - 고가도로','Street',20,NULL,NULL,NULL,1);",
        )
        .unwrap();
        let w = Wiki::from_connection(conn);
        let m = MissionRef { number: None, stage_no: Some(3), place: Some("현상수배-고가도로".into()), name: "고가도로 C".into() };
        let p = panel_for(Scene::Squad, Some(&m), &w).unwrap();
        assert_eq!(p.title, "현상수배-고가도로 03 고가도로 C");
        assert!(p.rows.contains(&Row::Info { label: "지형".into(), value: "시가지".into() }));
        assert!(p.rows.contains(&Row::Info { label: "추천 Lv".into(), value: "20".into() }));
        let no_place = MissionRef { place: None, ..m };
        assert!(matches!(panel_for(Scene::Squad, Some(&no_place), &w).unwrap().rows.as_slice(), [Row::Note(_)]));
    }

    #[test]
    fn student_cards_get_attack_armor_and_terrain_icons() {
        let mk = |id: i64, name: &str, bullet: &str, armor: &str, street: u8| StudentBrief { id, name: name.into(), bullet_type: bullet.into(), armor_type: armor.into(), street, outdoor: 0, indoor: 0, squad_type: "Main".into() };
        let students = vec![mk(1, "호시노(무장)", "Pierce", "HeavyArmor", 4), mk(2, "케이", "Mystic", "Unarmed", 1)];
        let cards = vec![
            CardName { text: "호시노(무장)".into(), x: 736.0, y0: 429.0 },
            CardName { text: "케이".into(), x: 1181.0, y0: 432.0 },
            CardName { text: "알수없음".into(), x: 900.0, y0: 430.0 },
        ];
        let with = marks_for(&cards, &students, Some("Street"));
        assert_eq!(with.len(), 2, "a name that matches no student gets no icons");
        assert_eq!(with[0].icons, vec![Icon::Face(4), Icon::Attack("Pierce".into()), Icon::Armor("HeavyArmor".into())]);
        assert_eq!((with[0].cx, with[0].cy), (736, 371), "on the portrait, above the name label");
        assert_eq!(with[1].icons[0], Icon::Face(1));
        // without a known mission terrain only the types are shown
        let without = marks_for(&cards, &students, None);
        assert_eq!(without[0].icons, vec![Icon::Attack("Pierce".into()), Icon::Armor("HeavyArmor".into())]);
    }

    #[test]
    fn the_verdict_and_bars_read_naturally() {
        let score = FormationScore { attack: 1.42, defense: 0.87, terrain: 1.01, overall: 1.2, members: 5, verdict: Verdict::Favorable };
        let rows = formation_rows(&score, true);
        assert_eq!(rows[0], Row::Verdict { label: "편성 판정".into(), text: "유리".into(), tone: Tone::Good });
        assert_eq!(rows[1], Row::Meter { label: "공격 상성".into(), permille: 1420, text: "+42%".into() });
        assert_eq!(rows[2], Row::Meter { label: "방어 상성".into(), permille: 870, text: "-13%".into() });
        assert_eq!(rows[3], Row::Meter { label: "지형 적응".into(), permille: 1010, text: "평균".into() });
        assert!(matches!(rows.last(), Some(Row::Note(n)) if n.contains("5명")));
        assert_eq!(formation_rows(&score, false).len(), rows.len() - 1, "no terrain bar without a known terrain");
        let bad = FormationScore { verdict: Verdict::Unfavorable, ..score };
        assert!(matches!(&formation_rows(&bad, true)[0], Row::Verdict { text, tone: Tone::Bad, .. } if text == "불리"));
    }

    #[test]
    fn an_unknown_screen_hides_the_card_and_an_unknown_mission_says_so() {
        assert_eq!(panel_for(Scene::Unknown, None, &wiki()), None);
        let missing = MissionRef { number: Some((1, 1)), stage_no: None, place: None, name: "없는 임무".into() };
        let p = panel_for(Scene::Squad, Some(&missing), &wiki()).unwrap();
        assert!(matches!(p.rows.as_slice(), [Row::Note(_)]));
    }
}
