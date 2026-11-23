//! Drives the native runner against the `input_target` window ONLY, with real input: screenshot, zoom,
//! clicks at several points, wheel turns, and the cases that must be refused (window moved, window
//! covered, point on no monitor). It moves the real mouse, so run it only when nothing else needs the
//! pointer, and never against the game.
//!
//! ```text
//! cargo build -p baocr-hcu --examples
//! cargo run -p baocr-hcu --example input_selftest
//! ```
//!
//! It starts `input_target` itself (built next to this example), compares what the target logged with
//! where it aimed, prints PASS/FAIL per check and exits 1 on any FAIL. The runner's own audit lines go to
//! `%TEMP%\hcu-selftest-input.log`.

#[cfg(windows)]
mod app {
    use baocr_hcu::{delivered_size, Desktop, FileSink, NativeRunner, Runner, Settle, WinDesktop, WinInfo};
    use serde_json::Value;
    use std::path::PathBuf;
    use std::process::{Child, Command};
    use std::time::{Duration, Instant};

    const TARGET: &str = "HCU Test Target";
    const COVER: &str = "HCU Cover";

    #[link(name = "user32")]
    extern "system" {
        fn SetWindowPos(hwnd: isize, after: isize, x: i32, y: i32, cx: i32, cy: i32, flags: u32) -> i32;
    }

    struct Checks(Vec<(bool, String)>);
    impl Checks {
        fn check(&mut self, ok: bool, what: impl Into<String>) {
            let what = what.into();
            println!("{} {what}", if ok { "PASS" } else { "FAIL" });
            self.0.push((ok, what));
        }
    }

    fn sibling(name: &str) -> PathBuf {
        let mut p = std::env::current_exe().expect("current exe");
        p.set_file_name(format!("{name}.exe"));
        p
    }

    fn find(desk: &WinDesktop, title: &str) -> Option<WinInfo> {
        desk.layout().into_iter().find(|w| w.title == title)
    }

