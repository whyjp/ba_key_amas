//! Pure logic of the progress scan: what the account's screens say, read from OCR boxes and pixels.
//!
//! Nothing here touches the game or an OCR engine. Every parser takes the boxes (and, where a value is an
//! icon, the frame) of one screen and returns what it can read; a value it cannot read is `None` and the
//! reason goes into `warnings`, never a guess. Parsers find things by text and by the distance between
//! boxes, not by fixed coordinates, so they work on both screen layouts.

pub mod account;
pub mod bareceipt;
pub mod event_open;
pub mod event_past;
pub mod missions;
pub mod momotalk;
pub mod schedule;
pub mod schema;
pub mod scroll;
pub mod stars;
pub mod story;

mod geom;
mod vision;

pub use vision::Rect;

/// A parsed value plus the notes about what could not be read.
#[derive(Debug, Clone, PartialEq)]
pub struct Read<T> {
    pub value: T,
    pub warnings: Vec<String>,
}

#[cfg(test)]
pub(crate) mod testutil {
    use baocr_core::ocr_api::TextBox;
    use image::RgbImage;

    fn path(name: &str, ext: &str) -> String {
        format!("{}/../../../fixtures/screens/{name}.{ext}", env!("CARGO_MANIFEST_DIR"))
    }

    pub fn boxes(name: &str) -> Vec<TextBox> {
        let p = path(name, "boxes.txt");
        std::fs::read_to_string(&p)
            .unwrap_or_else(|e| panic!("{p}: {e}"))
            .lines()
            .filter_map(|l| {
                let (pos, text) = l.split_once('\t')?;
                let v: Vec<f32> = pos.split(',').filter_map(|n| n.parse().ok()).collect();
                (v.len() == 4).then(|| TextBox { x0: v[0], y0: v[1], x1: v[2], y1: v[3], text: text.to_string(), score: 1.0 })
            })
            .collect()
    }

    pub fn png(name: &str) -> RgbImage {
        let p = path(name, "png");
        image::open(&p).unwrap_or_else(|e| panic!("{p}: {e}")).to_rgb8()
    }
}
