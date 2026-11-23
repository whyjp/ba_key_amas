//! Test doubles: a scripted "game" behind the `Runner` trait, and a fake `ScanSink`.
use crate::hcu::{Hcu, Runner};
use anyhow::Result;
use baocr_core::model::Student;
use baocr_core::ocr_api::ScanSink;
use image::{Rgb, RgbImage};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

pub fn flat(v: u8) -> RgbImage {
    RgbImage::from_pixel(1280, 960, Rgb([v, v, v]))
}

struct St {
    screen: usize,
    /// screen id leased by the last screenshot (what zooms show).
    leased: usize,
    inputs: Vec<String>,
    apps_calls: usize,
    active_calls: usize,
    animate_to: usize,
    zooms: usize,
}

/// A fake game window: `frame(screen)` is the grey level shown on that screen,
/// clicks and scrolls mutate the screen id. Only input is logged; reads are free.
#[derive(Clone)]
pub struct World {
    st: Arc<Mutex<St>>,
    frame: Arc<dyn Fn(usize) -> u8 + Send + Sync>,
    on_click: Arc<dyn Fn(&mut usize, f64, f64) + Send + Sync>,
    on_scroll: Arc<dyn Fn(&mut usize, i32) + Send + Sync>,
}

impl World {
    pub fn new(
        frame: impl Fn(usize) -> u8 + Send + Sync + 'static,
        on_click: impl Fn(&mut usize, f64, f64) + Send + Sync + 'static,
        on_scroll: impl Fn(&mut usize, i32) + Send + Sync + 'static,
    ) -> Self {
        World {
            st: Arc::new(Mutex::new(St { screen: 0, leased: 0, inputs: vec![], apps_calls: 0, active_calls: usize::MAX, animate_to: 0, zooms: 0 })),
            frame: Arc::new(frame),
            on_click: Arc::new(on_click),
            on_scroll: Arc::new(on_scroll),
        }
    }

    /// After a click, every screenshot advances the screen by one until `n` (a slide-in animation).
    pub fn animate_to(self, n: usize) -> Self {
        self.st.lock().unwrap().animate_to = n;
        self
    }

    /// The game window is the active one for only the first `n` `apps` calls.
    pub fn active_for(self, n: usize) -> Self {
        self.st.lock().unwrap().active_calls = n;
        self
    }

    pub fn inputs(&self) -> Vec<String> {
        self.st.lock().unwrap().inputs.clone()
    }

    pub fn hcu(&self) -> Hcu {
        Hcu::with_runner(Box::new(self.clone())).without_sleep()
    }
}

