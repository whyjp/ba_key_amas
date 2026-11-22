//! Autonomous updating (Go `watch.go`). A `Watcher` turns a stream of frames
//! into store updates and never sends input: frames come from a `FrameSource`
//! (passive screenshot of the game window, a folder of files in tests).
//! Extraction and storage are closures so this crate depends on neither the
//! OCR engine nor the store.
use crate::capture::{game_capture, layout_looks_calibrated};
use crate::hcu::Hcu;
use anyhow::Result;
use baocr_core::model::Student;
use baocr_pixels::{detail_distance, detail_key, detail_visible, frame_distance, frame_key, panel_delta};
use image::RgbImage;
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// Supplies the current game frame (1280x960 client layout).
pub trait FrameSource {
    fn frame(&mut self) -> Result<RgbImage>;
}

/// Passive HCU screenshots (no click, no key, no scroll).
pub struct HcuSource<'a> {
    pub hcu: &'a Hcu,
    pub region: [f64; 4],
}

impl FrameSource for HcuSource<'_> {
    fn frame(&mut self) -> Result<RgbImage> {
        game_capture(self.hcu, self.region)
    }
}

/// What one stable profile view produced. `changes` are the store's
/// human-readable `field from->to` strings.
#[derive(Debug, Clone, PartialEq)]
pub struct Update {
    pub name: String,
    pub new: bool,
    pub changes: Vec<String>,
}

impl fmt::Display for Update {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.new {
            return write!(f, "new student {}", self.name);
        }
        write!(f, "{}:", self.name)?;
        for c in &self.changes {
            write!(f, " {c}")?;
        }
        Ok(())
    }
}

/// Consecutive near-identical frames a view needs before it is read:
/// transitions and animations never get extracted.
pub const STILL_FRAMES: u32 = 2;

/// Reads stable profile views and stores what changed.
///
/// `E` extracts a `Student` from a frame (OCR lives with the caller); `S` upserts
/// it and returns `(change descriptions, is_new)`.
pub struct Watcher<Src, E, S> {
    pub src: Src,
    pub extract: E,
    pub store: S,
    pub interval: Duration,
    last_read: Option<RgbImage>,
    candidate: Option<RgbImage>,
    candidate_n: u32,
}

