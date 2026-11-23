//! The coordinator: observation in, widgets, settings, surface out.
//!
//! It owns no screen-reading and no drawing. Each frame it asks the `Observer` what is
//! on screen, asks every `Widget` what it would show, and sends each output where the
//! `Settings` say (beside the game, inside it, nowhere) through the `Surface`. Clicks on
//! the card's buttons come back as events and change the settings.
use crate::observe::{Observation, Observer};
use crate::settings::{Placement, Settings};
use crate::surface::Surface;
use crate::widget::{is_mission_screen, Context, Kind, Lookup, Output, Widget, WidgetId};
use crate::widgets;
use baocr_core::ocr_api::TextBox;
use baocr_extract::scene::Scene;
use baocr_overlay::{PanelState, Row};
use baocr_wikidb::{StudentBrief, TypeTable, Wiki};
use image::RgbImage;
use std::path::PathBuf;

const BUTTON_MENU: &str = "menu";
const BUTTON_INGAME: &str = "ingame";
const BUTTON_PLACE: &str = "place:";

pub struct Assistant<'a, S: Surface> {
    observer: Observer<'a>,
    wiki: &'a Wiki,
    students: &'a [StudentBrief],
    types: &'a TypeTable,
    widgets: Vec<Box<dyn Widget>>,
    settings: Settings,
    settings_path: Option<PathBuf>,
    surface: S,
    menu_open: bool,
    last: Observation,
}

fn lookup(wiki: &Wiki, obs: &Observation) -> Lookup {
    let Some(m) = obs.mission.as_ref().filter(|_| is_mission_screen(obs.scene)) else { return Lookup::None };
    let found = match (m.stage_no, m.place.as_deref()) {
        (Some(n), Some(place)) => wiki.dungeon_missions(place, n),
        (Some(_), None) => return Lookup::PlaceUnknown,
        _ => wiki.campaign_missions(m.number, &m.name),
    }
    .unwrap_or_default();
    match found.first() {
        Some(brief) => Lookup::Found { brief: brief.clone(), alternatives: found.len() },
        None => Lookup::NotFound,
    }
}

