//! Small helpers over `TextBox` geometry, shared by the parsers.
use baocr_core::ocr_api::TextBox;

pub fn cx(b: &TextBox) -> f32 {
    (b.x0 + b.x1) / 2.0
}
pub fn cy(b: &TextBox) -> f32 {
    (b.y0 + b.y1) / 2.0
}
pub fn w(b: &TextBox) -> f32 {
    b.x1 - b.x0
}
pub fn h(b: &TextBox) -> f32 {
    b.y1 - b.y0
}
pub fn text(b: &TextBox) -> &str {
    b.text.trim()
}

/// The text with every whitespace removed; OCR spacing of Korean text is not stable.
pub fn squashed(b: &TextBox) -> String {
    b.text.chars().filter(|c| !c.is_whitespace()).collect()
}

/// A run of 1..=`max_len` ASCII digits and nothing else.
pub fn digits(b: &TextBox, max_len: usize) -> Option<u32> {
    let t = text(b);
    (!t.is_empty() && t.len() <= max_len && t.bytes().all(|c| c.is_ascii_digit())).then(|| t.parse().ok()).flatten()
}
