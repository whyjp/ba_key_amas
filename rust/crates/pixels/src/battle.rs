//! The battle HUD, told apart from every menu by fixed marks of the 4:3 layout. No OCR.
//!
//! The live combat screen keeps three things in place whatever the stage: the stage tag on the
//! top left (a navy plate), the cost gauge at the bottom right (a navy rail with cyan cells) and
//! the AUTO button in the bottom right corner (yellow lettering). Two of the three must show, so
//! one of them hidden by a window or an effect does not hide the battle.
//!
//! Limits, from the one real frame available and from how the marks are built:
//! - the frame must be the 1280x960 game client (the add-on refuses any other aspect before it
//!   gets here), so 16:9 layouts are not recognised;
//! - the marks are drawn, not written, so the game's language does not matter, but a UI scale
//!   or a stage type with another HUD (a raid with its own bar, say) may move or lack them;
//! - a screen that darkens the HUD (pause dialog, a skill cut-in that hides it) reads as not
//!   battle, which is wanted: whatever opens there is a menu worth reading;
//! - a cut scene without the HUD reads as not battle.
use crate::{fraction, Rect};
use image::RgbImage;

/// The navy plate with the stage name ("Normal 19-4"), inside its edges.
const TAG_ROI: Rect = (4, 24, 100, 46);
/// Rows through the cost gauge's rail and the columns it spans.
const GAUGE_ROWS: [i32; 5] = [923, 926, 929, 932, 935];
const GAUGE_X: (i32, i32) = (828, 1144);
/// Rows just above and below the rail, which must not look like it (a rail is a thin band, not a dark screen).
const GAUGE_EDGE_ROWS: [i32; 2] = [915, 944];
/// The AUTO button's lettering.
const AUTO_ROI: Rect = (1180, 898, 1250, 934);

const TAG_MIN: f64 = 0.56;
const GAUGE_MIN: f64 = 0.75;
const GAUGE_EDGE_MAX: f64 = 0.3;
const AUTO_MIN: f64 = 0.15;

/// How much of each mark of the battle HUD a frame shows (a fraction of its pixels, 0..1).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BattleHud {
    pub tag: f64,
    pub gauge: f64,
    pub auto: f64,
}

impl BattleHud {
    pub fn marks(&self) -> usize {
        [self.tag >= TAG_MIN, self.gauge >= GAUGE_MIN, self.auto >= AUTO_MIN].into_iter().filter(|m| *m).count()
    }

    /// Two of the three marks.
    pub fn is_battle(&self) -> bool {
        self.marks() >= 2
    }
}

fn navy_plate([r, g, b]: [u8; 3]) -> bool {
    r <= 30 && (30..=70).contains(&g) && (60..=110).contains(&b) && b as i32 > g as i32 + 20
}

fn gauge_rail([r, g, b]: [u8; 3]) -> bool {
    let rail = r <= 60 && g <= 70 && (45..=100).contains(&b) && b > g;
    let cell = r <= 90 && g >= 130 && b >= 190;
    rail || cell
}

fn auto_yellow([r, g, b]: [u8; 3]) -> bool {
    r >= 200 && g >= 170 && b <= 90
}

fn gauge_fraction(img: &RgbImage) -> f64 {
    let row = |y: &i32| fraction(img, (GAUGE_X.0, *y, GAUGE_X.1, y + 1), gauge_rail);
    let edge = GAUGE_EDGE_ROWS.iter().map(row).fold(0.0, f64::max);
    if edge > GAUGE_EDGE_MAX {
        return 0.0;
    }
    GAUGE_ROWS.iter().map(row).sum::<f64>() / GAUGE_ROWS.len() as f64
}

pub fn battle_hud(img: &RgbImage) -> BattleHud {
    let gauge = gauge_fraction(img);
    BattleHud { tag: fraction(img, TAG_ROI, navy_plate), gauge, auto: fraction(img, AUTO_ROI, auto_yellow) }
}

/// Is this frame the live combat screen?
pub fn looks_like_battle(img: &RgbImage) -> bool {
    battle_hud(img).is_battle()
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb;

    fn fixture(name: &str) -> RgbImage {
        let path = format!("{}/../../../fixtures/screens/{name}", env!("CARGO_MANIFEST_DIR"));
        image::open(&path).unwrap_or_else(|e| panic!("{path}: {e}")).to_rgb8()
    }

    fn paint(img: &mut RgbImage, r: Rect, c: [u8; 3]) {
        for y in r.1..r.3 {
            for x in r.0..r.2 {
                img.put_pixel(x as u32, y as u32, Rgb(c));
            }
        }
    }

    const SAND: [u8; 3] = [205, 150, 100];

    #[test]
    fn the_recorded_battle_shows_all_three_marks() {
        let hud = battle_hud(&fixture("battle.png"));
        assert_eq!(hud.marks(), 3, "{hud:?}");
        assert!(looks_like_battle(&fixture("battle.png")));
    }

    #[test]
    fn menus_are_not_battles() {
        for name in ["quick-formation.png", "quick-formation-live-a.png", "quick-formation-scrolled-a.png", "quick-formation-scrolled-b.png"] {
            let hud = battle_hud(&fixture(name));
            assert!(!hud.is_battle(), "{name}: {hud:?}");
        }
        for c in [[0, 0, 0], [255, 255, 255], [6, 45, 81], [27, 38, 66]] {
            let hud = battle_hud(&RgbImage::from_pixel(1280, 960, Rgb(c)));
            assert!(!hud.is_battle(), "a flat {c:?} frame: {hud:?}");
        }
    }

    #[test]
    fn one_mark_hidden_still_reads_as_battle_two_hidden_does_not() {
        let mut img = fixture("battle.png");
        paint(&mut img, AUTO_ROI, SAND);
        assert!(looks_like_battle(&img), "AUTO covered by a window");
        paint(&mut img, (0, 0, 130, 50), SAND);
        assert!(!looks_like_battle(&img), "only the gauge left");
        let mut img = fixture("battle.png");
        paint(&mut img, (800, 915, 1150, 945), SAND);
        assert!(looks_like_battle(&img), "gauge covered");
    }
}
