//! Engine-neutral OCR surface. The engine (crates/ocr) implements it; core code only sees this.
use image::RgbImage;

/// One recognised text run in frame coordinates (pixels).
#[derive(Debug, Clone)]
pub struct TextBox {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
    pub text: String,
    pub score: f32,
}

pub trait Ocr: Send + Sync {
    /// Detect and recognise all text on a frame (detection + recognition).
    fn boxes(&self, img: &RgbImage) -> anyhow::Result<Vec<TextBox>>;
    /// Recognise one pre-cropped text line (no detection).
    fn line(&self, img: &RgbImage) -> anyhow::Result<(String, f32)>;
    /// Closed-set read: score every candidate against the line and return
    /// (best index, per-candidate score). Higher is better.
    fn choose(&self, img: &RgbImage, candidates: &[String]) -> anyhow::Result<(usize, Vec<f32>)>;
}

/// Boxes whose centre lies in `roi` (x0,y0,x1,y1), joined left to right.
pub fn text_in(boxes: &[TextBox], roi: (i32, i32, i32, i32)) -> String {
    let (x0, y0, x1, y1) = (roi.0 as f32 - 4.0, roi.1 as f32 - 4.0, roi.2 as f32 + 4.0, roi.3 as f32 + 4.0);
    let mut hits: Vec<&TextBox> = boxes
        .iter()
        .filter(|b| {
            let (cx, cy) = ((b.x0 + b.x1) / 2.0, (b.y0 + b.y1) / 2.0);
            cx >= x0 && cx <= x1 && cy >= y0 && cy <= y1
        })
        .collect();
    hits.sort_by(|a, b| a.x0.partial_cmp(&b.x0).unwrap());
    hits.iter().map(|b| b.text.as_str()).collect()
}

/// Where a scan loop hands captured frames. The async pipeline implements it;
/// the capture code (HCU) only sees this trait, so neither depends on the other.
pub trait ScanSink {
    /// Queue student `i`'s frames (keys are the FRAME_* names, e.g. "detail", "popup-top").
    fn submit(&self, i: usize, frames: std::collections::BTreeMap<String, RgbImage>);
    /// Resolved name of student `i` once extracted (blocks); empty if unreadable.
    fn name_of(&self, i: usize) -> String;
    /// Resolved name of a frame that was NOT submitted (empty if unreadable). Used to
    /// confirm a repeat or a wrap-around by name, not by fingerprint alone.
    fn peek_name(&self, img: &RgbImage) -> String;
    /// The extracted first student (blocks). Used for the layout sanity check.
    fn first_student(&self) -> anyhow::Result<crate::model::Student>;
    /// (reason, message) when a worker asked the scan to stop (e.g. two unreadable names).
    fn aborted(&self) -> Option<(String, String)>;
}
