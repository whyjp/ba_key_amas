//! `--assist`: a passive guide. It only looks at the game (screenshots, no input):
//! it recognises the screen, remembers the mission picked on the 임무 정보 popup, and
//! shows the screen and the mission's terrain and enemy types on a card beside the
//! game window.
use anyhow::{bail, Result};
use baocr_core::ocr_api::Ocr;
use baocr_extract::scene::{classify, MissionRef, MissionTracker, Scene};
use baocr_hcu::{detect_region, game_capture, screen_kind, Hcu, Screen};
use baocr_overlay::{Chip, Overlay, PanelState, Row, Tone};
use baocr_pixels::{frame_distance, frame_key};
use baocr_wikidb::{MissionBrief, Tally, Wiki};
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
    let on_mission_screen = matches!(scene, Scene::MissionInfo | Scene::EnemyInfo | Scene::MissionMap | Scene::Squad);
    let Some(m) = mission.filter(|_| on_mission_screen) else {
        return Some(p);
    };
    p.title = m.display();
    match wiki.campaign_missions(m.number, &m.name) {
        Ok(found) if !found.is_empty() => rows_for(&mut p, &found),
        Ok(_) => p.rows.push(Row::Note("위키DB에서 이 임무를 찾지 못했습니다".into())),
        Err(e) => p.rows.push(Row::Note(format!("위키DB 오류: {e}"))),
    }
    Some(p)
}

fn rows_for(p: &mut PanelState, found: &[MissionBrief]) {
    let b = &found[0];
    p.rows.push(Row::Info { label: "난이도".into(), value: if b.difficulty == 0 { "Normal" } else { "Hard" }.into() });
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
    let overlay = Overlay::start_hidden();
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
            let pixel = match screen_kind(&img) {
                Screen::Profile => Some(Scene::StudentProfile),
                Screen::Roster => Some(Scene::StudentList),
                Screen::Other => None,
            };
            let boxes = if pixel.is_some() { Vec::new() } else { ocr.boxes(&img).unwrap_or_default() };
            let scene = classify(&boxes, pixel);
            let mission = tracker.update(scene, &boxes).cloned();
            match panel_for(scene, mission.as_ref(), &wiki) {
                Some(p) => overlay.panel(p),
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
        MissionRef { number: Some((19, 3)), name: "설원 지구 제2 지역".into() }
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
    fn an_unknown_screen_hides_the_card_and_an_unknown_mission_says_so() {
        assert_eq!(panel_for(Scene::Unknown, None, &wiki()), None);
        let missing = MissionRef { number: Some((1, 1)), name: "없는 임무".into() };
        let p = panel_for(Scene::Squad, Some(&missing), &wiki()).unwrap();
        assert!(matches!(p.rows.as_slice(), [Row::Note(_)]));
    }
}
