//! A model of the add-on loop (`cli/src/assist.rs`) for estimating reaction time under different
//! policies. It sends nothing and looks at no game: stage costs are inputs (env, defaults are the
//! numbers measured in docs/speed-research.md) and the screen is a synthetic timeline.
//!   cargo run --release -p baocr --example speed_model
//! Reaction = from the screen reaching its final state to the card being on screen.
//! A transition: the view changes for A ms and is final afterwards. A scroll: the view moves up to
//! the final state at 0 (the wheel's last turn is INERTIA ms earlier).
//! Simplifications, all optimistic for the faster policies: OCR in the background costs the loop
//! nothing (real OCR uses every core and slows captures), the game answers a click after CLICK_LAG.

#[derive(Clone, Copy)]
struct Costs {
    capture: f64,
    key: f64,
    ocr: f64,
    observe: f64,
    draw: f64,
    tick: f64,
}

#[derive(Clone, Copy, Default)]
struct Policy {
    name: &'static str,
    /// overlay thread wakes on change instead of polling
    wake: bool,
    /// OCR cost override (ms), e.g. grid-only OCR
    ocr: Option<f64>,
    /// start OCR on a frame before it is confirmed still; adopt the result when the next frame matches
    speculative: bool,
    /// input hints (click look at +first, trusted look at +trusted; scroll-stopped look)
    hints: bool,
    /// settle needs this many matching frames (baseline 1 pair = 1)
    pairs: u32,
    /// idle pause after a read view
    idle: Option<f64>,
    /// pause while a view is settling
    settle: Option<f64>,
    /// cost of the frame key (ms) added to every look; None keeps `Costs::key`
    key: Option<f64>,
}

/// A frame sampled at time `t` differs from one sampled at `u` unless both are before the motion or both after it.
fn differ(m: (f64, f64), t: f64, u: f64) -> bool {
    let (a, b) = (t.min(u), t.max(u));
    !(b < m.0 || a >= m.1)
}