    fn wait_for(desk: &WinDesktop, title: &str) -> Option<WinInfo> {
        let until = Instant::now() + Duration::from_secs(8);
        while Instant::now() < until {
            if let Some(w) = find(desk, title) {
                std::thread::sleep(Duration::from_millis(300)); // let it paint
                return Some(w);
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        None
    }

    fn spawn(args: &[&str]) -> std::io::Result<Child> {
        Command::new(sibling("input_target")).args(args).spawn()
    }

    fn ask(r: &impl Runner, args: &[&str]) -> Value {
        let owned: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        match r.run(&owned) {
            Ok(text) => serde_json::from_str(&text).unwrap_or(Value::Null),
            Err(e) => serde_json::json!({"ok": false, "code": format!("error: {e:#}")}),
        }
    }

    /// The frame point (as the runner's protocol states it) that lands on desktop pixel (nx, ny).
    fn frame_point(desk: &WinDesktop, nx: i32, ny: i32) -> (f64, f64) {
        let (ox, oy, w, h) = desk.bounds();
        let (dw, dh) = delivered_size((w, h));
        (((nx - ox) as f64 + 0.5) * dw as f64 / w as f64 - 0.5, ((ny - oy) as f64 + 0.5) * dh as f64 / h as f64 - 0.5)
    }

    fn lines(path: &PathBuf, kind: &str) -> Vec<String> {
        std::fs::read_to_string(path).unwrap_or_default().lines().filter(|l| l.starts_with(kind)).map(String::from).collect()
    }

    /// `x=..` and `y=..` of a logged line.
    fn screen_xy(line: &str) -> Option<(i32, i32)> {
        let num = |key: &str| line.split_whitespace().find_map(|t| t.strip_prefix(key)).and_then(|v| v.parse::<i32>().ok());
        Some((num("x=")?, num("y=")?))
    }

    pub fn run() -> i32 {
        let desk = WinDesktop::new();
        let log = std::env::temp_dir().join("hcu-selftest-target.log");
        let _ = std::fs::remove_file(&log);
        let mut c = Checks(Vec::new());
        let mut target = match spawn(&["--log", &log.to_string_lossy()]) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("cannot start {} ({e}); run `cargo build -p baocr-hcu --examples` first", sibling("input_target").display());
                return 2;
            }
        };
        let Some(win) = wait_for(&desk, TARGET) else {
            eprintln!("the test target window did not appear");
            let _ = target.kill();
            return 2;
        };
        let audit = std::env::temp_dir().join("hcu-selftest-input.log");
        let runner = NativeRunner::new(WinDesktop::new()).with_target(TARGET).with_settle(Settle::from_env()).with_audit(Box::new(FileSink::new(audit.clone())));
        println!("target window {:?} at {:?}; audit log {}", win.title, win.rect, audit.display());
        let at = |fx: f64, fy: f64| -> (i32, i32) {
            let r = find(&desk, TARGET).map(|w| w.rect).unwrap_or(win.rect);
            (r[0] + ((r[2] - r[0]) as f64 * fx) as i32, r[1] + ((r[3] - r[1]) as f64 * fy) as i32)
        };
        let (_, _, w, _) = desk.bounds();
        let (dw, _) = delivered_size((desk.bounds().2, desk.bounds().3));
        let tol = (w as f64 / dw as f64).ceil() as i32 + 1;
        let click_args = |p: (f64, f64)| vec!["gui".to_string(), "click".into(), format!("{:.3}", p.0), format!("{:.3}", p.1), "--records".into()];
        let click = |p: (f64, f64)| {
            let a = click_args(p);
            ask(&runner, &a.iter().map(String::as_str).collect::<Vec<_>>())
        };

        // reading
        let shot = ask(&runner, &["gui", "screenshot"]);
        c.check(shot["ok"] == true, "screenshot");
        let (a, b) = (at(0.2, 0.2), at(0.8, 0.8));
        let (fa, fb) = (frame_point(&desk, a.0, a.1), frame_point(&desk, b.0, b.1));
        let z = ask(&runner, &["gui", "zoom", &format!("{:.0}", fa.0), &format!("{:.0}", fa.1), &format!("{:.0}", fb.0), &format!("{:.0}", fb.1)]);
        c.check(z["ok"] == true && z["images"][0].is_string(), "zoom over the target window");

        // clicks
        let spots = [(0.25, 0.4), (0.75, 0.4), (0.5, 0.8)];
        for (i, (fx, fy)) in spots.iter().enumerate() {
            ask(&runner, &["gui", "screenshot"]);
            let n = at(*fx, *fy);
            let v = click(frame_point(&desk, n.0, n.1));
            c.check(v["ok"] == true && v["completed"] == 1, format!("click {i} accepted"));
            std::thread::sleep(Duration::from_millis(300));
            let downs = lines(&log, "down");
            let got = downs.get(i).and_then(|l| screen_xy(l));
            c.check(got.is_some_and(|g| (g.0 - n.0).abs() <= tol && (g.1 - n.1).abs() <= tol), format!("click {i} landed at {n:?}, target saw {got:?} (tolerance {tol})"));
        }

        // wheel: two actions of three notches each
        ask(&runner, &["gui", "screenshot"]);
        let m = at(0.5, 0.5);
        let f = frame_point(&desk, m.0, m.1);
        let payload = format!(r#"{{"actions":[{{"type":"scroll","x":{:.3},"y":{:.3},"scroll_y":-3}},{{"type":"scroll","x":{:.3},"y":{:.3},"scroll_y":-3}}]}}"#, f.0, f.1, f.0, f.1);
        let v = ask(&runner, &["gui", "--json", &payload, "--records"]);
        std::thread::sleep(Duration::from_millis(300));
        let wheels = lines(&log, "wheel");
        c.check(v["ok"] == true && wheels.len() == 2 && wheels.iter().all(|l| l.contains("delta=360")), format!("two wheel actions of 3 notches ({} lines: {wheels:?})", wheels.len()));

        // refusals: none of them may reach the target
        let downs_before = lines(&log, "down").len();
        let code = |v: &Value| v["code"].as_str().unwrap_or("").to_string();

        ask(&runner, &["gui", "screenshot"]);
        let n = at(0.5, 0.5);
        let p = frame_point(&desk, n.0, n.1);
        let old = find(&desk, TARGET).unwrap_or(win.clone());
        unsafe { SetWindowPos(old.hwnd, 0, old.rect[0] + 40, old.rect[1] + 40, 0, 0, 0x15) };
        std::thread::sleep(Duration::from_millis(300));
        let v = click(p);
        c.check(code(&v) == "gui_frame_stale" && v["input_emitted"] == false, format!("moved window refused ({})", code(&v)));
        unsafe { SetWindowPos(old.hwnd, 0, old.rect[0], old.rect[1], 0, 0, 0x15) };
        std::thread::sleep(Duration::from_millis(300));

        ask(&runner, &["gui", "screenshot"]);
        let r = find(&desk, TARGET).map(|w| w.rect).unwrap_or(win.rect);
        let n = at(0.5, 0.5);
        let p = frame_point(&desk, n.0, n.1);
        let cover_at = format!("{},{}", n.0 - 60, n.1 - 60);
        let cover = spawn(&["--title", COVER, "--flat", "--at", &cover_at, "--size", "120,120"]);
        let covered = cover.is_ok() && wait_for(&desk, COVER).is_some();
        c.check(covered, format!("cover window opened over the target {r:?}"));
        if covered {
            let v = click(p);
            c.check(code(&v) == "gui_frame_stale" && v["input_emitted"] == false, format!("covered point refused as stale ({})", code(&v)));
            ask(&runner, &["gui", "screenshot"]);
            let v = click(p);
            c.check(code(&v) == "target_window_mismatch" && v["input_emitted"] == false, format!("covered point on a fresh frame refused as wrong window ({})", code(&v)));
        }
        if let Ok(mut cv) = cover {
            let _ = cv.kill();
        }
        std::thread::sleep(Duration::from_millis(300));

        // a pixel of the virtual desktop that no monitor shows
        ask(&runner, &["gui", "screenshot"]);
        let (ox, oy, w, h) = desk.bounds();
        let dead = (0..h as i32).step_by(20).flat_map(|y| (0..w as i32).step_by(20).map(move |x| (ox + x, oy + y))).find(|&(x, y)| !desk.monitor_contains(x, y));
        match dead {
            Some((x, y)) => {
                let v = click(frame_point(&desk, x, y));
                c.check(code(&v) == "target_off_monitor" && v["input_emitted"] == false, format!("point on no monitor ({x},{y}) refused ({})", code(&v)));
            }
            None => println!("SKIP no dead space in the virtual desktop on this machine"),
        }

        c.check(lines(&log, "down").len() == downs_before, "no refused request reached the target");
        let _ = target.kill();
        let failed = c.0.iter().filter(|(ok, _)| !ok).count();
        println!("{} checks, {failed} failed", c.0.len());
        i32::from(failed > 0)
    }
}

fn main() {
    #[cfg(windows)]
    std::process::exit(app::run());
    #[cfg(not(windows))]
    eprintln!("input_selftest is only built for Windows");
}
