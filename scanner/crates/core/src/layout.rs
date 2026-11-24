//! The game's two screen ratios. Both are read as a frame 1280 pixels wide; only the height differs,
//! and the game's own UI is laid out differently in each, so every position the add-on relies on is
//! chosen per layout.

/// The game's screen ratio setting, told from the shape of its client area.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Layout {
    /// 4:3, a 1280x960 frame: the layout every pixel constant was first calibrated on.
    Ratio43,
    /// 16:9, a 1280x720 frame.
    Ratio169,
}

const ASPECT_43: f64 = 4.0 / 3.0;
const ASPECT_169: f64 = 16.0 / 9.0;
const ASPECT_TOL: f64 = 0.03;

impl Layout {
    /// Height of the frame (its width is always 1280).
    pub fn frame_height(self) -> u32 {
        match self {
            Layout::Ratio43 => 960,
            Layout::Ratio169 => 720,
        }
    }

    /// The layout of a frame of this height, if it is one of the two.
    pub fn from_frame_height(height: u32) -> Option<Layout> {
        [Layout::Ratio43, Layout::Ratio169].into_iter().find(|l| l.frame_height() == height)
    }

    /// The layout of a frame of this size; anything that is not 16:9 is read as 4:3, the original calibration.
    pub fn of_frame(width: u32, height: u32) -> Layout {
        if width == 1280 && height == 720 {
            Layout::Ratio169
        } else {
            Layout::Ratio43
        }
    }

    /// The layout a capture region has, or `None` when it is neither 4:3 nor 16:9.
    pub fn detect(region: [f64; 4]) -> Option<Layout> {
        let (w, h) = (region[2] - region[0], region[3] - region[1]);
        if w <= 0.0 || h <= 0.0 {
            return None;
        }
        let r = w / h;
        [(ASPECT_43, Layout::Ratio43), (ASPECT_169, Layout::Ratio169)].into_iter().find(|(a, _)| (r - a).abs() <= ASPECT_TOL * a / ASPECT_43).map(|(_, l)| l)
    }

    pub fn label(self) -> &'static str {
        match self {
            Layout::Ratio43 => "4:3",
            Layout::Ratio169 => "16:9",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regions_are_told_by_their_shape() {
        assert_eq!(Layout::detect([0.0, 0.0, 1280.0, 960.0]), Some(Layout::Ratio43));
        assert_eq!(Layout::detect([10.0, 10.0, 1930.0, 1090.0]), Some(Layout::Ratio169));
        assert_eq!(Layout::detect([0.0, 0.0, 1920.0, 1200.0]), None, "16:10 is neither");
        assert_eq!(Layout::detect([5.0, 5.0, 5.0, 50.0]), None);
    }

    #[test]
    fn frames_are_told_by_their_height() {
        assert_eq!(Layout::of_frame(1280, 720), Layout::Ratio169);
        assert_eq!(Layout::of_frame(1280, 960), Layout::Ratio43);
        assert_eq!(Layout::from_frame_height(720), Some(Layout::Ratio169));
        assert_eq!(Layout::from_frame_height(800), None);
    }
}