/// `sleep` in 30 ms slices as `wait` does: a pause lasts a whole number of slices plus timer slack.
fn slept(ms: f64) -> f64 {
    (ms / 30.0).ceil() * 30.0 + 2.0
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// One run: motion interval `m` (start, end), hint times `hints` (due, trusted). Returns the reaction in ms.
fn run(c: Costs, p: Policy, m: (f64, f64), hints: &[(f64, bool)], rng: &mut Rng) -> f64 {
    let ocr = p.ocr.unwrap_or(c.ocr);
    let idle = p.idle.unwrap_or(300.0);
    let period = c.capture + c.key + idle;
    let mut t = -rng.next() * period.max(m.1 + 1.0) * 1.0 - 1.0; // the loop is somewhere in its idle cycle
    let mut prev: Option<f64> = Some(-1e9);
    let mut handled: Option<f64> = Some(-1e9);
    let (mut still, mut read) = (99u32, true);
    let mut job: Option<(f64, f64)> = None; // (sample time, ready at)
    let mut hints: Vec<(f64, bool)> = if p.hints { hints.to_vec() } else { Vec::new() };
    hints.sort_by(|a, b| a.0.total_cmp(&b.0));
    for _ in 0..400 {
        // one look at the game
        let start = t;
        let trusted = hints.first().is_some_and(|h| h.0 <= start && h.1);
        if hints.first().is_some_and(|h| h.0 <= start) {
            hints.remove(0);
        }
        let sample = start + c.capture / 2.0;
        t = start + c.capture + p.key.unwrap_or(c.key);
        let moving = prev.is_none_or(|q| differ(m, q, sample));
        still = if moving { 0 } else { still + 1 };
        let new_view = handled.is_none_or(|h| differ(m, h, sample));
        // speculative job on any unread view, one at a time
        if p.speculative && new_view && job.is_none_or(|(_, ready)| ready <= t) && job.is_none_or(|(s, _)| differ(m, s, sample)) {
            job = Some((sample, t + ocr + c.observe));
        }
        let confirmed = still > p.pairs.saturating_sub(1) && still >= p.pairs.max(1);
        if (confirmed && new_view) || (trusted && new_view) {
            let adopt = job.filter(|(s, _)| prev.is_some_and(|q| !differ(m, *s, q)) || *s == sample);
            match (p.speculative, adopt) {
                (true, Some((_, ready))) => t = t.max(ready),
                _ => t += ocr + c.observe,
            }
            handled = Some(sample);
            if sample >= m.1 {
                let wait = if p.wake { 0.0 } else { rng.next() * c.tick };
                return t + wait + c.draw - m.1;
            }
        }
        prev = Some(sample);
        read = handled.is_some_and(|h| !differ(m, h, sample));
        let pause = if !moving && read && still >= 3 { idle } else { p.settle.unwrap_or(60.0) };
        let mut next = t + slept(pause);
        // a hint cuts the wait short
        if let Some(h) = hints.first() {
            if h.0 < next {
                next = h.0.max(t);
            }
        }
        t = next;
    }
    f64::NAN
}

fn stats(mut v: Vec<f64>) -> (f64, f64, f64) {
    v.sort_by(|a, b| a.total_cmp(b));
    (v.iter().sum::<f64>() / v.len() as f64, v[v.len() / 2], v[v.len() * 9 / 10])
}

fn env(name: &str, default: f64) -> f64 {
    std::env::var(name).ok().and_then(|s| s.parse().ok()).unwrap_or(default)
}

fn main() {
    let c = Costs { capture: env("CAPTURE", 110.0), key: env("KEY", 7.0), ocr: env("OCR", 534.0), observe: env("OBSERVE", 87.0), draw: env("DRAW", 6.0), tick: env("TICK", 90.0) };
    let (grid_ocr, click_lag, _) = (env("GRID_OCR", 316.0), env("CLICK_LAG", 60.0), 0.0);
    let base = Policy { name: "today", pairs: 1, ..Default::default() };
    let policies = [
        base,
        Policy { name: "+ overlay wake", wake: true, ..base },
        Policy { name: "+ idle pause 150", idle: Some(150.0), ..base },
        Policy { name: "+ settle pause 20", settle: Some(20.0), ..base },
        Policy { name: "+ fast key (0.6 ms)", key: Some(0.6), ..base },
        Policy { name: "+ DirectML (OCR 190 ms)", ocr: Some(env("DML_OCR", 190.0)), ..base },
        Policy { name: "DirectML+wake+settle20+key", ocr: Some(env("DML_OCR", 190.0)), wake: true, settle: Some(20.0), key: Some(0.6), ..base },
        Policy { name: "+ band-rec OCR (scene known)", ocr: Some(env("BAND_OCR", 77.0)), ..base },
        Policy { name: "+ grid-only OCR", ocr: Some(grid_ocr), ..base },
        Policy { name: "+ speculative OCR", speculative: true, ..base },
        Policy { name: "+ input hints", hints: true, ..base },
        Policy { name: "wake+grid OCR", wake: true, ocr: Some(grid_ocr), ..base },
        Policy { name: "wake+grid+speculative", wake: true, ocr: Some(grid_ocr), speculative: true, ..base },
        Policy { name: "wake+grid+hints", wake: true, ocr: Some(grid_ocr), hints: true, ..base },
        Policy { name: "wake+band+hints (scene known)", wake: true, ocr: Some(env("BAND_OCR", 77.0)), hints: true, ..base },
        Policy { name: "wake+band+spec+hints (known)", wake: true, ocr: Some(env("BAND_OCR", 77.0)), speculative: true, hints: true, settle: Some(20.0), key: Some(0.6), ..base },
        Policy { name: "wake+grid+spec+hints", wake: true, ocr: Some(grid_ocr), speculative: true, hints: true, ..base },
    ];
    println!("costs: capture {} key {} ocr {} (grid {}) observe {} draw {} tick {} ms; click lag {click_lag}", c.capture, c.key, c.ocr, grid_ocr, c.observe, c.draw, c.tick);
    let scenarios: [(&str, f64, bool, f64); 5] = [
        ("instant cut (click, A=0)", 0.0, false, 0.0),
        ("slide-in 250 ms (click)", 250.0, false, 0.0),
        ("slow animation 600 ms (click)", 600.0, false, 0.0),
        ("scroll stop, inertia 100 ms (hint after 160 ms of quiet is on time)", 0.0, true, 100.0),
        ("scroll stop, inertia 250 ms (the same hint fires while the list still moves)", 0.0, true, 250.0),
    ];
    for (name, a, scroll, inertia) in scenarios {
        println!("\n{name}: reaction after the view is final, ms (mean / median / p90)");
        for p in policies {
            let mut rng = Rng(0x9E3779B97F4A7C15);
            let v: Vec<f64> = (0..4000)
                .map(|_| {
                    // click at 0 - lag: the view moves from 0 to a; scroll: moves up to 0, last wheel `inertia` earlier
                    if scroll {
                        run(c, p, (-1500.0, 0.0), &[(-inertia + 160.0, true)], &mut rng)
                    } else {
                        run(c, p, (0.0, a), &[(-click_lag + 80.0, false), (-click_lag + 450.0, true)], &mut rng)
                    }
                })
                .collect();
            let (mean, med, p90) = stats(v);
            println!("  {:<26} {mean:>6.0} / {med:>6.0} / {p90:>6.0}", p.name);
        }
    }
}