impl<Src, E, S> Watcher<Src, E, S>
where
    Src: FrameSource,
    E: FnMut(&RgbImage) -> Result<Student>,
    S: FnMut(&Student) -> Result<(Vec<String>, bool)>,
{
    pub fn new(src: Src, extract: E, store: S) -> Self {
        Self { src, extract, store, interval: Duration::from_secs(1), last_read: None, candidate: None, candidate_n: 0 }
    }

    /// Processes one frame. `Some` when a stable, changed profile view was
    /// stored, `None` otherwise.
    pub fn step(&mut self) -> Result<Option<Update>> {
        let img = self.src.frame()?;
        if !detail_visible(&img) {
            // not on a profile: forget the pending view
            self.candidate = None;
            self.candidate_n = 0;
            self.last_read = None;
            return Ok(None);
        }
        let still = self
            .candidate
            .as_ref()
            .is_some_and(|c| panel_delta(c, &img) < 0.0008 && frame_distance(&frame_key(c), &frame_key(&img)) < 3.0);
        self.candidate_n = if still { self.candidate_n + 1 } else { 1 };
        self.candidate = Some(img.clone());
        if self.candidate_n < STILL_FRAMES {
            return Ok(None);
        }
        // Same view as the one already stored: nothing to do.
        if let Some(lr) = &self.last_read {
            if panel_delta(lr, &img) < 0.0008 && detail_distance(&detail_key(lr), &detail_key(&img)) < 2.0 {
                return Ok(None);
            }
        }
        let s = (self.extract)(&img)?;
        self.last_read = Some(img);
        if s.name.is_empty() || !layout_looks_calibrated(&s) {
            return Ok(None); // unreadable view: never store a guess
        }
        let (changes, is_new) = (self.store)(&s)?;
        if !is_new && changes.is_empty() {
            return Ok(None);
        }
        Ok(Some(Update { name: s.name, new: is_new, changes }))
    }

    /// Steps every `interval` until `stop` is set.
    pub fn run(&mut self, stop: &AtomicBool, mut on_update: impl FnMut(Update), mut on_error: impl FnMut(&anyhow::Error)) {
        let interval = if self.interval.is_zero() { Duration::from_secs(1) } else { self.interval };
        while !stop.load(Ordering::Relaxed) {
            std::thread::sleep(interval);
            if stop.load(Ordering::Relaxed) {
                break;
            }
            match self.step() {
                Ok(Some(up)) => on_update(up),
                Ok(None) => {}
                Err(e) => on_error(&e),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake::flat;
    use image::Rgb;
    use std::cell::RefCell;
    use std::collections::VecDeque;

    struct Script(VecDeque<RgbImage>);
    impl FrameSource for Script {
        fn frame(&mut self) -> Result<RgbImage> {
            self.0.pop_front().ok_or_else(|| anyhow::anyhow!("no more frames"))
        }
    }

    fn profile(r: u8, g: u8, b: u8) -> RgbImage {
        // navy band colour everywhere, plus the white stat rows a real profile has
        let mut img = RgbImage::from_pixel(1280, 960, Rgb([r, g, b]));
        for y in 345..372 {
            for x in 700..1060 {
                img.put_pixel(x, y, Rgb([240, 244, 248]));
            }
        }
        img
    }

    fn student(name: &str, level: Option<i32>) -> Student {
        Student { name: name.into(), level, ..Default::default() }
    }

    #[test]
    fn reads_only_stable_changed_views() {
        let a = profile(40, 80, 120);
        let b = profile(70, 100, 148);
        let off = flat(200); // not a profile
        let frames = vec![off.clone(), a.clone(), a.clone(), a.clone(), a.clone(), b.clone(), b.clone(), off, a.clone(), a];
        let n = frames.len();
        let extracted = RefCell::new(0);
        let stored = RefCell::new(Vec::<String>::new());
        let mut w = Watcher::new(
            Script(frames.into()),
            |_img: &RgbImage| {
                *extracted.borrow_mut() += 1;
                let name = if *extracted.borrow() == 2 { "B" } else { "A" };
                Ok(student(name, Some(5)))
            },
            |s: &Student| {
                stored.borrow_mut().push(s.name.clone());
                Ok((vec!["level 4->5".into()], s.name == "A" && stored.borrow().len() == 1))
            },
        );
        let mut ups = vec![];
        for _ in 0..n {
            ups.push(w.step().unwrap());
        }
        // 0 off | 1 a (n=1) | 2 a (n=2 -> read, new A) | 3 a same | 4 a same | 5 b (n=1) | 6 b (n=2 -> read B, change) | 7 off | 8 a (n=1) | 9 a (n=2 -> read again)
        assert!(ups[0].is_none() && ups[1].is_none());
        assert_eq!(ups[2], Some(Update { name: "A".into(), new: true, changes: vec!["level 4->5".into()] }));
        assert!(ups[3].is_none() && ups[4].is_none() && ups[5].is_none());
        assert_eq!(ups[6].as_ref().map(|u| u.name.as_str()), Some("B"));
        assert!(ups[7].is_none() && ups[8].is_none());
        assert!(ups[9].is_some());
        assert_eq!(*extracted.borrow(), 3);
    }

    #[test]
    fn unreadable_view_is_never_stored() {
        let a = profile(40, 80, 120);
        let stored = RefCell::new(0);
        let mut w = Watcher::new(Script(vec![a.clone(), a.clone(), a].into()), |_: &RgbImage| Ok(student("", None)), |_: &Student| {
            *stored.borrow_mut() += 1;
            Ok((vec![], true))
        });
        for _ in 0..3 {
            assert!(w.step().unwrap().is_none());
        }
        assert_eq!(*stored.borrow(), 0);
        // uncalibrated layout with a name is also dropped
        let a = profile(40, 80, 120);
        let mut w = Watcher::new(Script(vec![a.clone(), a].into()), |_: &RgbImage| Ok(student("X", None)), |_: &Student| {
            *stored.borrow_mut() += 1;
            Ok((vec![], true))
        });
        assert!(w.step().unwrap().is_none() && w.step().unwrap().is_none());
        assert_eq!(*stored.borrow(), 0);
    }

    #[test]
    fn unchanged_record_gives_no_update() {
        let a = profile(40, 80, 120);
        let mut w = Watcher::new(Script(vec![a.clone(), a].into()), |_: &RgbImage| Ok(student("A", Some(1))), |_: &Student| Ok((vec![], false)));
        assert!(w.step().unwrap().is_none());
        assert!(w.step().unwrap().is_none());
    }

    #[test]
    fn extract_error_propagates_and_update_display() {
        let a = profile(40, 80, 120);
        let mut w = Watcher::new(Script(vec![a.clone(), a].into()), |_: &RgbImage| Err(anyhow::anyhow!("ocr down")), |_: &Student| Ok((vec![], false)));
        assert!(w.step().unwrap().is_none());
        assert!(w.step().is_err());
        let u = Update { name: "Aru".into(), new: false, changes: vec!["level 1->2".into(), "bond 3->4".into()] };
        assert_eq!(u.to_string(), "Aru: level 1->2 bond 3->4");
        assert_eq!(Update { name: "Aru".into(), new: true, changes: vec![] }.to_string(), "new student Aru");
    }
}
