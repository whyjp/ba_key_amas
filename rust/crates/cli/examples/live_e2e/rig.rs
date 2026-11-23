//! What the scenarios stand on: the add-on's state dump, checked clicks into the game, real mouse
//! input on the add-on's own card, screenshots and an independent OCR of them, and the result table.
use anyhow::{bail, Result};
use baocr_core::ocr_api::Ocr;
use baocr_hcu::{check_aspect, detect_region, game_capture, to_frame, HcuSession};
use baocr_ocr::PpOcr;
use baocr_pixels::{fast_frame_key, frame_distance, looks_like_battle, FrameKey};
use image::RgbImage;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub fn unix_ms() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis())
}

pub fn nap(ms: u64) {
    std::thread::sleep(Duration::from_millis(ms));
}

/// min / median / p90 of a sample, in the sample's unit.
pub fn stats(v: &[f64]) -> Option<(f64, f64, f64)> {
    if v.is_empty() {
        return None;
    }
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let at = |q: f64| s[((s.len() as f64 - 1.0) * q).round() as usize];
    Some((s[0], at(0.5), at(0.9)))
}

pub struct Check {
    pub scenario: String,
    pub criterion: String,
    pub measured: String,
    pub pass: bool,
}

/// Raw Win32 input on our own windows (the add-on's card), never on the game.
pub mod mouse {
    #[link(name = "user32")]
    extern "system" {
        fn SetCursorPos(x: i32, y: i32) -> i32;
        fn mouse_event(flags: u32, dx: i32, dy: i32, data: u32, extra: usize);
    }
    const LEFTDOWN: u32 = 0x2;
    const LEFTUP: u32 = 0x4;

    pub fn move_to(x: i32, y: i32) {
        unsafe { SetCursorPos(x, y) };
    }
    pub fn down() {
        unsafe { mouse_event(LEFTDOWN, 0, 0, 0, 0) };
    }
    pub fn up() {
        unsafe { mouse_event(LEFTUP, 0, 0, 0, 0) };
    }
    /// Press at `from`, glide to `to` in steps, release.
    pub fn drag(from: (i32, i32), to: (i32, i32)) {
        move_to(from.0, from.1);
        super::nap(80);
        down();
        super::nap(80);
        for i in 1..=12 {
            move_to(from.0 + (to.0 - from.0) * i / 12, from.1 + (to.1 - from.1) * i / 12);
            super::nap(25);
        }
        super::nap(120);
        up();
    }
}

pub struct Rig {
    pub session: HcuSession,
    pub region: [f64; 4],
    pub dump_path: PathBuf,
    pub out: PathBuf,
    pub ocr: PpOcr,
    pub checks: Vec<Check>,
    pub latency: BTreeMap<String, Vec<f64>>,
    pub refusals: Vec<String>,
    pub battle_frames_seen: u32,
    pub frames_seen: u32,
    shot_no: u32,
}

impl Rig {
    pub fn new(dump_path: PathBuf, out: PathBuf) -> Result<Rig> {
        std::fs::create_dir_all(out.join("shots"))?;
        let session = HcuSession::default_backend();
        let region = detect_region(session.view())?;
        check_aspect(region)?;
        let ocr = PpOcr::new(&PpOcr::default_models_dir())?;
        Ok(Rig { session, region, dump_path, out, ocr, checks: Vec::new(), latency: BTreeMap::new(), refusals: Vec::new(), battle_frames_seen: 0, frames_seen: 0, shot_no: 0 })
    }

    pub fn dump(&self) -> Result<Value> {
        let mut last = None;
        for _ in 0..8 {
            match std::fs::read_to_string(&self.dump_path).map_err(anyhow::Error::from).and_then(|t| Ok(serde_json::from_str::<Value>(&t)?)) {
                Ok(v) => return Ok(v),
                Err(e) => last = Some(e),
            }
            nap(5);
        }
        Err(last.unwrap())
    }

