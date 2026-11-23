//! A testing hook: with `BAOCR_STATE_DUMP=<file>` the add-on writes what its surface shows (cards,
//! icon count, dock and fold state) as JSON, so an outside harness can assert on it. It wraps the
//! `Surface` the assistant draws on; nothing else knows about it, and with the variable unset it
//! is not in the path at all.
use baocr_assist::Surface;
use baocr_overlay::{Chip, Dock, Mark, PanelState, Presentation, Row, ViewInfo, ViewShape};
use serde_json::{json, Value};
use std::cell::RefCell;
use std::path::PathBuf;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// The dump is rewritten at least this often, so a reader can tell the add-on is alive.
const HEARTBEAT_MS: u128 = 250;

fn chips_text(c: &[Chip]) -> String {
    c.iter().map(|c| c.text.as_str()).collect::<Vec<_>>().join(" ")
}

/// One line of text for a row, as a reader of the card would take it.
pub fn row_text(row: &Row) -> String {
    match row {
        Row::Info { label, value } => format!("{label} {value}"),
        Row::Chips { label, chips } => format!("{label} {}", chips_text(chips)),
        Row::Note(t) => t.clone(),
        Row::Matrix { heading, cols, rows } => {
            let body = rows.iter().map(|r| format!("{}: {}", r.label.text, chips_text(&r.cells))).collect::<Vec<_>>().join(" | ");
            format!("{heading} [{}] {body}", chips_text(cols))
        }
        Row::Legend { chips, text } => format!("{} {text}", chips_text(chips)),
        Row::Fits(list) => list.iter().map(|f| format!("{} {} {} {}", f.badge, f.name, chips_text(&f.tags), f.note)).collect::<Vec<_>>().join(" | "),
        Row::Verdict { label, text, .. } => format!("{label} {text}"),
        Row::Button { id, label, value, .. } => format!("{label} {value} <{id}>"),
        Row::Hint { label, value, .. } => format!("{label} {value}"),
        Row::Heading(t) => t.clone(),
        Row::Section { id, title, open, summary } => format!("{title} [{}] {summary} <{id}>", if *open { "open" } else { "folded" }),
        Row::Progress { label, done, total, text } => format!("{label} {done}/{} {text}", total.map_or("?".into(), |t| t.to_string())),
    }
}

fn card_json(c: &PanelState) -> Value {
    json!({
        "scene": c.scene,
        "title": c.title,
        "rows": c.rows.iter().map(row_text).collect::<Vec<_>>(),
        "close": c.close.is_some(),
        "pin": c.pin,
    })
}

/// What the surface was last told.
#[derive(Default, Clone)]
pub struct Shown {
    pub main: Option<PanelState>,
    pub ingame: Option<PanelState>,
    pub marks: usize,
    pub dock: Option<Dock>,
    pub locked: bool,
}

fn view_json(v: &ViewInfo) -> Value {
    let fold = match v.shape {
        ViewShape::Nothing => "hidden",
        ViewShape::Card => "open",
        ViewShape::Tab => "folded",
        ViewShape::Strip => "sliding",
    };
    json!({
        "fold": fold,
        "slide_milli": v.slide_milli,
        "rect": v.rect,
        "hits": v.hits.iter().map(|(id, r)| json!({"id": id, "rect": r})).collect::<Vec<_>>(),
    })
}

/// The part of the dump that says what is shown (no clocks): a change of it is a change of state.
pub fn state_json(s: &Shown, view: Option<&ViewInfo>) -> Value {
    let battle = s.main.as_ref().is_some_and(|m| m.scene == baocr_extract::scene::Scene::Battle.label());
    json!({
        "main": s.main.as_ref().map(card_json),
        "ingame": s.ingame.as_ref().map(card_json),
        "marks": s.marks,
        "dock": view.map(|v| v.dock).or(s.dock).map(|d| json!({"anchor": d.anchor.code(), "presentation": d.presentation.map(Presentation::code)})),
        "view": view.map(view_json),
        "battle": battle,
        "locked": s.locked,
    })
}

/// The full document: the state plus a sequence number and clocks (monotonic ms since the add-on
/// started, wall-clock ms since the epoch, and the wall-clock time the state last changed).
pub fn document(state: Value, seq: u64, mono_ms: u128, unix_ms: u128, changed_unix_ms: u128) -> Value {
    let mut doc = state;
    doc["seq"] = json!(seq);
    doc["mono_ms"] = json!(mono_ms);
    doc["unix_ms"] = json!(unix_ms);
    doc["changed_unix_ms"] = json!(changed_unix_ms);
    doc
}

struct Writer {
    path: PathBuf,
    started: Instant,
    seq: u64,
    last_state: Option<Value>,
    changed_unix_ms: u128,
    last_write: Option<Instant>,
}

pub struct DumpSurface<S: Surface> {
    inner: S,
    shown: RefCell<Shown>,
    writer: RefCell<Writer>,
}

fn unix_ms() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis())
}

