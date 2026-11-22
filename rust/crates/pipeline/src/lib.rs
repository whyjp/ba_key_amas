//! Asynchronous extraction (port of cmd/baocr/pipeline.go).
//!
//! The capture loop never waits for OCR: it hands each student's frames to a
//! worker pool and moves on. Every student gets an entry in `out/index.json`
//! (captured -> extracted | failed) and its frames stay on disk under
//! `details/NNN/`, so a failed student can be re-extracted later (`reextract`)
//! without touching the game.
use anyhow::{anyhow, Context, Result};
use baocr_core::model::{Account, Int, RosterReport, Student};
use baocr_core::ocr_api::{text_in, Ocr, ScanSink};
use baocr_export::{export_student, pin_cell, rebuild_report, save_report, Index, STATUS_CAPTURED, STATUS_EXTRACTED, STATUS_FAILED};
use baocr_extract::{DeepFrames, Extractor, Options, FRAME_DETAIL, FRAME_LEVEL_UP, FRAME_MYSTIC, FRAME_POPUP_BOTTOM, FRAME_POPUP_TOP};
use image::RgbImage;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::JoinHandle;

type Frames = BTreeMap<String, RgbImage>;

/// Account header ROIs (AP, credits, pyroxene) on the 1280x960 frame.
const ACCOUNT_ROIS: [(i32, i32, i32, i32); 3] = [(505, 10, 600, 36), (718, 10, 832, 36), (936, 10, 995, 36)];

pub fn student_id(i: usize) -> String {
    format!("{:03}", i + 1)
}

