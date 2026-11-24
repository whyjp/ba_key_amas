use super::*;
use baocr_ocr::PpOcr;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn engine() -> Option<Arc<dyn Ocr>> {
    let models = root().join("models");
    match PpOcr::new(&models) {
        Ok(o) => Some(Arc::new(o)),
        Err(e) => {
            eprintln!("skipping: {e}");
            None
        }
    }
}

fn tmp(name: &str) -> PathBuf {
    let d = PathBuf::from("D:/cargo-target/agent-pipeline-tmp").join(name);
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn fixture(f: &str) -> RgbImage {
    image::open(root().join("fixtures").join(f)).unwrap().to_rgb8()
}

fn fixture_frames(detail: &str) -> Frames {
    let mut m = Frames::new();
    m.insert(FRAME_DETAIL.into(), fixture(detail));
    m.insert(FRAME_POPUP_TOP.into(), fixture("popup-stats-top.png"));
    m.insert(FRAME_POPUP_BOTTOM.into(), fixture("popup-stats-bottom.png"));
    m.insert(FRAME_LEVEL_UP.into(), fixture("tab-levelup.png"));
    m.insert(FRAME_MYSTIC.into(), fixture("tab-mystic.png"));
    m
}

#[test]
fn async_pipeline_and_reextract() {
    let Some(ocr) = engine() else { return };
    let out = tmp("async");
    // deep=false: Extractor::deep may not be implemented; the deep path is covered separately.
    let p = Pipeline::new(&out, ocr.clone(), Options::default(), false, 2).unwrap();
    p.submit(0, fixture_frames("detail-shun.png"));
    p.submit(1, fixture_frames("detail-rio-armed.png"));
    let (st, err) = p.wait(0).unwrap();
    assert!(err.is_none() && st.name == "슌", "student 0: {:?} {err:?}", st.name);
    let rep = p.finish(RosterReport { schema_version: 3, ..Default::default() }, "test", true, true).unwrap();
    assert!(rep.account.is_some(), "account read from student 0");
    assert!(rep.complete);

    let ix = Index::load(&out).unwrap();
    for e in ix.sorted() {
        assert!(e.status == STATUS_EXTRACTED && e.frames.len() == 5 && e.attempts == 1, "index entry {e:?}");
        for f in &e.frames {
            assert!(out.join("details").join(&e.id).join(format!("{f}.png")).exists(), "frame {f} missing");
        }
    }
    assert_eq!(rep.entries.len(), 2);
    assert_eq!(rep.entries[0].student.name, "슌");
    assert_eq!(rep.entries[1].student.name, "리오(무장)");
    assert!(rep.unresolved.is_empty());

    // Simulate a failed extraction.
    std::fs::remove_file(out.join("details/002/student.json")).unwrap();
    ix.update("002", |e| {
        e.status = STATUS_FAILED.into();
        e.error = "simulated".into();
    });
    let rep = reextract(&out, &[], ocr, Options::default(), false, 2).unwrap();
    assert_eq!(rep.entries.len(), 2);
    assert_eq!(rep.entries[1].student.name, "리오(무장)");
    assert_eq!(rep.entries[1].status, "detail_ocr");
    for e in Index::load(&out).unwrap().sorted() {
        if e.id == "002" {
            assert!(e.status == STATUS_EXTRACTED && e.attempts == 2, "reextracted entry {e:?}");
        }
        if e.id == "001" {
            assert_eq!(e.attempts, 1, "untouched entry was re-run");
        }
    }
}

/// With deep on, either the menu pass works (Deep parsed) or the student is kept
/// with a deep_failed warning and marked failed.
#[test]
fn deep_pass_tolerates_unimplemented() {
    let Some(ocr) = engine() else { return };
    let out = tmp("deep");
    let p = Pipeline::new(&out, ocr, Options::default(), true, 1).unwrap();
    p.submit(0, fixture_frames("detail-shun.png"));
    let (st, err) = p.wait(0).unwrap();
    assert_eq!(st.name, "슌");
    match err {
        None => assert!(st.deep.is_some()),
        Some(_) => assert!(st.warnings.iter().any(|w| w.starts_with("deep_failed: "))),
    }
    p.finish(RosterReport { schema_version: 3, ..Default::default() }, "test", false, true).unwrap();
    assert!(out.join("details/001/student.json").exists());
}

#[test]
fn two_empty_names_abort() {
    let ocr: Arc<dyn Ocr> = Arc::new(NullOcr);
    let out = tmp("blank");
    let p = Pipeline::new(&out, ocr, Options::default(), false, 1).unwrap();
    let blank = || {
        let mut m = Frames::new();
        m.insert(FRAME_DETAIL.into(), RgbImage::from_pixel(1280, 960, image::Rgb([0, 0, 0])));
        m
    };
    p.submit(0, blank());
    p.submit(1, blank());
    assert_eq!(p.name_of(1), "");
    assert_eq!(p.name_of(0), "");
    let (reason, msg) = p.aborted().expect("abort after two empty names");
    assert_eq!(reason, "two_empty_names");
    assert_eq!(msg, "two consecutive profiles had no readable name; stopping");
    let rep = p.finish(RosterReport { schema_version: 3, ..Default::default() }, "two_empty_names", true, true).unwrap();
    assert!(!rep.complete);
    assert_eq!(rep.unresolved.len(), 2);
}

/// Reads nothing and scores every candidate equally, so no name can resolve.
struct NullOcr;

impl Ocr for NullOcr {
    fn boxes(&self, _: &RgbImage) -> Result<Vec<baocr_core::ocr_api::TextBox>> {
        Ok(vec![])
    }
    fn line(&self, _: &RgbImage) -> Result<(String, f32)> {
        Ok((String::new(), 0.0))
    }
    fn choose(&self, _: &RgbImage, c: &[String]) -> Result<(usize, Vec<f32>)> {
        Ok((0, vec![0.0; c.len()]))
    }
}
