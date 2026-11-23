use anyhow::{anyhow, bail, Result};
use image::RgbImage;
use serde::{Deserialize, Deserializer};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

/// Exact window title of the game. A bare "Blue Archive" substring is ambiguous
/// when a browser tab such as "... - Blue Archive Wiki" is open.
pub const GAME_TITLE: &str = "Blue Archive";

fn nullable<'de, D, T>(d: D) -> std::result::Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Ok(Option::<T>::deserialize(d)?.unwrap_or_default())
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Screen {
    #[serde(default, deserialize_with = "nullable")]
    pub width: i64,
    #[serde(default, deserialize_with = "nullable")]
    pub height: i64,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ZoomEvidence {
    #[serde(default, deserialize_with = "nullable")]
    pub region: Vec<f64>,
    #[serde(default, deserialize_with = "nullable")]
    pub scale_x: f64,
    #[serde(default, deserialize_with = "nullable")]
    pub scale_y: f64,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Evidence {
    #[serde(default, deserialize_with = "nullable")]
    pub zoom: ZoomEvidence,
}

/// One HCU `--records` JSON record (Go `record`).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Record {
    #[serde(default, deserialize_with = "nullable")]
    pub ok: bool,
    #[serde(default, deserialize_with = "nullable")]
    pub code: String,
    #[serde(default, deserialize_with = "nullable")]
    pub hint: String,
    #[serde(default, deserialize_with = "nullable")]
    pub input_emitted: bool,
    #[serde(default, deserialize_with = "nullable")]
    pub completed: i64,
    #[serde(default, deserialize_with = "nullable")]
    pub images: Vec<String>,
    #[serde(default, deserialize_with = "nullable")]
    pub screen: Screen,
    #[serde(default, deserialize_with = "nullable")]
    pub evidence: Evidence,
}

/// The only thing that touches the outside world. `run` executes the HCU CLI
/// with `args` and returns its combined output.
pub trait Runner: Send + Sync {
    fn run(&self, args: &[String]) -> Result<String>;
    /// Load a screenshot/zoom image the CLI reported by path.
    fn load_image(&self, path: &str) -> Result<RgbImage> {
        Ok(image::open(path)?.to_rgb8())
    }
}

/// Runs the real `computer-use` binary.
pub struct ProcessRunner {
    pub binary: PathBuf,
}

impl Runner for ProcessRunner {
    fn run(&self, args: &[String]) -> Result<String> {
        let mut cmd = Command::new(&self.binary);
        cmd.args(args);
        // the add-on has no console of its own: without this every call would flash a console window
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        let out = cmd.output().map_err(|e| anyhow!("run {}: {e}", self.binary.display()))?;
        let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
        text.push_str(&String::from_utf8_lossy(&out.stderr));
        // A failing exit with a JSON record is reported through the record (ok/code/hint),
        // like Go, which parses the output before looking at the exit status.
        if !out.status.success() && !text.trim_start().starts_with('{') {
            bail!("{} {:?}: {} ({})", self.binary.display(), args, out.status, text.trim());
        }
        Ok(text)
    }
}

/// One input action, in HCU frame pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Act {
    Scroll { x: f64, y: f64, delta: i32 },
    Click { x: f64, y: f64 },
}