fn detail_dir(out: &Path, id: &str) -> PathBuf {
    out.join("details").join(id)
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn save_frames(dir: &Path, frames: &Frames) -> Result<Vec<String>> {
    std::fs::create_dir_all(dir)?;
    let mut names = Vec::new();
    for (name, img) in frames {
        img.save(dir.join(format!("{name}.png"))).with_context(|| format!("save {name}"))?;
        names.push(name.clone());
    }
    names.sort();
    Ok(names)
}

fn load_frames(dir: &Path, names: &[String]) -> Result<Frames> {
    let mut frames = Frames::new();
    for n in names {
        let img = image::open(dir.join(format!("{n}.png"))).with_context(|| format!("load frame {n}"))?;
        frames.insert(n.clone(), img.to_rgb8());
    }
    Ok(frames)
}

/// Turns captured frames into a Student. A menu-pass failure keeps the base
/// profile (nothing is lost) and returns the error for the index.
fn extract_frames(ocr: &dyn Ocr, frames: &Frames, source: &str, opts: &Options, deep: bool) -> (Student, Option<String>) {
    let Some(detail) = frames.get(FRAME_DETAIL) else {
        return (Student::default(), Some("detail frame missing".into()));
    };
    let ex = Extractor::new(ocr);
    let mut s = match ex.profile(detail, source, opts) {
        Ok(s) => s,
        Err(e) => return (Student::default(), Some(format!("{e:#}"))),
    };
    if deep {
        let get = |k: &str| frames.get(k).cloned();
        if let (Some(popup_top), Some(popup_bottom), Some(level_up), Some(mystic)) =
            (get(FRAME_POPUP_TOP), get(FRAME_POPUP_BOTTOM), get(FRAME_LEVEL_UP), get(FRAME_MYSTIC))
        {
            let df = DeepFrames { popup_top, popup_bottom, level_up, mystic };
            if let Err(e) = ex.deep(&mut s, &df) {
                let msg = format!("{e:#}");
                s.warnings.push(format!("deep_failed: {msg}"));
                return (s, Some(msg));
            }
        }
    }
    (s, None)
}

fn atoi_loose(s: &str) -> Option<Int> {
    s.replace(',', "").parse().ok()
}

/// Header AP / credits / pyroxene, read from frame-level OCR boxes.
pub fn read_account(ocr: &dyn Ocr, detail: &RgbImage) -> Result<Account> {
    let boxes = ocr.boxes(detail)?;
    let cell = |i: usize| text_in(&boxes, ACCOUNT_ROIS[i]).trim().to_string();
    let num = |s: String| atoi_loose(&s.replace([' ', '.'], ""));
    let credits = cell(1);
    Ok(Account { ap: cell(0), credits: num(credits.clone()), pyroxene: num(cell(2)), raw_credits: credits })
}

/// Extracts one student from its frames, exports it and updates the index.
/// Returns the student and the error (if any).
fn extract_and_record(ocr: &dyn Ocr, ix: &Index, id: &str, dir: &Path, frames: &Frames, source: &str, opts: &Options, deep: bool) -> (Student, Option<String>) {
    let (s, mut err) = extract_frames(ocr, frames, source, opts, deep);
    if !s.name.is_empty() || err.is_none() {
        if let Err(e) = export_student(&s, dir) {
            err.get_or_insert(format!("{e:#}"));
        }
    }
    ix.update(id, |e| {
        e.name = s.name.clone();
        e.warnings = s.warnings.len() as Int;
        match &err {
            Some(m) => {
                e.status = STATUS_FAILED.into();
                e.error = m.clone();
            }
            None => {
                e.status = STATUS_EXTRACTED.into();
                e.error.clear();
            }
        }
    });
    (s, err)
}

// ---- worker pool --------------------------------------------------------------

struct Outcome {
    student: Student,
    err: Option<String>,
}

#[derive(Default)]
struct State {
    submitted: BTreeSet<usize>,
    results: BTreeMap<usize, Outcome>,
    account: Option<Account>,
    abort: Option<(String, String)>,
}

struct Shared {
    out: PathBuf,
    ocr: Arc<dyn Ocr>,
    opts: Options,
    deep: bool,
    idx: Index,
    state: Mutex<State>,
    cv: Condvar,
}

pub struct Pipeline {
    shared: Arc<Shared>,
    tx: Mutex<Option<SyncSender<(usize, Frames)>>>,
    workers: Vec<JoinHandle<()>>,
}

impl Pipeline {
    pub fn new(out: &Path, ocr: Arc<dyn Ocr>, opts: Options, deep: bool, workers: usize) -> Result<Pipeline> {
        let workers = workers.max(1);
        std::fs::create_dir_all(out)?;
        let shared = Arc::new(Shared { out: out.to_path_buf(), ocr, opts, deep, idx: Index::new(out), state: Mutex::new(State::default()), cv: Condvar::new() });
        let (tx, rx) = sync_channel::<(usize, Frames)>(workers * 2);
        let rx: Arc<Mutex<Receiver<(usize, Frames)>>> = Arc::new(Mutex::new(rx));
        let handles = (0..workers)
            .map(|_| {
                let (sh, rx) = (shared.clone(), rx.clone());
                std::thread::spawn(move || loop {
                    let job = lock(&rx).recv();
                    let Ok((i, frames)) = job else { break };
                    sh.run_job(i, frames);
                })
            })
            .collect();
        Ok(Pipeline { shared, tx: Mutex::new(Some(tx)), workers: handles })
    }

    /// The on-disk work list (for inspection and tests).
    pub fn index(&self) -> &Index {
        &self.shared.idx
    }

    /// Blocks until student `i` has been extracted.
    pub fn wait(&self, i: usize) -> Result<(Student, Option<String>)> {
        let mut st = lock(&self.shared.state);
        if !st.submitted.contains(&i) {
            return Err(anyhow!("student {} was never submitted", i + 1));
        }
        loop {
            if let Some(o) = st.results.get(&i) {
                return Ok((o.student.clone(), o.err.clone()));
            }
            st = self.shared.cv.wait(st).unwrap_or_else(|e| e.into_inner());
        }
    }

    fn shutdown(&mut self) {
        lock(&self.tx).take();
        for h in self.workers.drain(..) {
            let _ = h.join();
        }
    }

    /// Drains the workers, rebuilds the report from disk and saves it.
    pub fn finish(mut self, base: RosterReport, stop_reason: &str, wrapped: bool, from_first: bool) -> Result<RosterReport> {
        self.shutdown();
        let mut rep = rebuild_report(&self.shared.out, &self.shared.idx, base);
        rep.account = lock(&self.shared.state).account.clone().or(rep.account);
        rep.stop_reason = stop_reason.to_string();
        rep.complete = wrapped && from_first && rep.unresolved.is_empty();
        save_report(&rep, &self.shared.out)?;
        Ok(rep)
    }
}

impl Drop for Pipeline {
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl Shared {
    fn run_job(&self, i: usize, frames: Frames) {
        let r = catch_unwind(AssertUnwindSafe(|| self.process(i, &frames)));
        let oc = r.unwrap_or_else(|_| Outcome { student: Student::default(), err: Some("worker panicked".into()) });
        let mut st = lock(&self.state);
        st.results.insert(i, oc);
        if i > 0 && st.abort.is_none() {
            if let (Some(cur), Some(prev)) = (st.results.get(&i), st.results.get(&(i - 1))) {
                if cur.student.name.is_empty() && prev.student.name.is_empty() {
                    st.abort = Some(("two_empty_names".into(), "two consecutive profiles had no readable name; stopping".into()));
                }
            }
        }
        drop(st);
        self.cv.notify_all();
    }

    fn process(&self, i: usize, frames: &Frames) -> Outcome {
        let id = student_id(i);
        let dir = detail_dir(&self.out, &id);
        let names = match save_frames(&dir, frames) {
            Ok(n) => n,
            Err(e) => {
                let m = format!("save frames: {e:#}");
                self.idx.update(&id, |x| {
                    x.status = STATUS_FAILED.into();
                    x.error = m.clone();
                });
                return Outcome { student: Student::default(), err: Some(m) };
            }
        };
        self.idx.update(&id, |x| {
            x.frames = names;
            x.attempts += 1;
        });
        if i == 0 {
            if let Some(d) = frames.get(FRAME_DETAIL) {
                if let Ok(acc) = read_account(self.ocr.as_ref(), d) {
                    lock(&self.state).account = Some(acc);
                }
            }
        }
        let (s, err) = extract_and_record(self.ocr.as_ref(), &self.idx, &id, &dir, frames, &format!("profile arrow #{}", i + 1), &self.opts, self.deep);
        let status = match (&err, s.name.is_empty()) {
            (Some(e), _) => format!("FAILED: {e}"),
            (None, true) => "unresolved_name".into(),
            _ => "ok".into(),
        };
        println!("  extracted {id} {} {status} pin={} role={}", s.name, pin_cell(s.is_pinned), s.combat_role);
        Outcome { student: s, err }
    }
}

impl ScanSink for Pipeline {
    /// Queues a captured student; blocks only when every worker is busy and the queue is full.
    fn submit(&self, i: usize, frames: Frames) {
        lock(&self.shared.state).submitted.insert(i);
        self.shared.idx.update(&student_id(i), |e| e.status = STATUS_CAPTURED.into());
        let tx = lock(&self.tx).clone();
        match tx {
            Some(tx) if tx.send((i, frames)).is_ok() => {}
            _ => {
                // pipeline already closed: record the failure so waiters do not hang
                let m = "pipeline closed".to_string();
                self.shared.idx.update(&student_id(i), |e| {
                    e.status = STATUS_FAILED.into();
                    e.error = m.clone();
                });
                lock(&self.shared.state).results.insert(i, Outcome { student: Student::default(), err: Some(m) });
                self.shared.cv.notify_all();
            }
        }
    }

    fn name_of(&self, i: usize) -> String {
        self.wait(i).map(|(s, _)| s.name).unwrap_or_default()
    }

    fn peek_name(&self, img: &image::RgbImage) -> String {
        baocr_extract::Extractor::new(&*self.shared.ocr).name_only(img)
    }

    fn first_student(&self) -> Result<Student> {
        match self.wait(0)? {
            (s, None) => Ok(s),
            (_, Some(e)) => Err(anyhow!("{e}")),
        }
    }

    fn aborted(&self) -> Option<(String, String)> {
        lock(&self.shared.state).abort.clone()
    }
}

// ---- re-extraction ------------------------------------------------------------

/// Re-runs extraction from the saved frames of the selected students (all
/// non-extracted ones when `ids` is empty). Never touches the game.
pub fn reextract(out: &Path, ids: &[String], ocr: Arc<dyn Ocr>, opts: Options, deep: bool, workers: usize) -> Result<RosterReport> {
    let ix = Index::load(out).map_err(|e| anyhow!("no index in {}: {e}", out.display()))?;
    let want: BTreeSet<&str> = ids.iter().map(|s| s.trim()).collect();
    let todo: VecDeque<_> = ix
        .sorted()
        .into_iter()
        .filter(|e| if want.is_empty() { e.status != STATUS_EXTRACTED } else { want.contains(e.id.as_str()) })
        .collect();
    let queue = Mutex::new(todo);
    std::thread::scope(|sc| {
        for _ in 0..workers.max(1) {
            sc.spawn(|| loop {
                let Some(e) = lock(&queue).pop_front() else { break };
                let dir = detail_dir(out, &e.id);
                let (s, err) = match load_frames(&dir, &e.frames) {
                    Ok(frames) => extract_and_record(ocr.as_ref(), &ix, &e.id, &dir, &frames, &format!("re-extract {}", e.id), &opts, deep),
                    Err(er) => {
                        let m = format!("{er:#}");
                        ix.update(&e.id, |x| {
                            x.status = STATUS_FAILED.into();
                            x.error = m.clone();
                        });
                        (Student::default(), Some(m))
                    }
                };
                ix.update(&e.id, |x| x.attempts += 1);
                println!("  re-extracted {} {} err={}", e.id, s.name, err.as_deref().unwrap_or("none"));
            });
        }
    });
    let mut base = RosterReport { schema_version: 3, stop_reason: "reextracted".into(), ..Default::default() };
    if let Some(b) = std::fs::read(out.join("students.json")).ok().and_then(|b| serde_json::from_slice::<RosterReport>(&b).ok()) {
        base = b; // keep Complete/StopReason/Account/Capture from the scan
    }
    let rep = rebuild_report(out, &ix, base);
    save_report(&rep, out)?;
    Ok(rep)
}

#[cfg(test)]
mod tests;