    pub fn scene(&self) -> String {
        self.dump().ok().and_then(|d| d["main"]["scene"].as_str().map(str::to_string)).unwrap_or_default()
    }

    /// The dump, but only when the add-on has written it recently (it is alive and looking).
    pub fn fresh_dump(&self) -> Result<Value> {
        let d = self.dump()?;
        let age = unix_ms().saturating_sub(d["unix_ms"].as_u64().unwrap_or(0) as u128);
        if age > 3000 {
            bail!("the state dump is {age} ms old: the add-on is not running");
        }
        Ok(d)
    }

    pub fn grab(&mut self) -> Result<RgbImage> {
        let img = game_capture(self.session.view(), self.region)?;
        self.frames_seen += 1;
        if looks_like_battle(&img) {
            self.battle_frames_seen += 1;
        }
        Ok(img)
    }

    /// Save the frame and its OCR text; returns the saved png path and the normalised text.
    pub fn shot(&mut self, name: &str) -> Result<(PathBuf, String)> {
        let img = self.grab()?;
        self.shot_no += 1;
        let path = self.out.join("shots").join(format!("{:03}-{name}.png", self.shot_no));
        img.save(&path)?;
        let mut text = String::new();
        let mut all = String::new();
        for b in self.ocr.boxes(&img)? {
            text.push_str(&format!("{:.0},{:.0},{:.0},{:.0}\t{}\n", b.x0, b.y0, b.x1, b.y1, b.text.trim()));
            all.push_str(&b.text.chars().filter(|c| !c.is_whitespace()).collect::<String>());
            all.push('\n');
        }
        std::fs::write(path.with_extension("boxes.txt"), text)?;
        Ok((path, all))
    }

    pub fn record(&mut self, scenario: &str, criterion: &str, measured: impl Into<String>, pass: bool) {
        let measured = measured.into();
        println!("{} [{scenario}] {criterion}: {measured}", if pass { "PASS" } else { "FAIL" });
        self.checks.push(Check { scenario: scenario.into(), criterion: criterion.into(), measured, pass });
    }

