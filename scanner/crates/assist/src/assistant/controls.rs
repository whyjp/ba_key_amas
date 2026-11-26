//! The card's buttons: what a click does, and the rows of the card's body.

use super::*;

impl<'a, S: Surface> Assistant<'a, S> {
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

    pub(super) fn handle(&mut self, event: &str) -> bool {
        // the card was dragged and snapped to a side: remember where
        if let Some(anchor) = Anchor::decode(event) {
            self.settings.set_anchor(anchor);
            return true;
        }
        // the pin button: the card now stays open, or folds into a tab
        if let Some(p) = Presentation::decode(event) {
            self.settings.set_presentation(p);
            return true;
        }
        if event == BUTTON_SETTINGS {
            self.settings_open = !self.settings_open;
            return true;
        }
        if event == BUTTON_STUDENT_SECTION {
            self.settings.student_open = !self.settings.student_open;
            return true;
        }
        if event == BUTTON_QUIT {
            if !self.commands.contains(&Command::Quit) {
                self.commands.push(Command::Quit);
            }
            return false;
        }
        if event == BUTTON_SCAN {
            // one scan at a time; a click while scanning is ignored
            let start = Command::StartScan { deep: self.settings.scan_deep };
            if !self.scanning && !self.commands.contains(&start) {
                self.commands.push(start);
            }
            return false;
        }
        if event == BUTTON_PROGRESS_SCAN {
            if !self.scanning && !self.commands.contains(&Command::StartProgressScan) {
                self.commands.push(Command::StartProgressScan);
            }
            return false;
        }
        if event == BUTTON_UPLOAD {
            if self.uploading || self.scanning {
                return false;
            }
            if !self.account_saved && self.member_code.is_none() {
                self.upload_note = vec!["회원코드를 먼저 붙여넣어 주세요 (아래 업로드 항목)".into()];
                return true;
            }
            if !self.commands.contains(&Command::Upload) {
                self.commands.push(Command::Upload);
            }
            return false;
        }
        if event == BUTTON_CODE_PASTE {
            if !self.commands.contains(&Command::PasteMemberCode) {
                self.commands.push(Command::PasteMemberCode);
            }
            return false;
        }
        if event == BUTTON_CODE_CHANGE {
            self.account_saved = false;
            self.member_code = None;
            return true;
        }
        if event == BUTTON_LINK_OPEN || event == BUTTON_LINK_COPY {
            if let Some(url) = self.upload_link.clone() {
                self.commands.push(if event == BUTTON_LINK_OPEN { Command::OpenUrl(url) } else { Command::CopyText(url) });
            }
            return false;
        }
        if event == BUTTON_SCAN_DEEP {
            self.settings.scan_deep = !self.settings.scan_deep;
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

    /// The card's body: one folding section for collecting and uploading the data, or, behind the header's gear,
    /// the display options.
    pub(super) fn control_rows(&self) -> Vec<Row> {
        let on_off = |on: bool| if on { "켜짐" } else { "꺼짐" }.to_string();
        if self.settings_open {
            let mut rows = vec![Row::Heading("표시 설정".into())];
            rows.push(Row::Button { id: BUTTON_INGAME.into(), label: "인게임 표시".into(), value: on_off(self.settings.ingame_visible), on: self.settings.ingame_visible });
            for w in &self.widgets {
                let p = self.settings.placement(w.id());
                rows.push(Row::Button { id: format!("{BUTTON_PLACE}{}", w.id().key()), label: w.title().into(), value: p.label(w.kind()).into(), on: p != Placement::Off });
            }
            return rows;
        }
        let open = self.settings.student_open;
        let mut rows = vec![Row::Section { id: BUTTON_STUDENT_SECTION.into(), title: "데이터 수집 · 업로드".into(), open, summary: if open { String::new() } else { self.library.age_text(self.now) } }];
        if !open {
            return rows;
        }
        rows.push(Row::Heading("1  학생 정보".into()));
        rows.push(Row::Hint { label: "갱신".into(), value: self.library.age_text(self.now), tip: self.library.exact_text() });
        rows.push(Row::Button { id: BUTTON_SCAN_DEEP.into(), label: "스캔 · 상세 포함".into(), value: on_off(self.settings.scan_deep), on: self.settings.scan_deep });
        rows.push(Row::Button { id: BUTTON_SCAN.into(), label: "학생 정보 스캔".into(), value: "시작".into(), on: false });
        rows.push(Row::Heading("2  진행 상태".into()));
        rows.push(Row::Button { id: BUTTON_PROGRESS_SCAN.into(), label: "진행 상태 스캔".into(), value: "시작".into(), on: false });
        rows.push(Row::Heading("3  업로드".into()));
        if self.account_saved {
            rows.push(Row::Button { id: BUTTON_CODE_CHANGE.into(), label: "계정".into(), value: "저장됨 · 바꾸기".into(), on: false });
        } else {
            rows.push(Row::Info { label: "회원코드".into(), value: self.member_code.clone().unwrap_or_else(|| "붙여넣기 필요".into()) });
            rows.push(Row::Button { id: BUTTON_CODE_PASTE.into(), label: "회원코드 붙여넣기".into(), value: "클립보드".into(), on: false });
        }
        rows.push(Row::Button { id: BUTTON_UPLOAD.into(), label: "스캔 결과 업로드".into(), value: "전송".into(), on: false });
        if let Some((first, rest)) = self.upload_note.split_first() {
            rows.push(Row::Info { label: "결과".into(), value: first.clone() });
            rows.extend(rest.iter().map(|l| Row::Note(l.clone())));
        }
        if self.upload_link.is_some() {
            rows.push(Row::Heading("4  내 페이지".into()));
            rows.push(Row::Button { id: BUTTON_LINK_OPEN.into(), label: "내 페이지".into(), value: "열기".into(), on: false });
            rows.push(Row::Button { id: BUTTON_LINK_COPY.into(), label: "주소".into(), value: "복사".into(), on: false });
        }
        rows
    }
}
