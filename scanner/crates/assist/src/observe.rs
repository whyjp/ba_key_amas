//! The input side: what is on the game screen right now.
//!
//! An `Observer` turns a frame (and the OCR text read on it) into an `Observation`: the
//! screen, the mission the player picked (remembered across screens that do not name
//! it), the student cards in the 빠른 편성 grid, and who is in the squad. It knows
//! nothing about how any of that is shown.
use baocr_core::ocr_api::{Ocr, TextBox};
use baocr_extract::raid::{RaidRef, RaidTracker};
use baocr_extract::scene::{classify_in, squad_names_in, Layout, MissionRef, MissionTracker, Scene};
use crate::portrait;
use baocr_extract::{read_card_names_cached, unread_label_band, CardName, LabelCache};
use baocr_pixels::{quick_card_labels, quick_card_selected, quick_label_ink, quick_tab_is_striker, quick_thumb_cards};
use baocr_wikidb::{StudentBrief, Wiki};
use image::RgbImage;

/// What the screen shows.
#[derive(Debug, Clone, PartialEq)]
pub struct Observation {
    pub scene: Scene,
    /// The mission in play, remembered from its 임무 정보 popup.
    pub mission: Option<MissionRef>,
    /// The raid encounter in play (총력전, 모의 총력전, 제약해제결전), remembered from the screens that name it.
    pub raid: Option<RaidRef>,
    /// The student cards whose name is whole in the 빠른 편성 grid.
    pub cards: Vec<CardName>,
    /// The squad being edited, strikers and specials in slot order, as far as it has been seen.
    pub squad: Vec<i64>,
    /// Who stands in each of the six slots of the squad strip (four strikers, two specials): the strikers' portraits are
    /// told by their pictures (the slots carry no name), the specials by the names under the popup.
    pub slots: [Option<i64>; 6],
    /// The kind of student the grid's active tab lists: "Main" (STRIKER) or "Support" (SPECIAL).
    pub grid_kind: Option<&'static str>,
    /// The screen ratio of the frame this was read from: where marks are placed depends on it.
    pub layout: Layout,
}

impl Observation {
    pub fn new(scene: Scene) -> Observation {
        Observation { scene, mission: None, raid: None, cards: Vec::new(), squad: Vec::new(), slots: [None; 6], grid_kind: None, layout: Layout::Ratio43 }
    }

    /// The 빠른 편성 popup is up but its grid shows no names (compact view).
    pub fn grid_without_names(&self) -> bool {
        self.scene == Scene::QuickFormation && self.cards.is_empty()
    }
}

/// A frame with at most this many text boxes, right after the main menu, is the idle main menu.
const IDLE_MAX_BOXES: usize = 1;

/// Name labels are 12 to 14 px text, so a card's name is now and then not read for a frame or two while nothing on
/// screen has changed. Taking its icons down for that frame makes the icons blink. A card that was read at a place
/// and is missing from an otherwise identical reading is kept for a few readings; any scroll or tab change (the other
/// cards no longer sit where they did) forgets everything at once.
#[derive(Debug, Default)]
struct CardMemory {
    cards: Vec<CardName>,
    kind: Option<&'static str>,
    /// How many readings in a row the kept cards have been carried over.
    age: u8,
}

/// Readings a missing card is carried over for.
const CARD_MEMORY_READINGS: u8 = 2;
/// Two labels at the same place, in frame pixels.
const SAME_PLACE: f32 = 4.0;

impl CardMemory {
    fn steady(&mut self, fresh: Vec<CardName>, kind: Option<&'static str>) -> Vec<CardName> {
        let same = |a: &CardName, b: &CardName| (a.x - b.x).abs() < SAME_PLACE && (a.y0 - b.y0).abs() < SAME_PLACE;
        let unmoved = self.kind == kind && !fresh.is_empty() && fresh.iter().filter(|f| self.cards.iter().any(|p| same(f, p))).count() * 2 >= fresh.len();
        let missing: Vec<CardName> = if unmoved { self.cards.iter().filter(|p| !fresh.iter().any(|f| same(f, p))).cloned().collect() } else { Vec::new() };
        let mut cards = fresh;
        if missing.is_empty() {
            self.age = 0;
        } else if self.age < CARD_MEMORY_READINGS {
            self.age += 1;
            cards.extend(missing);
        } else {
            self.age = 0;
        }
        self.cards = cards.clone();
        self.kind = kind;
        cards
    }
}