impl Runner for World {
    fn run(&self, args: &[String]) -> Result<String> {
        let a: Vec<&str> = args.iter().map(String::as_str).collect();
        let mut st = self.st.lock().unwrap();
        let ok = r#"{"ok":true,"input_emitted":true,"completed":1}"#.to_string();
        Ok(match a.as_slice() {
            ["apps"] => {
                st.apps_calls += 1;
                let active = if st.apps_calls <= st.active_calls { "  active" } else { "" };
                format!("w22 \"New Tab - Aside\"  app=Aside\nw13 \"Blue Archive\"  app=BlueArchive{active}\nw15 \"Main Page - Blue Archive Wiki - Chrome\"  app=chrome\n")
            }
            ["activate", r, "--records"] => {
                assert_eq!(*r, "w13", "must activate the exact-title window");
                st.inputs.push(format!("activate {r}"));
                ok
            }
            ["gui", "screenshot", "--records"] => {
                if st.animate_to > 0 && st.screen > 0 && st.screen < st.animate_to {
                    st.screen += 1;
                }
                st.leased = st.screen;
                r#"{"ok":true,"screen":{"width":1280,"height":960}}"#.to_string()
            }
            ["gui", "zoom", x0, y0, x1, y1, "--records"] => {
                // zoom looks at the live screen; a capture is two zooms, the first one sets what both show
                st.zooms += 1;
                if st.zooms % 2 == 1 {
                    st.leased = st.screen;
                    if st.animate_to > 0 && st.screen > 0 && st.screen < st.animate_to {
                        st.screen += 1;
                    }
                }
                format!(
                r#"{{"ok":true,"images":["z:{x0}:{y0}:{x1}:{y1}"],"evidence":{{"zoom":{{"region":[{x0},{y0},{x1},{y1}],"scale_x":1.0,"scale_y":1.0}}}}}}"#
                )
            }
            ["gui", "click", x, y, "--records"] => {
                assert!(st.leased < usize::MAX);
                st.inputs.push(format!("click {x} {y}"));
                let (xf, yf) = (x.parse::<f64>().unwrap(), y.parse::<f64>().unwrap());
                let mut s = st.screen;
                (self.on_click)(&mut s, xf, yf);
                st.screen = s;
                ok
            }
            ["gui", "--json", payload, "--records"] => {
                let v: serde_json::Value = serde_json::from_str(payload).unwrap();
                let acts = v["actions"].as_array().unwrap();
                for act in acts {
                    let (xf, yf) = (act["x"].as_f64().unwrap(), act["y"].as_f64().unwrap());
                    let mut s = st.screen;
                    match act["type"].as_str().unwrap() {
                        "scroll" => {
                            let d = act["scroll_y"].as_i64().unwrap() as i32;
                            st.inputs.push(format!("scroll {xf:.2} {yf:.2} {d}"));
                            (self.on_scroll)(&mut s, d);
                        }
                        "click" => {
                            st.inputs.push(format!("click {xf:.2} {yf:.2}"));
                            (self.on_click)(&mut s, xf, yf);
                        }
                        other => panic!("unexpected action {other}"),
                    }
                    st.screen = s;
                }
                format!(r#"{{"ok":true,"input_emitted":true,"completed":{}}}"#, acts.len())
            }
            other => panic!("unexpected HCU call {other:?}"),
        })
    }

    fn load_image(&self, path: &str) -> Result<RgbImage> {
        let n: Vec<i64> = path[2..].split(':').map(|s| s.parse().unwrap()).collect();
        let v = (self.frame)(self.st.lock().unwrap().leased);
        Ok(RgbImage::from_pixel((n[2] - n[0]) as u32, (n[3] - n[1]) as u32, Rgb([v, v, v])))
    }
}

/// A `ScanSink` that records submissions.
pub struct FakeSink {
    pub submitted: Mutex<Vec<(usize, BTreeMap<String, RgbImage>)>>,
    pub calibrated: bool,
    pub abort: Mutex<Option<(String, String)>>,
    /// abort once this many students were submitted
    pub abort_after: Mutex<Option<usize>>,
    /// what `peek_name` returns for a frame that was not submitted (None = unreadable)
    pub peek: Mutex<Option<String>>,
}

impl FakeSink {
    pub fn new() -> Self {
        FakeSink { submitted: Mutex::new(vec![]), calibrated: true, abort: Mutex::new(None), abort_after: Mutex::new(None), peek: Mutex::new(None) }
    }
    pub fn keys(&self) -> Vec<(usize, Vec<String>)> {
        self.submitted.lock().unwrap().iter().map(|(i, m)| (*i, m.keys().cloned().collect())).collect()
    }
}

impl ScanSink for FakeSink {
    fn submit(&self, i: usize, frames: BTreeMap<String, RgbImage>) {
        self.submitted.lock().unwrap().push((i, frames));
    }
    fn name_of(&self, i: usize) -> String {
        format!("student{i}")
    }
    fn peek_name(&self, _img: &RgbImage) -> String {
        self.peek.lock().unwrap().clone().unwrap_or_default()
    }
    fn first_student(&self) -> Result<Student> {
        let mut s = Student::default();
        if self.calibrated {
            s.level = Some(10);
        }
        Ok(s)
    }
    fn aborted(&self) -> Option<(String, String)> {
        if let Some(n) = *self.abort_after.lock().unwrap() {
            if self.submitted.lock().unwrap().len() >= n {
                return self.abort.lock().unwrap().clone();
            }
        }
        None
    }
}
