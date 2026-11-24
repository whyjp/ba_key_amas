//! `BAOCR_PROFILE=1`: how long each stage of the add-on's loop takes, printed every 10 s.
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

pub struct Profile {
    on: bool,
    since: Instant,
    stages: BTreeMap<&'static str, (Duration, u32)>,
}

impl Profile {
    pub fn from_env() -> Profile {
        Profile { on: std::env::var_os("BAOCR_PROFILE").is_some(), since: Instant::now(), stages: BTreeMap::new() }
    }

    pub fn time<T>(&mut self, stage: &'static str, f: impl FnOnce() -> T) -> T {
        if !self.on {
            return f();
        }
        let t = Instant::now();
        let r = f();
        let e = self.stages.entry(stage).or_default();
        e.0 += t.elapsed();
        e.1 += 1;
        r
    }

    pub fn report(&mut self) {
        if !self.on || self.since.elapsed() < Duration::from_secs(10) {
            return;
        }
        let wall = self.since.elapsed().as_secs_f64();
        let busy: f64 = self.stages.values().map(|(d, _)| d.as_secs_f64()).sum();
        let mut line = format!("profile {wall:.1}s (stages {:.0}% of wall):", busy / wall * 100.0);
        for (name, (d, n)) in &self.stages {
            line += &format!(" {name} {:.1}ms x{n};", d.as_secs_f64() * 1000.0 / *n as f64);
        }
        eprintln!("{line}");
        self.stages.clear();
        self.since = Instant::now();
    }
}