pub struct Observer<'a> {
    ocr: &'a dyn Ocr,
    students: &'a [StudentBrief],
    tracker: MissionTracker,
    raid_tracker: RaidTracker,
    squad: Vec<i64>,
    /// The last screen that was recognised (the idle lobby is told from it: it has no text of its own).
    last: Scene,
    cards: CardMemory,
    /// The grids read lately (the last three): where each card was, a fingerprint of its label (or of its picture when the
    /// names are hidden), and the names read. While a frame shows the same as one of them, the grid was not scrolled and
    /// no tab was pressed, so those names are still right and only the squad can have changed. Several are kept because a
    /// click shakes the picture for a moment: the frame of that moment is a grid of its own, and the settled one that
    /// follows is the one from before.
    grids: Vec<KnownGrid>,
    /// The fingerprint of the last grid that was not one of those, seen by `grid_settled`.
    pending: Option<Vec<(i32, i32, u64)>>,
    /// The grid read last and when: a scroll slows down for a while after it has nearly stopped, moving the labels a pixel
    /// or two at a time, and each of those frames is a new grid to the fingerprints. The grid is read once the labels have
    /// nearly stopped; for a moment afterwards a frame whose labels are within a few pixels of those is that grid.
    tail: Option<Tail>,
    /// Names read from labels, by the ink of the label: a card that is still on screen after a scroll is not read again.
    label_cache: LabelCache,
    /// The four striker slots as they looked when last told, and whom: a slot that looks the same is not told again (telling a
    /// slot, 75 probes against 270 templates, takes 20 ms).
    slot_memory: std::cell::RefCell<[Option<(Vec<u8>, Option<i64>)>; 4]>,
    /// Whether each student is a striker (true) or a special, and each one's name: what the portraits are matched among.
    strikers: std::collections::HashMap<i64, bool>,
    names: std::collections::HashMap<i64, String>,
}

