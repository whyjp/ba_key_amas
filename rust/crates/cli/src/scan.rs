//! The roster scan: count the students, walk through their profiles with the arrow button
//! (the one thing here that sends input to the game), and write the JSON output.
//! Progress is reported through a callback, so the command line and the add-on's menu
//! both show it on the same banner.
use anyhow::Result;
use baocr_core::model::RosterReport;
use baocr_core::ocr_api::{Ocr, ScanSink};
use baocr_extract::Options;
use baocr_hcu::{ensure_profile, new_capture, prepare_scan, scan_arrows, Exclusive};
use baocr_overlay::BannerState;
use baocr_pipeline::Pipeline;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

pub struct ScanRequest<'a> {
    /// Where students.json, index.json and details/ are written.
    pub out: &'a Path,
    /// Also read the stat popup and the level-up and mystic tabs (about 2.5x slower).
    pub deep: bool,
    pub workers: usize,
    pub max_students: usize,
    pub from_first: bool,
}

#[derive(Debug)]
pub struct ScanSummary {
    pub entries: usize,
    pub complete: bool,
    pub stop_reason: String,
    pub error: Option<String>,
}

/// Forwards frames to the pipeline and moves the banner's n/m forward.
struct ProgressSink<'a> {
    inner: &'a Pipeline,
    report: &'a dyn Fn(BannerState),
    total: Option<usize>,
}

impl ScanSink for ProgressSink<'_> {
    fn submit(&self, i: usize, frames: BTreeMap<String, image::RgbImage>) {
        self.inner.submit(i, frames);
        (self.report)(BannerState::scanning(i + 1, self.total));
    }
    fn name_of(&self, i: usize) -> String {
        self.inner.name_of(i)
    }
    fn peek_name(&self, img: &image::RgbImage) -> String {
        self.inner.peek_name(img)
    }
    fn first_student(&self) -> Result<baocr_core::model::Student> {
        self.inner.first_student()
    }
    fn aborted(&self) -> Option<(String, String)> {
        self.inner.aborted()
    }
}

/// Plain-language reason shown on the banner when a scan stops early.
fn stop_note(reason: &str) -> String {
    match reason {
        "focus_lost" => "게임 창이 활성 상태가 아니어서 중단했습니다".into(),
        "layout_mismatch" => "4:3 창모드 화면이 아니어서 중단했습니다".into(),
        "two_empty_names" => "학생 이름을 읽지 못해 중단했습니다".into(),
        "deep_failed" => "메뉴 화면 캡처에 실패해 중단했습니다".into(),
        "repeated_profile" => "같은 학생이 반복되어 중단했습니다".into(),
        other => format!("중단됨: {other}"),
    }
}

pub fn scan_students(ocr: Arc<dyn Ocr>, hcu: &Exclusive, region: [f64; 4], req: &ScanRequest, opts: Options, report: &dyn Fn(BannerState)) -> Result<ScanSummary> {
    // Learn the total first (scroll through the roster once) so progress reads n/m,
    // and start from the first student.
    report(BannerState::counting(0));
    // The main menu is recognised by its bottom labels, and 학생 is found there.
    let student_button = |img: &image::RgbImage| baocr_extract::lobby_student_button(&*ocr, img);
    let total = match prepare_scan(hcu, region, &student_button, |n| report(BannerState::counting(n))) {
        Ok(t) => {
            println!("students in the roster: {t}");
            Some(t)
        }
        Err(e) => {
            eprintln!("could not count the roster ({e:#}); scanning from the open profile without a total");
            if let Err(e2) = ensure_profile(hcu, region, &student_button) {
                report(BannerState::failed(0, None, "학생 목록, 학생 프로필 또는 메인 메뉴 화면을 먼저 열어 주세요"));
                return Err(e2);
            }
            None
        }
    };
    let from_first = req.from_first || total.is_some();
    report(BannerState::scanning(0, total));

    let pipeline = Pipeline::new(req.out, ocr.clone(), opts, req.deep, req.workers)?;
    let sink = ProgressSink { inner: &pipeline, report, total };
    hcu.stats_reset();
    let scan_started = std::time::Instant::now();
    let outcome = scan_arrows(hcu, region, req.out, req.max_students, from_first, req.deep, &sink)?;
    if std::env::var_os("BAOCR_HCU_STATS").is_some() {
        eprintln!("scan loop {:.1}s; HCU time by command:\n{}", scan_started.elapsed().as_secs_f64(), hcu.stats_report());
    }
    let base = RosterReport { schema_version: 3, capture: Some(new_capture(region)), ..Default::default() };
    let rep = pipeline.finish(base, &outcome.stop_reason, outcome.wrapped, from_first)?;
    println!("arrows: entries={} complete={} reason={} unresolved={}", rep.entries.len(), rep.complete, rep.stop_reason, rep.unresolved.len());
    let n = rep.entries.len();
    if outcome.error.is_none() || matches!(outcome.stop_reason.as_str(), "wrapped_to_first" | "max_students") {
        report(BannerState::finished(n, total.or(Some(n))));
    } else {
        report(BannerState::failed(n, total, &stop_note(&outcome.stop_reason)));
    }
    Ok(ScanSummary { entries: n, complete: rep.complete, stop_reason: rep.stop_reason, error: outcome.error })
}