impl<'a, S: Surface> Assistant<'a, S> {
    pub fn new(observer: Observer<'a>, wiki: &'a Wiki, students: &'a [StudentBrief], types: &'a TypeTable, settings: Settings, settings_path: Option<PathBuf>, surface: S) -> Self {
        Assistant { observer, wiki, students, types, widgets: widgets::all(), settings, settings_path, surface, menu_open: false, last: Observation::new(Scene::Unknown) }
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    /// A new settled frame: read it and show what follows from it.
    pub fn on_frame(&mut self, img: &RgbImage, boxes: &[TextBox], fallback: Scene) -> Scene {
        let obs = self.observer.observe(img, boxes, fallback);
        let scene = obs.scene;
        self.show(obs);
        scene
    }

    /// Show what an observation calls for.
    pub fn show(&mut self, obs: Observation) {
        self.last = obs;
        self.present();
    }

    /// Handle the card's button clicks. Returns whether anything changed.
    pub fn poll(&mut self) -> bool {
        let events = self.surface.take_events();
        let mut changed = false;
        for e in events {
            changed |= self.handle(&e);
        }
        if changed {
            if let Some(path) = &self.settings_path {
                let _ = self.settings.save(path);
            }
            self.present();
        }
        changed
    }

    fn handle(&mut self, event: &str) -> bool {
        if event == BUTTON_MENU {
            self.menu_open = !self.menu_open;
            return true;
        }
        if event == BUTTON_INGAME {
            self.settings.ingame_visible = !self.settings.ingame_visible;
            return true;
        }
        if let Some(widget) = event.strip_prefix(BUTTON_PLACE).and_then(WidgetId::from_key) {
            if let Some(w) = self.widgets.iter().find(|w| w.id() == widget) {
                self.settings.cycle(widget, w.kind());
                return true;
            }
        }
        false
    }

    fn menu_rows(&self) -> Vec<Row> {
        let mut rows = vec![Row::Button { id: BUTTON_INGAME.into(), label: "인게임 표시".into(), value: if self.settings.ingame_visible { "켜짐" } else { "꺼짐" }.into(), on: self.settings.ingame_visible }];
        for w in &self.widgets {
            let p = self.settings.placement(w.id());
            rows.push(Row::Button { id: format!("{BUTTON_PLACE}{}", w.id().key()), label: w.title().into(), value: p.label(w.kind()).into(), on: p != Placement::Off });
        }
        rows
    }

    fn present(&self) {
        let obs = &self.last;
        let cx = Context { obs, lookup: lookup(self.wiki, obs), students: self.students, types: self.types };
        let (mut outside, mut ingame): (Vec<Row>, Vec<Row>) = (Vec::new(), Vec::new());
        let (mut outside_title, mut ingame_title) = (String::new(), String::new());
        let mut marks = Vec::new();
        for w in &self.widgets {
            let placement = self.settings.effective(w.id());
            if placement == Placement::Off {
                continue;
            }
            match (w.render(&cx), placement) {
                (Some(Output::Card { title, rows }), Placement::Outside) => {
                    if outside_title.is_empty() {
                        outside_title = title;
                    }
                    outside.extend(rows);
                }
                (Some(Output::Card { title, rows }), Placement::InGame) => {
                    if ingame_title.is_empty() {
                        ingame_title = title;
                    }
                    ingame.extend(rows);
                }
                (Some(Output::Marks(m)), Placement::InGame) => marks.extend(m),
                _ => {}
            }
        }
        // the main card: the settings button always, so the menu can always be reached
        let mut rows = vec![Row::Button { id: BUTTON_MENU.into(), label: "설정".into(), value: if self.menu_open { "닫기" } else { "열기" }.into(), on: self.menu_open }];
        if self.menu_open {
            rows.extend(self.menu_rows());
        }
        rows.extend(outside);
        let icons_wanted = self.widgets.iter().any(|w| w.kind() == Kind::Marks && self.settings.effective(w.id()) != Placement::Off);
        if obs.grid_without_names() && icons_wanted {
            rows.push(Row::Note("학생 이름이 보이는 보기(목록 오른쪽 위 버튼)에서 아이콘이 표시됩니다".into()));
        }
        if std::env::var_os("BAOCR_DEBUG_ASSIST").is_some() {
            eprintln!("assist: scene={:?} squad={:?} cards={:?} marks={} kind={:?}", obs.scene, obs.squad, obs.cards.iter().map(|c| c.text.as_str()).collect::<Vec<_>>(), marks.len(), obs.grid_kind);
        }
        self.surface.main_card(PanelState { scene: obs.scene.label().to_string(), title: outside_title, rows });
        self.surface.ingame_card((!ingame.is_empty()).then(|| PanelState { scene: String::new(), title: ingame_title, rows: ingame }));
        self.surface.marks(marks);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use baocr_core::ocr_api::Ocr;
    use baocr_extract::scene::MissionRef;
    use baocr_extract::CardName;
    use baocr_overlay::Mark;
    use rusqlite::Connection;
    use std::cell::RefCell;
    use std::rc::Rc;

    struct NoOcr;
    impl Ocr for NoOcr {
        fn boxes(&self, _: &RgbImage) -> anyhow::Result<Vec<TextBox>> {
            anyhow::bail!("no ocr in tests")
        }
        fn line(&self, _: &RgbImage) -> anyhow::Result<(String, f32)> {
            anyhow::bail!("no ocr in tests")
        }
        fn choose(&self, _: &RgbImage, _: &[String]) -> anyhow::Result<(usize, Vec<f32>)> {
            anyhow::bail!("no ocr in tests")
        }
    }

    #[derive(Default)]
    struct Shown {
        main: Option<PanelState>,
        ingame: Option<PanelState>,
        marks: Vec<Mark>,
        clicks: Vec<String>,
    }

    #[derive(Clone, Default)]
    struct Fake(Rc<RefCell<Shown>>);

    impl Surface for Fake {
        fn main_card(&self, card: PanelState) {
            self.0.borrow_mut().main = Some(card);
        }
        fn ingame_card(&self, card: Option<PanelState>) {
            self.0.borrow_mut().ingame = card;
        }
        fn marks(&self, marks: Vec<Mark>) {
            self.0.borrow_mut().marks = marks;
        }
        fn take_events(&self) -> Vec<String> {
            std::mem::take(&mut self.0.borrow_mut().clicks)
        }
    }

    fn wiki() -> Wiki {
        let conn = Connection::open_in_memory().unwrap();
        baocr_wikidb::create(&conn).unwrap();
        conn.execute_batch(
            "INSERT INTO source VALUES (1,'t','u','2026-11-23',NULL,'n');
             INSERT INTO label VALUES ('Terrain','Outdoor','야외'),('ArmorType','HeavyArmor','중장갑'),('BulletType','Mystic','신비'),('BulletType','Normal','일반');
             INSERT INTO type_effectiveness VALUES ('Mystic','HeavyArmor',20000),('Pierce','HeavyArmor',5000);
             INSERT INTO enemy VALUES (1,'a','중장갑병','Minion','Main','Normal','HeavyArmor','S',1,'{}');
             INSERT INTO stage VALUES (10,'Campaign',NULL,19,3,0,'설원 지구 제2 지역','Outdoor',85,NULL,NULL,NULL,1);
             INSERT INTO stage_formation VALUES (100,10,0,'Grade1',1,1);
             INSERT INTO formation_enemy VALUES (100,0,1);
             INSERT INTO student VALUES (1,'a','a','신비학생','','',3,'Main','','','Mystic','HeavyArmor',1,4,1,'',1,1,'{}'),(2,'b','b','관통학생','','',3,'Main','','','Pierce','HeavyArmor',1,0,1,'',1,1,'{}'),
                                        (3,'c','c','스페셜학생','','',3,'Support','','','Mystic','HeavyArmor',1,2,1,'',1,1,'{}');",
        )
        .unwrap();
        Wiki::from_connection(conn)
    }

    struct Rig {
        fake: Fake,
        wiki: Wiki,
        students: Vec<StudentBrief>,
        types: TypeTable,
    }

    fn rig() -> Rig {
        let w = wiki();
        let (students, types) = (w.students().unwrap(), w.type_table().unwrap());
        Rig { fake: Fake::default(), wiki: w, students, types }
    }

    fn mission() -> Option<MissionRef> {
        Some(MissionRef { number: Some((19, 3)), stage_no: None, place: None, name: "설원 지구 제2 지역".into() })
    }

    fn squad_obs(scene: Scene) -> Observation {
        Observation { mission: mission(), squad: vec![1, 3], ..Observation::new(scene) }
    }

    fn assistant<'a>(r: &'a Rig, ocr: &'a NoOcr) -> Assistant<'a, Fake> {
        Assistant::new(Observer::new(ocr, &r.students), &r.wiki, &r.students, &r.types, Settings::default(), None, r.fake.clone())
    }

    fn rows_of(p: &PanelState) -> String {
        format!("{:?}", p.rows)
    }

    #[test]
    fn by_default_the_mission_and_verdict_are_beside_the_game_and_nothing_is_inside() {
        let (r, ocr) = (rig(), NoOcr);
        let mut a = assistant(&r, &ocr);
        a.show(squad_obs(Scene::Squad));
        let shown = r.fake.0.borrow();
        let main = shown.main.as_ref().unwrap();
        assert_eq!((main.scene.as_str(), main.title.as_str()), ("편성", "19-3 설원 지구 제2 지역"));
        assert!(rows_of(main).contains("야외") && rows_of(main).contains("편성 판정"), "{main:?}");
        assert!(shown.ingame.is_none() && shown.marks.is_empty());
        assert!(matches!(&main.rows[0], Row::Button { id, .. } if id == "menu"), "the settings button comes first");
    }

    #[test]
    fn the_menu_opens_and_a_widget_moves_inside_the_game() {
        let (r, ocr) = (rig(), NoOcr);
        let mut a = assistant(&r, &ocr);
        a.show(squad_obs(Scene::Squad));
        r.fake.0.borrow_mut().clicks.push("menu".into());
        assert!(a.poll());
        assert!(rows_of(r.fake.0.borrow().main.as_ref().unwrap()).contains("place:formation"), "menu rows appear");
        // the formation card goes inside the game, the mission card stays beside it
        r.fake.0.borrow_mut().clicks.push("place:formation".into());
        a.poll();
        let shown = r.fake.0.borrow();
        assert!(rows_of(shown.ingame.as_ref().expect("an in-game card")).contains("Verdict"));
        // (the menu's own button is also called "편성 판정", so look for the verdict row itself)
        assert!(!rows_of(shown.main.as_ref().unwrap()).contains("Verdict") && rows_of(shown.main.as_ref().unwrap()).contains("야외"));
    }

    #[test]
    fn the_master_switch_removes_everything_inside_the_game_but_not_the_card_beside_it() {
        let (r, ocr) = (rig(), NoOcr);
        let mut a = assistant(&r, &ocr);
        r.fake.0.borrow_mut().clicks.push("place:formation".into());
        a.show(squad_obs(Scene::QuickFormation));
        a.poll();
        assert!(r.fake.0.borrow().ingame.is_some() && !r.fake.0.borrow().marks.is_empty(), "icons and the moved card are inside the game");
        r.fake.0.borrow_mut().clicks.push("ingame".into());
        a.poll();
        let shown = r.fake.0.borrow();
        assert!(shown.ingame.is_none() && shown.marks.is_empty());
        assert!(rows_of(shown.main.as_ref().unwrap()).contains("야외"), "the card beside the game stays");
        assert!(!a.settings().ingame_visible);
    }

    #[test]
    fn slot_stats_follow_the_squad_and_icons_can_be_turned_off_one_by_one() {
        let (r, ocr) = (rig(), NoOcr);
        let mut a = assistant(&r, &ocr);
        let mut obs = squad_obs(Scene::QuickFormation);
        obs.cards = vec![CardName { text: "관통학생".into(), x: 629.0, y0: 428.0 }];
        a.show(obs);
        {
            let shown = r.fake.0.borrow();
            // one icon row on the grid card, one per squad member on the strip at the bottom
            assert_eq!(shown.marks.len(), 3, "{:?}", shown.marks);
            assert!(shown.marks.iter().any(|m| m.cx == 68 && m.cy == 718), "first striker slot");
            assert!(shown.marks.iter().any(|m| m.cx == 445), "first special slot");
        }
        r.fake.0.borrow_mut().clicks.push("place:slot_stats".into());
        a.poll();
        assert_eq!(r.fake.0.borrow().marks.len(), 1, "only the grid card's icons are left");
    }

    #[test]
    fn a_grid_without_names_says_how_to_get_icons_and_other_screens_show_only_the_scene() {
        let (r, ocr) = (rig(), NoOcr);
        let mut a = assistant(&r, &ocr);
        a.show(Observation::new(Scene::QuickFormation));
        assert!(rows_of(r.fake.0.borrow().main.as_ref().unwrap()).contains("이름이 보이는 보기"));
        a.show(Observation::new(Scene::Lobby));
        let shown = r.fake.0.borrow();
        let main = shown.main.as_ref().unwrap();
        assert_eq!((main.scene.as_str(), main.rows.len()), ("메인 메뉴", 1), "just the settings button");
    }

    #[test]
    fn card_widgets_stay_off_when_set_off_and_unknown_buttons_are_ignored() {
        let (r, ocr) = (rig(), NoOcr);
        let mut a = assistant(&r, &ocr);
        a.show(squad_obs(Scene::Squad));
        // beside -> inside -> off
        for _ in 0..2 {
            r.fake.0.borrow_mut().clicks.push("place:mission_info".into());
            a.poll();
        }
        assert!(!rows_of(r.fake.0.borrow().main.as_ref().unwrap()).contains("야외"));
        assert!(r.fake.0.borrow().ingame.is_none());
        r.fake.0.borrow_mut().clicks.push("nonsense".into());
        assert!(!a.poll());
    }
}