impl<'a> Observer<'a> {
    pub fn new(ocr: &'a dyn Ocr, students: &'a [StudentBrief]) -> Observer<'a> {
        Observer { ocr, students, tracker: MissionTracker::default(), raid_tracker: RaidTracker::default(), squad: Vec::new(), last: Scene::Unknown, cards: CardMemory::default(), grids: Vec::new(), pending: None, tail: None, label_cache: LabelCache::new(), slot_memory: Default::default(), strikers: students.iter().map(|s| (s.id, s.squad_type == "Main")).collect(), names: students.iter().map(|s| (s.id, s.name.clone())).collect() }
    }

    /// Forget the mission and the squad (the game was closed: what was known is not true any more).
    pub fn forget(&mut self) {
        self.tracker = MissionTracker::default();
        self.raid_tracker = RaidTracker::default();
        self.squad.clear();
        self.last = Scene::Unknown;
        self.cards = CardMemory::default();
        self.grids.clear();
        self.tail = None;
        self.label_cache.clear();
        *self.slot_memory.borrow_mut() = Default::default();
    }

    /// `boxes`: the frame's free-text OCR. `fallback`: the screen to assume when the text
    /// names none (the student list and profile are told apart by pixels, not text).
    pub fn observe(&mut self, img: &RgbImage, boxes: &[TextBox], fallback: Scene) -> Observation {
        let layout = Layout::of_frame(img.width(), img.height());
        let mut scene = classify_in(boxes, None, layout);
        if scene == Scene::Unknown {
            scene = fallback;
        }
        // The 빠른 편성 popup is modal: while it is up, the text may fail to name it for a frame (a glow on its title,
        // the cursor over it), or the screen behind it (the squad screen) may be named instead. Read as "the popup is
        // gone", that takes every icon down and puts them back a moment later. The active tab's red fill says the popup
        // is still there.
        if matches!(scene, Scene::Unknown | Scene::Squad) && quick_tab_is_striker(img).is_some() {
            scene = Scene::QuickFormation;
        }
        // After the main menu the game hides every menu when the player is idle: art and no text.
        if scene == Scene::Unknown && boxes.len() <= IDLE_MAX_BOXES && matches!(self.last, Scene::Lobby | Scene::LobbyIdle) {
            scene = Scene::LobbyIdle;
        }
        if scene != Scene::Unknown {
            self.last = scene;
        }
        let mission = self.tracker.update_in(scene, boxes, layout).cloned();
        let raid = self.raid_tracker.update_in(scene, boxes, layout).cloned();
        // A squad belongs to the mission or edit session in progress: leaving for a screen that
        // has nothing to do with one forgets it, so the next mission never inherits it.
        if !matches!(scene, Scene::Squad | Scene::QuickFormation | Scene::MissionMap | Scene::MissionInfo | Scene::EnemyInfo | Scene::RaidFormation(_) | Scene::AlertPopup | Scene::Unknown) {
            self.squad.clear();
        }
        let mut obs = Observation::new(scene);
        obs.layout = layout;
        obs.mission = mission;
        obs.raid = raid;
        match scene {
            Scene::Squad => {
                self.squad = squad_names_in(boxes, layout).iter().filter_map(|(n, special)| Wiki::match_student_as(self.students, n, Some(if *special { "Support" } else { "Main" }))).map(|s| s.id).collect();
            }
            Scene::QuickFormation => {
                obs.grid_kind = quick_tab_is_striker(img).map(|striker| if striker { "Main" } else { "Support" });
                let snapshot = grid_snapshot(img);
                let in_tail = self.tail.as_ref().is_some_and(|tail| tail.kind == obs.grid_kind && tail.holds(&snapshot));
                if in_tail {
                    // the labels are still settling a pixel or two from where they were read: the same cards, moved a little
                    let tail = self.tail.as_ref().expect("in the tail");
                    obs.cards = tail.cards.iter().map(|c| tail.moved(c, &snapshot)).collect();
                    self.cards.cards = obs.cards.clone();
                    self.cards.kind = obs.grid_kind;
                } else if let Some(known) = self.grids.iter().find(|g| g.kind == obs.grid_kind && !g.cards.is_empty() && same_grid(&g.snapshot, &snapshot)) {
                    // the same grid as one read lately: the names are known, only who is selected is looked at again
                    obs.cards = known.cards.clone();
                    self.cards.cards = obs.cards.clone();
                    self.cards.kind = obs.grid_kind;
                } else {
                    // with the names hidden (the grid's thumbnail view) the cards are told by their pictures
                    let fresh = if quick_card_labels(img).is_empty() { self.thumb_cards(img, obs.grid_kind) } else { read_card_names_cached(self.ocr, img, boxes, &mut self.label_cache) };
                    trace("  cards read");
                    obs.cards = self.cards.steady(fresh, obs.grid_kind);
                    self.tail = Some(Tail { at: std::time::Instant::now(), places: snapshot.iter().map(|g| (g.0, g.1)).collect(), cards: obs.cards.clone(), kind: obs.grid_kind });
                    self.grids.push(KnownGrid { snapshot, cards: obs.cards.clone(), kind: obs.grid_kind });
                    if self.grids.len() > 3 {
                        self.grids.remove(0);
                    }
                }
                for c in &obs.cards {
                    if let Some(s) = Wiki::match_student_as(self.students, &c.text, obs.grid_kind) {
                        // a student already in the squad keeps their slot; only changes move anyone
                        let at = self.squad.iter().position(|id| *id == s.id);
                        // the selection band is placed from the name label, which lies 106 px under a bare card's top
                        let label_y = c.y0.round() as i32 + if c.bare { 106 } else { 0 };
                        match (quick_card_selected(img, c.x.round() as i32, label_y), at) {
                            (true, None) => self.squad.push(s.id),
                            (false, Some(i)) => {
                                self.squad.remove(i);
                            }
                            _ => {}
                        }
                    }
                }
                // the specials' names under the popup are still readable: they say who holds those slots
                let specials: Vec<i64> = squad_names_in(boxes, layout).iter().filter(|(_, special)| *special).filter_map(|(n, _)| Wiki::match_student_as(self.students, n, Some("Support"))).map(|s| s.id).collect();
                if !specials.is_empty() {
                    let students = self.students;
                    self.squad.retain(|id| students.iter().find(|s| s.id == *id).is_none_or(|s| s.squad_type == "Main"));
                    self.squad.extend(specials);
                }
            }
            _ => {}
        }
        if scene != Scene::QuickFormation {
            self.grids.clear();
        }
        trace("  cards and squad done");
        obs.slots = self.slots(img, scene, layout);
        trace("  slots done");
        obs.squad = if scene == Scene::QuickFormation { obs.slots.iter().flatten().copied().collect() } else { self.squad.clone() };
        obs
    }

    /// The cards of the grid's thumbnail view, each told by its picture (only what is believed is named).
    /// The part of the frame (x, y, width, height) whose text the labels not read yet need, or None when every label is known.
    pub fn unread_band(&self, img: &RgbImage) -> Option<(u32, u32, u32, u32)> {
        unread_label_band(img, &self.label_cache)
    }

    /// Whether the last screen read was the 빠른 편성 popup.
    pub fn in_popup(&self) -> bool {
        self.last == Scene::QuickFormation
    }

    /// Whether the frame is the 빠른 편성 popup, already being read, on its STRIKER tab. There the text of the grid part
    /// of the frame is all that is needed (the specials' names under the popup, which the rest of the frame holds, do not change).
    pub fn striker_popup(&self, img: &RgbImage) -> bool {
        self.last == Scene::QuickFormation && quick_tab_is_striker(img) == Some(true)
    }

    /// What the 빠른 편성 popup's grid looks like in this frame, worked out once (the fingerprint of every label is the
    /// costly part): the one read lately (`Known`: the squad is all that can have changed, the text need not be read again),
    /// a new one that has held still for two looks, with each card within a few pixels of where it was (`Settled`: a click
    /// shakes the picture for a moment and a scroll's last pixels come slowly, and reading those moments costs 700 ms), or
    /// one that is still moving or the first of a kind (`Moving`: look again). Outside the popup it is `Settled`.
    pub fn grid_state(&mut self, img: &RgbImage) -> GridState {
        if self.last != Scene::QuickFormation || quick_tab_is_striker(img).is_none() {
            self.pending = None;
            return GridState::Settled;
        }
        let now = grid_snapshot(img);
        if self.grids.iter().any(|g| same_grid(&g.snapshot, &now)) || self.tail.as_ref().is_some_and(|tail| tail.holds(&now)) {
            self.pending = None;
            return GridState::Known;
        }
        let settled = self.pending.as_deref().is_some_and(|before| near_places(before.iter().map(|g| (g.0, g.1)), now.iter().map(|g| (g.0, g.1)), SETTLED_PX));
        self.pending = if settled { None } else { Some(now) };
        if settled {
            GridState::Settled
        } else {
            GridState::Moving
        }
    }

    /// Two students are costumes of one character.
    fn kin(&self, a: i64, b: i64) -> bool {
        match (self.names.get(&a), self.names.get(&b)) {
            (Some(x), Some(y)) => portrait::base_name(x) == portrait::base_name(y),
            _ => false,
        }
    }

    fn thumb_cards(&self, img: &RgbImage, kind: Option<&'static str>) -> Vec<CardName> {
        let Some(kind) = kind else { return Vec::new() };
        let striker = kind == "Main";
        let book = portrait::Book::bundled();
        let same = |a: i64, b: i64| self.names.get(&a) == self.names.get(&b);
        quick_thumb_cards(img)
            .into_iter()
            .filter_map(|card| {
                let m = book.identify(&portrait::probe(img, card.cx as f32, card.picture_y() as f32), &|id| self.strikers.get(&id) == Some(&striker), &same)?;
                let name = self.names.get(&m.id).filter(|_| m.trusted_with(&|a, b| self.kin(a, b)))?;
                Some(CardName { text: name.clone(), x: card.cx as f32, y0: card.top() as f32, bare: true })
            })
            .collect()
    }

    /// The six slots of the strip. In the popup the strikers' pictures say who they are; where a picture is not
    /// believed, the names read elsewhere fill the slot in order, as before.
    fn slots(&self, img: &RgbImage, scene: Scene, layout: Layout) -> [Option<i64>; 6] {
        let is_striker = |id: &i64| self.strikers.get(id) == Some(&true);
        let named_strikers: Vec<i64> = self.squad.iter().copied().filter(is_striker).collect();
        let specials: Vec<i64> = self.squad.iter().copied().filter(|id| !is_striker(id)).collect();
        let mut slots = [None; 6];
        // slots the game shows as EMPTY stay empty: no name read elsewhere may fill them
        let mut empty = [false; 6];
        if scene == Scene::QuickFormation {
            let book = portrait::Book::bundled();
            let same = |a: i64, b: i64| self.names.get(&a) == self.names.get(&b);
            for (k, slot) in slots.iter_mut().enumerate().take(4) {
                let (cx, cy) = portrait::slot_centre(k, layout);
                if portrait::slot_is_empty(img, cx, cy) {
                    empty[k] = true;
                    self.slot_memory.borrow_mut()[k] = None;
                    continue;
                }
                let sample = portrait::patch_sample(img, cx, cy);
                if let Some((was, who)) = self.slot_memory.borrow()[k].as_ref() {
                    if portrait::sample_distance(was, &sample) < SAME_SLOT {
                        *slot = *who;
                        continue;
                    }
                }
                *slot = book.identify(&portrait::probe(img, cx, cy), &|id| self.strikers.get(&id) == Some(&true), &same).filter(|m| m.trusted_with(&|a, b| self.kin(a, b))).map(|m| m.id);
                self.slot_memory.borrow_mut()[k] = Some((sample, *slot));
            }
        }
        let seen: Vec<i64> = slots.iter().flatten().copied().collect();
        let mut rest = named_strikers.into_iter().filter(|id| !seen.contains(id));
        for (k, slot) in slots.iter_mut().enumerate().take(4) {
            if slot.is_none() && !empty[k] {
                *slot = rest.next();
            }
        }
        for (k, id) in specials.into_iter().take(2).enumerate() {
            slots[4 + k] = Some(id);
        }
        slots
    }
}

