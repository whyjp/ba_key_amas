use image::RgbImage;

/// Names of the frames a student's capture is stored as (`<name>.png` in details/NNN/).
pub const FRAME_DETAIL: &str = "detail";
pub const FRAME_POPUP_TOP: &str = "popup-top";
pub const FRAME_POPUP_BOTTOM: &str = "popup-bottom";
pub const FRAME_LEVEL_UP: &str = "tab-levelup";
pub const FRAME_MYSTIC: &str = "tab-mystic";

/// The four menu frames captured by the menu pass (상세 popup top/bottom,
/// 레벨 업 tab, 신비 해방 tab).
#[derive(Clone)]
pub struct DeepFrames {
    pub popup_top: RgbImage,
    pub popup_bottom: RgbImage,
    pub level_up: RgbImage,
    pub mystic: RgbImage,
}