    /// Why a click must never be sent here (the money and ticket buttons), or None.
    fn forbidden(scene: &str, title: &str, x: i32, y: i32) -> Option<&'static str> {
        let back = (x - 55).abs() < 45 && (y - 38).abs() < 30;
        if scene.ends_with("편성") && x >= 1080 && y >= 850 {
            return Some("the 출격 button of a formation");
        }
        if scene == "총력전 편성" && !back {
            return Some("any click on a real 총력전 formation");
        }
        if scene == "방 정보" && (860..1010).contains(&x) && (470..540).contains(&y) {
            return Some("the 소탕 button");
        }
        if scene == "방 정보" && (850..1000).contains(&x) && (620..700).contains(&y) && !title.starts_with("모의 총력전") {
            return Some("the 입장 button of a real raid");
        }
        if scene == "총력전 선택" && x >= 1100 && (150..800).contains(&y) {
            return Some("a real raid's 입장 button");
        }
        None
    }

    /// A navigation click in the game. Refused unless the add-on reports `expect` (when given) right
    /// now and a fresh frame can be taken; sends nothing otherwise. Returns when the click was sent (unix ms).
    pub fn click(&mut self, x: i32, y: i32, expect: &str, why: &str) -> Result<u128> {
        let d = self.fresh_dump()?;
        let scene = d["main"]["scene"].as_str().unwrap_or("").to_string();
        let title = d["main"]["title"].as_str().unwrap_or("").to_string();
        if !expect.is_empty() && scene != expect {
            bail!("refusing to click ({x},{y}) for '{why}': the add-on says '{scene}', expected '{expect}'");
        }
        if let Some(what) = Self::forbidden(&scene, &title, x, y) {
            bail!("refusing to click ({x},{y}) for '{why}': that is {what}");
        }
        let (fx, fy) = to_frame(self.region, x, y);
        for attempt in 1..=3 {
            let img = self.grab()?;
            if looks_like_battle(&img) {
                bail!("refusing to click for '{why}': the frame looks like a battle");
            }
            let input = self.session.exclusive();
            let ready = input.activate().and_then(|_| {
                nap(200);
                input.foreground()
            });
            match ready.and_then(|_| input.click(fx, fy)) {
                Ok(()) => {
                    let sent = unix_ms();
                    println!("click ({x},{y}) {why} [{scene}]");
                    return Ok(sent);
                }
                Err(e) => {
                    // the guard refused (a window over the point, a stale frame): nothing was sent. A shell window can be there for a moment.
                    let msg = format!("{e:#}");
                    println!("click refused ({attempt}/3): {msg}");
                    self.refusals.push(msg.clone());
                    if attempt == 3 || !(msg.contains("input_emitted=false") || msg.contains("not the active window")) {
                        return Err(e);
                    }
                    nap(1500);
                }
            }
        }
        unreachable!()
    }

    /// Run `f` while frames of the game are captured; returns what `f` returned and the (time, key) of each frame.
    pub fn with_frame_watch<R>(&self, f: impl FnOnce() -> R) -> (R, Vec<(u128, FrameKey)>) {
        let stop = std::sync::atomic::AtomicBool::new(false);
        let (hcu, region) = (self.session.view(), self.region);
        std::thread::scope(|s| {
            let h = s.spawn(|| {
                let mut v = Vec::new();
                while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                    if let Ok(img) = game_capture(hcu, region) {
                        v.push((unix_ms(), fast_frame_key(&img)));
                    }
                }
                v
            });
            let r = f();
            stop.store(true, std::sync::atomic::Ordering::Relaxed);
            (r, h.join().unwrap_or_default())
        })
    }

    /// When the screen stopped changing: the time of the first frame of the final run of frames that all look like the last one.
    pub fn settled_at(frames: &[(u128, FrameKey)]) -> Option<u128> {
        let last = frames.last()?;
        let mut at = last.0;
        for (t, k) in frames.iter().rev() {
            if frame_distance(k, &last.1) > 1.5 {
                break;
            }
            at = *t;
        }
        Some(at)
    }

    /// Wait until the add-on shows `scene`, changed at or after `since`. Returns the latency in ms
    /// from `since` to the state change written by the add-on, or None on timeout.
    pub fn wait_scene(&self, scene: &str, since: u128, timeout_ms: u64) -> Option<f64> {
        let end = unix_ms() + timeout_ms as u128;
        while unix_ms() < end {
            if let Ok(d) = self.dump() {
                let changed = d["changed_unix_ms"].as_u64().unwrap_or(0) as u128;
                if d["main"]["scene"].as_str() == Some(scene) && changed >= since {
                    return Some(changed.saturating_sub(since) as f64);
                }
            }
            nap(8);
        }
        None
    }

    /// Wait until the add-on's scene is `scene` (whatever the change time), for the start of a step.
    pub fn wait_for_scene_now(&self, scene: &str, timeout_ms: u64) -> bool {
        let end = unix_ms() + timeout_ms as u128;
        while unix_ms() < end {
            if self.scene() == scene {
                return true;
            }
            nap(50);
        }
        false
    }

    pub fn add_latency(&mut self, key: &str, ms: f64) {
        self.latency.entry(key.into()).or_default().push(ms);
    }

    pub fn card_text(&self) -> String {
        let d = self.dump().unwrap_or_default();
        let mut s = String::new();
        for c in ["main", "ingame"] {
            if let Some(rows) = d[c]["rows"].as_array() {
                s.push_str(d[c]["title"].as_str().unwrap_or(""));
                s.push('\n');
                for r in rows {
                    s.push_str(r.as_str().unwrap_or(""));
                    s.push('\n');
                }
            }
        }
        s
    }
}

pub fn norm(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}
