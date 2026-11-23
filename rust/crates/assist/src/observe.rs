//! The input side: what is on the game screen right now.
//!
//! An `Observer` turns a frame (and the OCR text read on it) into an `Observation`: the
//! screen, the mission the player picked (remembered across screens that do not name
//! it), the student cards in the 빠른 편성 grid, and who is in the squad. It knows
//! nothing about how any of that is shown.
use baocr_core::ocr_api::{Ocr, TextBox};
use baocr_extract::scene::{classify, squad_names, MissionRef, MissionTracker, Scene};
use baocr_extract::{read_card_names, CardName};
use baocr_pixels::{quick_card_selected, quick_tab_is_striker};
use baocr_wikidb::{StudentBrief, Wiki};
use image::RgbImage;

/// What the screen shows.
#[derive(Debug, Clone, PartialEq)]
pub struct Observation {
    pub scene: Scene,
    /// The mission in play, remembered from its 임무 정보 popup.
    pub mission: Option<MissionRef>,
    /// The student cards whose name is whole in the 빠른 편성 grid.
    pub cards: Vec<CardName>,
    /// The squad being edited, strikers and specials in slot order, as far as it has been seen.
    pub squad: Vec<i64>,
    /// The kind of student the grid's active tab lists: "Main" (STRIKER) or "Support" (SPECIAL).
    pub grid_kind: Option<&'static str>,
}

impl Observation {
    pub fn new(scene: Scene) -> Observation {
        Observation { scene, mission: None, cards: Vec::new(), squad: Vec::new(), grid_kind: None }
    }

    /// The 빠른 편성 popup is up but its grid shows no names (compact view).
    pub fn grid_without_names(&self) -> bool {
        self.scene == Scene::QuickFormation && self.cards.is_empty()
    }
}

pub struct Observer<'a> {
    ocr: &'a dyn Ocr,
    students: &'a [StudentBrief],
    tracker: MissionTracker,
    squad: Vec<i64>,
}

impl<'a> Observer<'a> {
    pub fn new(ocr: &'a dyn Ocr, students: &'a [StudentBrief]) -> Observer<'a> {
        Observer { ocr, students, tracker: MissionTracker::default(), squad: Vec::new() }
    }

    /// Forget the mission and the squad (the game was closed: what was known is not true any more).
    pub fn forget(&mut self) {
        self.tracker = MissionTracker::default();
        self.squad.clear();
    }

    /// `boxes`: the frame's free-text OCR. `fallback`: the screen to assume when the text
    /// names none (the student list and profile are told apart by pixels, not text).
    pub fn observe(&mut self, img: &RgbImage, boxes: &[TextBox], fallback: Scene) -> Observation {
        let mut scene = classify(boxes, None);
        if scene == Scene::Unknown {
            scene = fallback;
        }
        let mission = self.tracker.update(scene, boxes).cloned();
        // A squad belongs to the mission or edit session in progress: leaving for a screen that
        // has nothing to do with one forgets it, so the next mission never inherits it.
        if !matches!(scene, Scene::Squad | Scene::QuickFormation | Scene::MissionMap | Scene::MissionInfo | Scene::EnemyInfo | Scene::Unknown) {
            self.squad.clear();
        }
        let mut obs = Observation::new(scene);
        obs.mission = mission;
        match scene {
            Scene::Squad => {
                self.squad = squad_names(boxes).iter().filter_map(|(n, special)| Wiki::match_student_as(self.students, n, Some(if *special { "Support" } else { "Main" }))).map(|s| s.id).collect();
            }
            Scene::QuickFormation => {
                obs.grid_kind = quick_tab_is_striker(img).map(|striker| if striker { "Main" } else { "Support" });
                obs.cards = read_card_names(self.ocr, img, boxes);
                for c in &obs.cards {
                    if let Some(s) = Wiki::match_student_as(self.students, &c.text, obs.grid_kind) {
                        // a student already in the squad keeps their slot; only changes move anyone
                        let at = self.squad.iter().position(|id| *id == s.id);
                        match (quick_card_selected(img, c.x.round() as i32, c.y0.round() as i32), at) {
                            (true, None) => self.squad.push(s.id),
                            (false, Some(i)) => {
                                self.squad.remove(i);
                            }
                            _ => {}
                        }
                    }
                }
                // the specials' names under the popup are still readable: they say who holds those slots
                let specials: Vec<i64> = squad_names(boxes).iter().filter(|(_, special)| *special).filter_map(|(n, _)| Wiki::match_student_as(self.students, n, Some("Support"))).map(|s| s.id).collect();
                if !specials.is_empty() {
                    let students = self.students;
                    self.squad.retain(|id| students.iter().find(|s| s.id == *id).is_none_or(|s| s.squad_type == "Main"));
                    self.squad.extend(specials);
                }
            }
            _ => {}
        }
        obs.squad = self.squad.clone();
        obs
    }
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
}