/// `BAOCR_TRACE=1`: a line with the wall clock in milliseconds (the add-on's loop prints the same kind).
fn trace(what: &str) {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if *ON.get_or_init(|| std::env::var_os("BAOCR_TRACE").is_some()) {
        let ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis());
        eprintln!("trace {ms} {what}");
    }
}

/// See [`Observer::grid_state`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GridState {
    Known,
    Settled,
    Moving,
}

/// How far (mean difference of the coarse copies) a slot's picture may be from when it was last told and still be that student.
const SAME_SLOT: f32 = 3.0;

/// How far a label may be from its place a look ago, in frame pixels, for the grid to count as held still.
const SETTLED_PX: i32 = 6;
/// How far from the places it was read at a label may be and still belong to the grid read last, and for how long after that
/// read that is believed (the tail of a scroll).
const TAIL_PX: i32 = 6;
const TAIL_MS: u128 = 900;

/// The grid read last, with the places of its cards.
struct Tail {
    at: std::time::Instant,
    places: Vec<(i32, i32)>,
    cards: Vec<CardName>,
    kind: Option<&'static str>,
}

impl Tail {
    /// Whether `now` shows the cards of this grid within the tail's reach: as many cards at places within TAIL_PX.
    fn holds(&self, now: &[(i32, i32, u64)]) -> bool {
        self.at.elapsed().as_millis() < TAIL_MS && near_places(self.places.iter().copied(), now.iter().map(|g| (g.0, g.1)), TAIL_PX)
    }