/// Handle on the HCU CLI.
pub struct Hcu {
    runner: Box<dyn Runner>,
    /// Tests only: skip the real sleeps.
    no_sleep: bool,
    stats: std::sync::Mutex<std::collections::BTreeMap<String, (u64, f64)>>,
    /// Size of the GUI frame last reported by HCU (screenshots and input results both carry it).
    frame: std::sync::Mutex<(i64, i64)>,
    retrying: std::sync::atomic::AtomicBool,
    /// Raised to stop everything: while set, every call fails at once (the player pressed ESC).
    cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl Hcu {
    pub fn new(binary: impl Into<PathBuf>) -> Self {
        Self::with_runner(Box::new(ProcessRunner { binary: binary.into() }))
    }

    pub fn with_runner(runner: Box<dyn Runner>) -> Self {
        Self { runner, no_sleep: false, stats: Default::default(), frame: Default::default(), retrying: Default::default(), cancel: Default::default() }
    }

    /// Control the desktop in-process (no helper program).
    #[cfg(windows)]
    pub fn native() -> Self {
        Self::with_runner(Box::new(crate::native::NativeRunner::new(crate::native::WinDesktop::new())))
    }

    /// The in-process desktop on Windows; the `computer-use` helper (`$HCU_BIN`, `vendor/hcu`, PATH)
    /// when `BAOCR_HCU=process` is set or on other systems.
    pub fn default_backend() -> Self {
        #[cfg(windows)]
        if std::env::var("BAOCR_HCU").map_or(true, |v| v != "process") {
            return Self::native();
        }
        Self::from_env()
    }

    /// Binary from `$HCU_BIN`, else `vendor/hcu/computer-use[.exe]` beside the
    /// executable (or under the working directory), else on PATH.
    pub fn from_env() -> Self {
        let name = if cfg!(windows) { "computer-use.exe" } else { "computer-use" };
        let exe_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf)).unwrap_or_default();
        let rel = Path::new("vendor").join("hcu").join(name);
        let mut bin = find_bin(&exe_dir, "HCU_BIN", &rel, name);
        if !bin.exists() && bin.components().count() > 1 {
            let cwd = std::env::current_dir().unwrap_or_default().join(&rel);
            if cwd.exists() {
                bin = cwd;
            }
        }
        Self::new(bin)
    }

    /// Skip the sleeps in settle/wait loops (unit tests with a fake runner).
    pub fn without_sleep(mut self) -> Self {
        self.no_sleep = true;
        self
    }

    pub(crate) fn nap(&self, ms: u64) {
        if !self.no_sleep {
            std::thread::sleep(Duration::from_millis(ms));
        }
    }

    pub(crate) fn load_image(&self, path: &str) -> Result<RgbImage> {
        self.runner.load_image(path)
    }

    fn run_args(&self, args: &[&str]) -> Result<String> {
        let owned: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        self.runner.run(&owned)
    }

    /// Run an HCU subcommand and require an `ok` record.
    /// Calls and seconds spent per kind of HCU command since the last reset
    /// (screenshot, zoom, click, scroll, ...), for finding where a scan's time goes.
    pub fn stats_report(&self) -> String {
        let g = self.stats.lock().unwrap();
        let mut total = 0.0;
        let mut lines: Vec<String> = Vec::new();
        for (k, (n, secs)) in g.iter() {
            total += secs;
            lines.push(format!("  {k:<12} {n:>5} calls {secs:>8.1}s  ({:.2}s each)", secs / *n as f64));
        }
        lines.push(format!("  {:<12} {:>5}       {total:>8.1}s", "total", ""));
        lines.join("\n")
    }

    /// The GUI frame size, if HCU has reported one yet.
    pub fn frame_size(&self) -> Option<(i64, i64)> {
        let f = *self.frame.lock().unwrap();
        (f.0 > 0).then_some(f)
    }

    pub fn stats_reset(&self) {
        self.stats.lock().unwrap().clear();
    }

    /// The flag that cancels this handle: once it is set every call fails with "cancelled".
    pub fn cancel_token(&self) -> std::sync::Arc<std::sync::atomic::AtomicBool> {
        self.cancel.clone()
    }

    pub fn call(&self, args: &[&str]) -> Result<Record> {
        if self.cancel.load(std::sync::atomic::Ordering::SeqCst) {
            bail!("cancelled by the user");
        }
        let started = std::time::Instant::now();
        let r = self.call_inner(args);
        if let Ok(rec) = &r {
            if rec.screen.width > 0 {
                *self.frame.lock().unwrap() = (rec.screen.width, rec.screen.height);
            }
        }
        let kind = match args.get(1) {
            Some(&"--json") => {
                if args[2].contains("click") {
                    "batch"
                } else {
                    "scroll"
                }
            }
            Some(k) => k,
            None => "?",
        };
        let mut g = self.stats.lock().unwrap();
        let e = g.entry(kind.to_string()).or_insert((0, 0.0));
        e.0 += 1;
        e.1 += started.elapsed().as_secs_f64();
        r
    }

    fn call_inner(&self, args: &[&str]) -> Result<Record> {
        let out = self.run_args(args)?;
        let r: Record = serde_json::from_str(out.trim()).map_err(|e| anyhow!("HCU {:?}: {} ({})", args, e, out))?;
        if !r.ok {
            // The screen's layout changed since the last frame HCU gave out: input is
            // refused (nothing was sent), the remedy is a fresh screenshot, then again.
            let is_input = matches!(args.get(1), Some(&"click") | Some(&"double_click") | Some(&"move") | Some(&"--json"));
            if is_input && !r.input_emitted && r.code == "gui_frame_stale" && !self.retrying.swap(true, std::sync::atomic::Ordering::SeqCst) {
                let again = self.screenshot().and_then(|_| self.call_inner(args));
                self.retrying.store(false, std::sync::atomic::Ordering::SeqCst);
                return again;
            }
            bail!("HCU {:?}: {} {} (input_emitted={})", args, r.code, r.hint, r.input_emitted);
        }
        Ok(r)
    }

    pub fn screenshot(&self) -> Result<Record> {
        self.call(&["gui", "screenshot", "--records"])
    }

    pub fn zoom(&self, x0: i32, y0: i32, x1: i32, y1: i32) -> Result<Record> {
        let (a, b, c, d) = (x0.to_string(), y0.to_string(), x1.to_string(), y1.to_string());
        self.call(&["gui", "zoom", &a, &b, &c, &d, "--records"])
    }

    /// Click at frame coordinates; fails unless HCU confirms the dispatch.
    pub fn click(&self, x: f64, y: f64) -> Result<()> {
        let r = self.call(&["gui", "click", &format!("{x:.2}"), &format!("{y:.2}"), "--records"])?;
        if !r.input_emitted || r.completed != 1 {
            bail!("HCU click dispatch unconfirmed: {r:?}");
        }
        Ok(())
    }

    /// Scroll the wheel at frame coordinates.
    pub fn scroll(&self, x: f64, y: f64, delta: i32) -> Result<()> {
        self.scroll_n(x, y, delta, 1)
    }

    /// `count` wheel movements in a single HCU call. One call moves the list only a
    /// fixed distance whatever its delta, but extra movements in the same call cost
    /// almost nothing, so this is how a longer scroll is made.
    pub fn scroll_n(&self, x: f64, y: f64, delta: i32, count: usize) -> Result<()> {
        self.actions(&vec![Act::Scroll { x, y, delta }; count.max(1)])
    }

    /// Several input actions in ONE HCU call. Every input call costs a fixed ~1.35 s
    /// (HCU returns a final screenshot), however many actions it holds, so actions
    /// that do not need to look at the screen in between belong in one call.
    pub fn actions(&self, acts: &[Act]) -> Result<()> {
        let list: Vec<String> = acts
            .iter()
            .map(|a| match a {
                Act::Scroll { x, y, delta } => format!(r#"{{"type":"scroll","x":{x:.2},"y":{y:.2},"scroll_y":{delta}}}"#),
                Act::Click { x, y } => format!(r#"{{"type":"click","x":{x:.2},"y":{y:.2}}}"#),
            })
            .collect();
        let payload = format!(r#"{{"actions":[{}]}}"#, list.join(","));
        let r = self.call(&["gui", "--json", &payload, "--records"])?;
        if !r.input_emitted || r.completed != acts.len() as i64 {
            bail!("HCU input dispatch unconfirmed: {r:?}");
        }
        Ok(())
    }

    fn apps(&self) -> Result<String> {
        self.run_args(&["apps"]).map_err(|e| anyhow!("HCU apps: {e}"))
    }

    /// Target the game window by exact title.
    pub fn activate(&self) -> Result<()> {
        let apps = self.apps()?;
        let Some(r) = window_ref(&apps, GAME_TITLE) else {
            bail!("no window titled exactly \"{GAME_TITLE}\"");
        };
        self.call(&["activate", &r, "--records"])?;
        Ok(())
    }

    /// Fails unless the exact-titled game window is the active one, so a focus
    /// change can never turn a scan click into input for another window.
    pub fn foreground(&self) -> Result<()> {
        let apps = self.apps()?;
        if !window_active(&apps, GAME_TITLE) {
            bail!("Blue Archive is not the active window; refusing to send input");
        }
        Ok(())
    }
}

/// Finds the `apps` line whose `line[len(ref):]` starts with ` "title"` (exact title).
fn exact_title_line<'a>(apps: &'a str, title: &str) -> Option<(&'a str, &'a str)> {
    let want = format!(" \"{title}\"");
    for line in apps.split('\n') {
        let mut fields = line.split_whitespace();
        let (Some(f0), Some(_)) = (fields.next(), fields.next()) else { continue };
        if f0.starts_with('w') && line.get(f0.len()..).is_some_and(|rest| rest.starts_with(&want)) {
            return Some((f0, line));
        }
    }
    None
}

/// Whether the window with exactly this title is marked active.
pub fn window_active(apps: &str, title: &str) -> bool {
    exact_title_line(apps, title).is_some_and(|(_, line)| line.trim().ends_with("active"))
}

/// The `wN` ref of the window whose title equals `title` exactly.
pub fn window_ref(apps: &str, title: &str) -> Option<String> {
    exact_title_line(apps, title).map(|(r, _)| r.to_string())
}

/// Go `findBin`: env override, else `dir/rel` if it exists, else PATH, else `dir/rel`.
pub fn find_bin(dir: &Path, env: &str, rel: &Path, unix: &str) -> PathBuf {
    if let Ok(v) = std::env::var(env) {
        if !v.is_empty() {
            return PathBuf::from(v);
        }
    }
    let p = dir.join(rel);
    if p.exists() {
        return p;
    }
    if let Some(path) = std::env::var_os("PATH") {
        for d in std::env::split_paths(&path) {
            let c = d.join(unix);
            if c.is_file() {
                return c;
            }
        }
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    const APPS: &str = "w22 \"New Tab - Aside\"  app=Aside\nw13 \"Blue Archive\"  app=BlueArchive  active\nw15 \"Main Page - Blue Archive Wiki - Chrome\"  app=chrome\n";

    #[test]
    fn a_raised_cancel_token_stops_every_call_before_it_runs() {
        struct Never;
        impl Runner for Never {
            fn run(&self, _: &[String]) -> Result<String> {
                panic!("a cancelled handle must not run anything")
            }
        }
        let h = Hcu::with_runner(Box::new(Never));
        h.cancel_token().store(true, std::sync::atomic::Ordering::SeqCst);
        let e = h.call(&["gui", "screenshot"]).unwrap_err();
        assert!(format!("{e:#}").contains("cancelled"), "{e:#}");
        // and it can be lowered again for the next scan
        h.cancel_token().store(false, std::sync::atomic::Ordering::SeqCst);
        assert!(!h.cancel.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[test]
    fn exact_title_picks_game_window() {
        assert_eq!(window_ref(APPS, GAME_TITLE).as_deref(), Some("w13"));
        assert!(window_active(APPS, GAME_TITLE));
    }

    #[test]
    fn inactive_and_missing() {
        let apps = APPS.replace("  active", "");
        assert!(!window_active(&apps, GAME_TITLE));
        assert_eq!(window_ref(&apps, GAME_TITLE).as_deref(), Some("w13"));
        let wiki_only = "w15 \"Main Page - Blue Archive Wiki - Chrome\"  app=chrome  active\n";
        assert_eq!(window_ref(wiki_only, GAME_TITLE), None);
        assert!(!window_active(wiki_only, GAME_TITLE));
        assert!(!window_active("", GAME_TITLE));
    }

    #[test]
    fn crlf_active_line() {
        assert!(window_active("w13 \"Blue Archive\"  app=x  active\r\n", GAME_TITLE));
    }

    #[test]
    fn record_parses_sample_json() {
        let j = r#"{"ok":true,"code":"","hint":"","input_emitted":true,"completed":1,"images":["C:/t/z.png"],"screen":{"width":1920,"height":1080},"evidence":{"zoom":{"region":[1,2,3,4],"scale_x":2.5,"scale_y":2.0}},"extra":1}"#;
        let r: Record = serde_json::from_str(j).unwrap();
        assert!(r.ok && r.input_emitted);
        assert_eq!((r.completed, r.screen.width, r.screen.height), (1, 1920, 1080));
        assert_eq!(r.images, vec!["C:/t/z.png"]);
        assert_eq!(r.evidence.zoom.region, vec![1.0, 2.0, 3.0, 4.0]);
        assert_eq!((r.evidence.zoom.scale_x, r.evidence.zoom.scale_y), (2.5, 2.0));
        let n: Record = serde_json::from_str(r#"{"ok":false,"code":"E","images":null}"#).unwrap();
        assert!(!n.ok && n.images.is_empty());
    }
}
