//! What is shown for an observation: the battle card, the wiki lookups and the full card with its icons.

use super::*;

impl<'a, S: Surface> Assistant<'a, S> {
    /// In a battle the add-on keeps out of the way: no in-game card, no icons, only the status and
    /// the two things the player may still need (the in-game switch, closing).
    pub(super) fn present_battle(&self) {
        let on_off = |on: bool| if on { "켜짐" } else { "꺼짐" }.to_string();
        let rows = vec![
            Row::Info { label: "게임".into(), value: if self.game_present { "연결됨" } else { "대기 중" }.into() },
            Row::Heading("표시".into()),
            Row::Button { id: BUTTON_INGAME.into(), label: "인게임 표시".into(), value: on_off(self.settings.ingame_visible), on: self.settings.ingame_visible },
        ];
        self.send_main(PanelState { scene: Scene::Battle.label().to_string(), title: String::new(), rows, ..Default::default() });
        self.send_ingame(None);
        self.send_marks(Vec::new());
    }

    pub(super) fn lookup_cached(&self, obs: &Observation) -> Lookup {
        let Some(m) = obs.mission.as_ref().filter(|_| is_mission_screen(obs.scene)) else { return Lookup::None };
        if let Some((cached, found)) = self.looked_up.borrow().as_ref() {
            if cached == m {
                return found.clone();
            }
        }
        let found = lookup(self.wiki, obs);
        *self.looked_up.borrow_mut() = Some((m.clone(), found.clone()));
        found
    }

    pub(super) fn raid_lookup_cached(&self, obs: &Observation) -> RaidLookup {
        let Some(raid) = obs.raid.as_ref() else { return RaidLookup::None };
        let now = self.now.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
        if let Some((cached, hour, found)) = self.looked_up_raid.borrow().as_ref() {
            if cached == raid && *hour == now / 3600 {
                return found.clone();
            }
        }
        let found = raid_lookup::lookup(self.wiki, raid, now).unwrap_or(RaidLookup::NotFound);
        *self.looked_up_raid.borrow_mut() = Some((raid.clone(), now / 3600, found.clone()));
        found
    }

    pub(super) fn present(&self) {
        if self.scanning {
            // the card is the scan: how far it is, and that the player's input is locked
            let (title, rows) = scan_rows(&self.scan_state.borrow(), self.input_locked);
            self.send_main(PanelState { scene: "학생 스캔".into(), title, rows, ..Default::default() });
            self.send_ingame(None);
            self.send_marks(Vec::new());
            return;
        }
        if self.last.scene == Scene::Battle {
            self.present_battle();
            return;
        }
        let obs = &self.last;
        let cx = Context { obs, lookup: self.lookup_cached(obs), raid: self.raid_lookup_cached(obs), students: self.students, types: self.types };
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
        // The main card: first what the add-on sees (the game, then the screen's own information),
        // then what it can do. It is shown whatever the screen, so it can always be used and closed.
        let mut rows = vec![Row::Info { label: "게임".into(), value: if self.game_present { "연결됨" } else { "대기 중" }.into() }];
        rows.extend(outside);
        let icons_wanted = self.widgets.iter().any(|w| w.kind() == Kind::Marks && self.settings.effective(w.id()) != Placement::Off);
        if obs.scene == Scene::LobbyIdle {
            rows.push(Row::Note("메뉴가 숨겨졌습니다. 화면을 한 번 클릭하면 메뉴가 돌아옵니다".into()));
        }
        if obs.grid_without_names() && icons_wanted {
            rows.push(Row::Note("학생 이름이 보이는 보기(목록 오른쪽 위 버튼)에서 아이콘이 표시됩니다".into()));
        }
        rows.extend(self.control_rows());
        if std::env::var_os("BAOCR_DEBUG_ASSIST").is_some() {
            eprintln!("assist: scene={:?} squad={:?} cards={:?} marks={} kind={:?}", obs.scene, obs.squad, obs.cards.iter().map(|c| c.text.as_str()).collect::<Vec<_>>(), marks.len(), obs.grid_kind);
        }
        let scene = if self.game_present { obs.scene.label().to_string() } else { "게임 대기 중".to_string() };
        self.send_main(PanelState { scene, title: outside_title, rows, ..Default::default() });
        self.send_ingame((!ingame.is_empty()).then(|| PanelState { scene: String::new(), title: ingame_title, rows: ingame, ..Default::default() }));
        self.send_marks(if self.view_moving.get() { Vec::new() } else { marks });
        self.presented.set(self.presented.get() + 1);
    }
}