    /// `card` at the place its label has now.
    fn moved(&self, card: &CardName, now: &[(i32, i32, u64)]) -> CardName {
        let top = if card.bare { 79 } else { 0 };
        let (x, y) = (card.x as i32, card.y0 as i32 + top);
        match now.iter().min_by_key(|g| (g.0 - x).abs() + (g.1 - y).abs()).filter(|g| (g.0 - x).abs() <= TAIL_PX && (g.1 - y).abs() <= TAIL_PX) {
            Some(g) => CardName { x: g.0 as f32, y0: (g.1 - top) as f32, ..card.clone() },
            None => card.clone(),
        }
    }
}

/// Whether two lists of places are the same number of cards, each within `px` of its pair.
fn near_places(a: impl Iterator<Item = (i32, i32)>, b: impl Iterator<Item = (i32, i32)>, px: i32) -> bool {
    let (a, b): (Vec<_>, Vec<_>) = (a.collect(), b.collect());
    !a.is_empty() && a.len() == b.len() && a.iter().zip(&b).all(|(p, q)| (p.0 - q.0).abs() <= px && (p.1 - q.1).abs() <= px)
}

/// A grid that was read: how it looked and the names found on it.
struct KnownGrid {
    snapshot: Vec<(i32, i32, u64)>,
    cards: Vec<CardName>,
    kind: Option<&'static str>,
}