impl<S: Surface> DumpSurface<S> {
    pub fn new(inner: S, path: PathBuf) -> Self {
        let s = DumpSurface { inner, shown: RefCell::default(), writer: RefCell::new(Writer { path, started: Instant::now(), seq: 0, last_state: None, changed_unix_ms: unix_ms(), last_write: None }) };
        s.flush();
        s
    }

    /// Write the dump if the state changed or the heartbeat is due. Atomic: a temp file, then a rename.
    fn flush(&self) {
        let view = self.inner.view();
        let state = state_json(&self.shown.borrow(), view.as_ref());
        let mut w = self.writer.borrow_mut();
        let changed = w.last_state.as_ref() != Some(&state);
        if !changed && w.last_write.is_some_and(|t| t.elapsed().as_millis() < HEARTBEAT_MS) {
            return;
        }
        if changed {
            w.changed_unix_ms = unix_ms();
            w.seq += 1;
        }
        let doc = document(state.clone(), w.seq, w.started.elapsed().as_millis(), unix_ms(), w.changed_unix_ms);
        let tmp = w.path.with_extension("tmp");
        if std::fs::write(&tmp, doc.to_string()).is_ok() && std::fs::rename(&tmp, &w.path).is_ok() {
            w.last_write = Some(Instant::now());
        }
        w.last_state = Some(state);
    }
}

impl<S: Surface> Surface for DumpSurface<S> {
    fn main_card(&self, card: PanelState) {
        self.shown.borrow_mut().main = Some(card.clone());
        self.inner.main_card(card);
        self.flush();
    }
    fn ingame_card(&self, card: Option<PanelState>) {
        self.shown.borrow_mut().ingame = card.clone();
        self.inner.ingame_card(card);
        self.flush();
    }
    fn marks(&self, marks: Vec<Mark>) {
        self.shown.borrow_mut().marks = marks.len();
        self.inner.marks(marks);
        self.flush();
    }
    fn take_events(&self) -> Vec<String> {
        let events = self.inner.take_events();
        self.flush();
        events
    }
    fn set_dock(&self, dock: Dock) {
        self.shown.borrow_mut().dock = Some(dock);
        self.inner.set_dock(dock);
        self.flush();
    }
    fn lock_input(&self, on: bool) -> bool {
        let r = self.inner.lock_input(on);
        self.shown.borrow_mut().locked = r;
        self.flush();
        r
    }
    fn view(&self) -> Option<ViewInfo> {
        self.inner.view()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use baocr_overlay::{Anchor, Corner, Tone};

    fn card(scene: &str) -> PanelState {
        PanelState {
            scene: scene.into(),
            title: "t".into(),
            rows: vec![Row::Info { label: "방어".into(), value: "중장갑".into() }, Row::Chips { label: "적 방어".into(), chips: vec![Chip { text: "중장갑".into(), tone: Tone::HeavyArmor }] }],
            close: Some(("quit".into(), "x".into())),
            pin: Some(true),
        }
    }

    #[test]
    fn rows_become_one_text_line_each() {
        let c = card("편성");
        assert_eq!(row_text(&c.rows[0]), "방어 중장갑");
        assert_eq!(row_text(&c.rows[1]), "적 방어 중장갑");
    }

    #[test]
    fn the_state_names_cards_marks_dock_and_fold() {
        let s = Shown { main: Some(card("편성")), ingame: None, marks: 3, dock: Some(Dock { anchor: Anchor::Inside(Corner::TopRight), presentation: Some(Presentation::Bookmark) }), locked: false };
        let view = ViewInfo { shape: ViewShape::Tab, slide_milli: 0, rect: [1, 2, 30, 74], hits: vec![], dock: Dock { anchor: Anchor::Inside(Corner::TopRight), presentation: Some(Presentation::Bookmark) } };
        let v = state_json(&s, Some(&view));
        assert_eq!(v["main"]["scene"], "편성");
        assert_eq!(v["main"]["close"], true);
        assert_eq!(v["main"]["pin"], true);
        assert_eq!(v["main"]["rows"][1], "적 방어 중장갑");
        assert_eq!(v["ingame"], Value::Null);
        assert_eq!(v["marks"], 3);
        assert_eq!(v["dock"]["anchor"], "ir");
        assert_eq!(v["dock"]["presentation"], "bookmark");
        assert_eq!(v["view"]["fold"], "folded");
        assert_eq!(v["battle"], false);
    }

    #[test]
    fn the_battle_card_sets_the_battle_flag() {
        let s = Shown { main: Some(card(baocr_extract::scene::Scene::Battle.label())), ..Default::default() };
        assert_eq!(state_json(&s, None)["battle"], true);
    }

    #[test]
    fn the_document_adds_the_clocks_without_touching_the_state() {
        let st = state_json(&Shown::default(), None);
        let d = document(st.clone(), 7, 1234, 1_700_000_000_000, 1_699_999_999_000);
        assert_eq!(d["seq"], 7);
        assert_eq!(d["mono_ms"], 1234);
        assert_eq!(d["unix_ms"], 1_700_000_000_000u64);
        assert_eq!(d["changed_unix_ms"], 1_699_999_999_000u64);
        assert_eq!(d["marks"], st["marks"]);
    }
}
