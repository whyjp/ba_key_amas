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

/// A furniture card in the cafe's furniture view, with the students that interact with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FurnitureCard {
    /// Top-left of the card's inside and its width (frame pixels): where the name tags go.
    pub x: i32,
    pub y: i32,
    pub w: i32,
    /// The students' names, in id order.
    pub students: Vec<String>,
}

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
    /// The furniture cards on screen that have students (cafe furniture view).
    pub furniture: Vec<FurnitureCard>,
}

impl Observation {
    pub fn new(scene: Scene) -> Observation {
        Observation { scene, mission: None, cards: Vec::new(), squad: Vec::new(), grid_kind: None, furniture: Vec::new() }
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

    /// `boxes`: the frame's free-text OCR. `fallback`: the screen to assume when the text
    /// names none (the student list and profile are told apart by pixels, not text).
    pub fn observe(&mut self, img: &RgbImage, boxes: &[TextBox], fallback: Scene) -> Observation {
        let mut scene = classify(boxes, None);
        if scene == Scene::Unknown {
            scene = fallback;
        }
        let mission = self.tracker.update(scene, boxes).cloned();
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
                        self.squad.retain(|id| *id != s.id);
                        if quick_card_selected(img, c.x.round() as i32, c.y0.round() as i32) {
                            self.squad.push(s.id);
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