/// Whether `now` is the grid `known` was read from: every card in it is at a place of the known ones, nearly all with the
/// same label, and most of them are there (a card can be covered for a moment, by a toast or the pointer, while the squad
/// changes; a scroll or a tab changes every place or label at once).
fn same_grid(known: &[(i32, i32, u64)], now: &[(i32, i32, u64)]) -> bool {
    if now.is_empty() || now.len() * 10 < known.len() * 6 {
        return false;
    }
    // A label that looks different at the place of a known one is the card under the pointer (it lights up) or one a toast covers;
    // a few of those do not make another grid, while a scroll or a tab changes half the labels or all of them.
    let changed = now.iter().filter(|c| !known.contains(c)).count();
    changed <= (now.len() / 4).max(2) && now.iter().all(|c| known.contains(c) || known.iter().any(|k| (k.0, k.1) == (c.0, c.1)))
}

/// The cards of the grid as one frame shows them: place and fingerprint. A card's label is a thresholded picture, so its
/// bytes are the same in every frame of the same view; with the names hidden a coarse, quantised average of the picture
/// stands in for it (a row scrolled in has other pictures at the same places).
pub fn grid_snapshot(img: &RgbImage) -> Vec<(i32, i32, u64)> {
    use std::hash::{Hash, Hasher};
    let labels = quick_card_labels(img);
    if !labels.is_empty() {
        return labels
            .iter()
            .map(|l| {
                let mut h = std::collections::hash_map::DefaultHasher::new();
                quick_label_ink(img, l).as_raw().hash(&mut h);
                (l.cx, l.y0, h.finish())
            })
            .collect();
    }
    quick_thumb_cards(img)
        .iter()
        .map(|c| {
            let mut h = std::collections::hash_map::DefaultHasher::new();
            let (cx, cy) = (c.cx, c.picture_y());
            for gy in 0..4 {
                for gx in 0..4 {
                    let mut sum = [0u32; 3];
                    for y in 0..10 {
                        for x in 0..10 {
                            let (px, py) = ((cx - 20 + gx * 10 + x).clamp(0, img.width() as i32 - 1), (cy - 20 + gy * 10 + y).clamp(0, img.height() as i32 - 1));
                            let p = img.get_pixel(px as u32, py as u32).0;
                            for i in 0..3 {
                                sum[i] += p[i] as u32;
                            }
                        }
                    }
                    sum.map(|s| (s / 100) >> 5).hash(&mut h);
                }
            }
            (c.cx, c.bar_top, h.finish())
        })
        .collect()
}

/// Decides when the add-on is in a battle from one reading per frame, so a single odd frame
/// (a flash, a cut-in) does not flip it: two readings in a row are needed either way.
#[derive(Debug, Default)]
pub struct BattleGate {
    in_battle: bool,
    streak: u32,
}

const BATTLE_READINGS: u32 = 2;

impl BattleGate {
    pub fn in_battle(&self) -> bool {
        self.in_battle
    }

    /// Feed whether the latest frame looks like a battle. Returns the new state when it flipped.
    pub fn feed(&mut self, looks_like_battle: bool) -> Option<bool> {
        if looks_like_battle == self.in_battle {
            self.streak = 0;
            return None;
        }
        self.streak += 1;
        if self.streak < BATTLE_READINGS {
            return None;
        }
        self.streak = 0;
        self.in_battle = looks_like_battle;
        Some(looks_like_battle)
    }

    /// The game went away: whatever was happening is over.
    pub fn reset(&mut self) {
        *self = BattleGate::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct NoOcr;
    impl Ocr for NoOcr {
        fn boxes(&self, _: &RgbImage) -> anyhow::Result<Vec<TextBox>> {
            Ok(Vec::new())
        }
        fn line(&self, _: &RgbImage) -> anyhow::Result<(String, f32)> {
            Ok((String::new(), 0.0))
        }
        fn choose(&self, _: &RgbImage, _: &[String]) -> anyhow::Result<(usize, Vec<f32>)> {
            Ok((0, Vec::new()))
        }
    }

    fn boxes_of(name: &str) -> Vec<TextBox> {
        let path = format!("{}/../../../fixtures/screens/{name}.boxes.txt", env!("CARGO_MANIFEST_DIR"));
        std::fs::read_to_string(path)
            .unwrap()
            .lines()
            .filter_map(|l| {
                let (c, text) = l.split_once('\t')?;
                let n: Vec<f32> = c.split(',').filter_map(|v| v.parse().ok()).collect();
                (n.len() == 4).then(|| TextBox { x0: n[0], y0: n[1], x1: n[2], y1: n[3], text: text.to_string(), score: 1.0 })
            })
            .collect()
    }

    #[test]
    fn the_main_menu_that_hides_itself_when_idle_is_still_the_main_menu() {
        let ocr = NoOcr;
        let mut o = Observer::new(&ocr, &[]);
        let img = RgbImage::new(1280, 960);
        let (lobby, idle) = (boxes_of("lobby"), boxes_of("lobby-idle"));
        assert!(idle.is_empty() && !lobby.is_empty(), "fixtures: the idle screen has no text, the menu has");
        // with no menu seen before, a text-less frame is just unknown (a cutscene, a loading screen)
        assert_eq!(o.observe(&img, &idle, Scene::Unknown).scene, Scene::Unknown);
        assert_eq!(o.observe(&img, &lobby, Scene::Unknown).scene, Scene::Lobby);
        assert_eq!(o.observe(&img, &idle, Scene::Unknown).scene, Scene::LobbyIdle);
        assert_eq!(o.observe(&img, &idle, Scene::Unknown).scene, Scene::LobbyIdle, "stays while the art is shown");
        // a click brings the menu back
        assert_eq!(o.observe(&img, &lobby, Scene::Unknown).scene, Scene::Lobby);
        // after another screen is recognised, an empty frame is not the idle menu
        o.observe(&img, &lobby, Scene::StudentList);
        o.observe(&img, &[], Scene::StudentList);
        assert_eq!(o.observe(&img, &idle, Scene::Unknown).scene, Scene::Unknown);
        // forgetting the game forgets the menu
        o.observe(&img, &lobby, Scene::Unknown);
        o.forget();
        assert_eq!(o.observe(&img, &idle, Scene::Unknown).scene, Scene::Unknown);
    }

    #[test]
    fn the_raid_encounter_follows_the_player_from_the_boss_list_to_the_formation_and_is_dropped_for_a_mission() {
        let ocr = NoOcr;
        let mut o = Observer::new(&ocr, &[]);
        let img = RgbImage::new(1280, 960);
        let mut see = |file: &str| o.observe(&img, &boxes_of(file), Scene::Unknown);
        let list = see("raid-mock-m11");
        assert_eq!((list.scene, list.mission.is_none()), (Scene::MockRaidBosses, true));
        assert_eq!(list.raid.as_ref().map(|r| (r.boss.as_str(), r.words.armors.len())), Some(("드럼통게", 2)));
        let room = see("raid-mock-room-info");
        assert_eq!(room.raid.as_ref().map(|r| (r.boss.as_str(), r.difficulty)), Some(("시로&쿠로", Some(0))), "another boss's popup replaces the list's");
        let formation = see("raid-formation-empty");
        assert_eq!(formation.scene, Scene::RaidFormation(baocr_extract::scene::RaidMode::MockTotalAssault));
        assert_eq!(formation.raid, room.raid, "the formation names nothing and keeps the room");
        let mission = see("mission-info");
        assert!(mission.raid.is_none() && mission.mission.is_some(), "a mission screen forgets the raid");
        assert!(see("raid-formation-empty").mission.is_none(), "and a raid screen the mission");
    }

    #[test]
    fn the_popup_stays_the_popup_when_its_title_is_not_read_for_a_frame() {
        /// Reads nothing, but scores every name it is offered (as a real model does), so the card reading can run.
        struct Blank;
        impl Ocr for Blank {
            fn boxes(&self, _: &RgbImage) -> anyhow::Result<Vec<TextBox>> {
                Ok(Vec::new())
            }
            fn line(&self, _: &RgbImage) -> anyhow::Result<(String, f32)> {
                Ok((String::new(), 0.0))
            }
            fn choose(&self, _: &RgbImage, names: &[String]) -> anyhow::Result<(usize, Vec<f32>)> {
                Ok((0, vec![f32::NEG_INFINITY; names.len()]))
            }
        }
        let ocr = Blank;
        let mut o = Observer::new(&ocr, &[]);
        let popup = image::open(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../fixtures/screens/quick-formation.png")).expect("fixture").to_rgb8();
        // no text read at all (the title was missed): the red tab still says "popup"
        assert_eq!(o.observe(&popup, &[], Scene::Unknown).scene, Scene::QuickFormation);
        // a frame without the popup is not mistaken for it
        let lobby = RgbImage::new(1280, 960);
        assert_eq!(o.observe(&lobby, &[], Scene::Unknown).scene, Scene::Unknown);
    }

    #[test]
    fn a_card_missed_for_a_frame_keeps_its_icons_but_a_scroll_forgets_every_card() {
        let card = |name: &str, x: f32, y0: f32| CardName { text: name.into(), x, y0, bare: false };
        let (a, b, c) = (card("a", 100.0, 400.0), card("b", 210.0, 400.0), card("c", 320.0, 400.0));
        let mut m = CardMemory::default();
        assert_eq!(m.steady(vec![a.clone(), b.clone(), c.clone()], Some("Main")).len(), 3);
        // c is missing from two identical readings in a row: still there
        assert_eq!(m.steady(vec![a.clone(), b.clone()], Some("Main")).len(), 3);
        assert_eq!(m.steady(vec![a.clone(), b.clone()], Some("Main")).len(), 3);
        // missing for a third time: really gone
        assert_eq!(m.steady(vec![a.clone(), b.clone()], Some("Main")).len(), 2);
        assert_eq!(m.steady(vec![a.clone(), b.clone()], Some("Main")).len(), 2, "and it does not come back by itself");
        // a scroll moves every card: nothing from before is kept
        let mut m = CardMemory::default();
        m.steady(vec![a.clone(), b.clone(), c.clone()], Some("Main"));
        let scrolled = vec![card("d", 100.0, 300.0), card("e", 210.0, 300.0)];
        assert_eq!(m.steady(scrolled.clone(), Some("Main")), scrolled);
        // so does the other tab
        let mut m = CardMemory::default();
        m.steady(vec![a.clone(), b.clone(), c], Some("Main"));
        assert_eq!(m.steady(vec![a, b], Some("Support")).len(), 2);
    }

    #[test]
    fn two_readings_in_a_row_enter_and_leave() {
        let mut g = BattleGate::default();
        let seq = [false, true, false, true, true, true, false, true, false, false, false];
        let flips: Vec<Option<bool>> = seq.iter().map(|b| g.feed(*b)).collect();
        assert_eq!(flips, vec![None, None, None, None, Some(true), None, None, None, None, Some(false), None]);
        assert!(!g.in_battle());
    }

    #[test]
    fn a_lone_odd_frame_does_not_flip_it() {
        let mut g = BattleGate::default();
        g.feed(true);
        g.feed(true);
        assert!(g.in_battle());
        assert_eq!(g.feed(false), None);
        assert_eq!(g.feed(true), None);
        assert!(g.in_battle());
        g.reset();
        assert!(!g.in_battle());
    }

    fn fixture(name: &str) -> RgbImage {
        image::open(format!("{}/../../../fixtures/screens/{name}.png", env!("CARGO_MANIFEST_DIR"))).unwrap().to_rgb8()
    }

    #[test]
    fn the_grid_looks_the_same_when_only_the_squad_changed_and_different_when_it_was_scrolled_or_shown_by_pictures() {
        // the same 빠른 편성 view with the fourth striker taken out of the squad and put back: the cards did not move
        let (removed, readded) = (grid_snapshot(&fixture("quick-formation-empty-slot")), grid_snapshot(&fixture("quick-formation-readded")));
        assert!(!removed.is_empty());
        assert_eq!(removed, readded, "a squad change leaves the grid's fingerprint alone");
        // another recording of the view (another scroll position and squad) and the thumbnail view differ
        assert_ne!(removed, grid_snapshot(&fixture("quick-formation")));
        let thumbs = grid_snapshot(&fixture("quick-thumbs"));
        assert!(!thumbs.is_empty() && thumbs != removed);
        assert_eq!(thumbs, grid_snapshot(&fixture("quick-thumbs")), "the fingerprint of one frame is stable");
        // a card covered for a moment is still the same grid; a grid of other cards is not
        assert!(same_grid(&removed, &removed[1..]));
        assert!(!same_grid(&removed, &removed[..3]), "a few cards of many is not enough");
        assert!(!same_grid(&removed, &thumbs) && !same_grid(&thumbs, &removed));
        assert!(!same_grid(&removed, &[]));
        // a card or two that look different at their places (one lights up under the pointer) are the same grid; half the labels
        // changed at the same places is a scroll by a row (or a tab) and is another grid
        let lit = |n: usize| -> Vec<(i32, i32, u64)> { removed.iter().enumerate().map(|(i, g)| if i < n { (g.0, g.1, g.2 ^ 0xFFFF) } else { *g }).collect() };
        assert!(same_grid(&removed, &lit(2)));
        assert!(!same_grid(&removed, &lit(removed.len() / 2)));
    }
}
