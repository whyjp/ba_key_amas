//! Times the add-on's loop stage by stage, with no input to the game:
//!   cargo run --release -p baocr --example perf_loop [frame.png ...]
//! Offline stages run on the given frames (default: the quick-formation and battle fixtures).
//! LIVE=1 adds read-only desktop captures (`Hcu::native`). OVERLAY=secs measures the overlay
//! thread's CPU with a synthetic card for that long (it opens its windows at the top right of
//! the screen; the game window is not looked for).
use baocr_assist::{Assistant, Observation, Observer, Settings, Surface};
use baocr_core::ocr_api::Ocr;
use baocr_extract::scene::{MissionRef, Scene};
use baocr_hcu::{detect_region, game_capture, screen_kind, Hcu};
use baocr_overlay::{load_fonts, render_panel_hits, Dock, Mark, Overlay, PanelState};
use baocr_pixels::{frame_distance, frame_key};
use baocr_wikidb::Wiki;
use std::cell::Cell;
use std::path::PathBuf;
use std::time::{Duration, Instant};

const RUNS: usize = 7;

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.total_cmp(b));
    v[v.len() / 2]
}

fn time<T>(runs: usize, mut f: impl FnMut() -> T) -> f64 {
    f();
    median((0..runs).map(|_| {
        let t = Instant::now();
        std::hint::black_box(f());
        t.elapsed().as_secs_f64() * 1000.0
    }).collect())
}

#[derive(Default)]
struct Counting {
    last: std::cell::RefCell<Option<PanelState>>,
    main: Cell<u32>,
    ingame: Cell<u32>,
    marks: Cell<u32>,
}

impl Surface for &Counting {
    fn main_card(&self, card: PanelState) {
        *self.last.borrow_mut() = Some(card);
        self.main.set(self.main.get() + 1);
    }
    fn ingame_card(&self, _: Option<PanelState>) {
        self.ingame.set(self.ingame.get() + 1);
    }
    fn marks(&self, _: Vec<Mark>) {
        self.marks.set(self.marks.get() + 1);
    }
    fn take_events(&self) -> Vec<String> {
        Vec::new()
    }
    fn set_dock(&self, _: Dock) {}
    fn lock_input(&self, _: bool) -> bool {
        false
    }
}

#[cfg(windows)]
fn cpu_seconds() -> f64 {
    #[repr(C)]
    #[derive(Default, Clone, Copy)]
    struct FileTime {
        lo: u32,
        hi: u32,
    }
    extern "system" {
        fn GetCurrentProcess() -> isize;
        fn GetProcessTimes(p: isize, c: *mut FileTime, e: *mut FileTime, k: *mut FileTime, u: *mut FileTime) -> i32;
    }
    let (mut c, mut e, mut k, mut u) = (FileTime::default(), FileTime::default(), FileTime::default(), FileTime::default());
    unsafe { GetProcessTimes(GetCurrentProcess(), &mut c, &mut e, &mut k, &mut u) };
    let t = |f: FileTime| ((f.hi as u64) << 32 | f.lo as u64) as f64 / 1e7;
    t(k) + t(u)
}
#[cfg(not(windows))]
fn cpu_seconds() -> f64 {
    0.0
}

#[cfg(windows)]
fn stand_in_window(title: &str) -> isize {
    extern "system" {
        fn CreateWindowExW(ex: u32, class: *const u16, title: *const u16, style: u32, x: i32, y: i32, w: i32, h: i32, parent: isize, menu: isize, inst: isize, param: *const u8) -> isize;
        fn SetForegroundWindow(h: isize) -> i32;
    }
    let wide = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
    unsafe {
        let h = CreateWindowExW(0, wide("STATIC").as_ptr(), wide(title).as_ptr(), 0x8000_0000 | 0x1000_0000, 60, 40, 1280, 960, 0, 0, 0, std::ptr::null());
        SetForegroundWindow(h);
        h
    }
}
#[cfg(not(windows))]
fn stand_in_window(_: &str) -> isize {
    0
}

fn main() -> anyhow::Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../");
    let wiki = Wiki::open(&std::env::var("WIKI").map(PathBuf::from).unwrap_or_else(|_| "D:/work-github/bluearchive-ocr/wikidb/wiki.db".into()))?;
    let (students, types) = (wiki.students()?, wiki.type_table()?);
    let models = std::env::var("BAOCR_MODELS").map(PathBuf::from).unwrap_or_else(|_| baocr_ocr::PpOcr::default_models_dir());
    let ocr = baocr_ocr::PpOcr::new(&models)?;
    let fonts = load_fonts().ok_or_else(|| anyhow::anyhow!("no Korean font"))?;

    let mut frames: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    if frames.is_empty() {
        frames = ["quick-formation.png", "battle.png"].iter().map(|f| root.join("fixtures/screens").join(f)).filter(|p| p.exists()).collect();
    }
    println!("{:<26} {:>9} {:>9} {:>9} {:>10} {:>9}   (ms, median of {RUNS})", "frame", "key+dist", "kind", "ocr.boxes", "observe", "present");
    let mission = MissionRef { number: Some((19, 3)), stage_no: None, place: None, name: "설원 지구 제2 지역".into() };
    for path in &frames {
        let img = image::open(path)?.to_rgb8();
        let key = frame_key(&img);
        let t_key = time(RUNS, || frame_distance(&frame_key(&img), &key));
        let t_kind = time(RUNS, || screen_kind(&img));
        let t_ocr = time(3, || ocr.boxes(&img).unwrap_or_default());
        let boxes = ocr.boxes(&img).unwrap_or_default();
        let mut observer = Observer::new(&ocr, &students);
        let t_obs = time(3, || observer.observe(&img, &boxes, Scene::Unknown));
        let obs = observer.observe(&img, &boxes, Scene::Unknown);
        let counting = Counting::default();
        let mut a = Assistant::new(Observer::new(&ocr, &students), &wiki, &students, &types, Settings::default(), None, &counting);
        a.set_game_present(true);
        let squad = students.iter().take(4).map(|s| s.id).collect();
        let shown = if obs.scene == Scene::Unknown { Observation { mission: Some(mission.clone()), squad, ..Observation::new(Scene::Squad) } } else { obs.clone() };
        let t_present = time(RUNS, || a.show(shown.clone()));
        let name = path.file_name().unwrap().to_string_lossy();
        println!("{name:<26} {t_key:>9.2} {t_kind:>9.2} {t_ocr:>9.1} {t_obs:>10.1} {t_present:>9.2}   scene={:?}", obs.scene);
        let before = (counting.main.get(), counting.ingame.get(), counting.marks.get());
        for _ in 0..10 {
            a.show(shown.clone());
        }
        println!("{:<26} surface calls for 10 identical presents: main {}, ingame {}, marks {}", "", counting.main.get() - before.0, counting.ingame.get() - before.1, counting.marks.get() - before.2);
    }

    // the overlay's own per-tick work for the card the assistant would show
    {
        let counting = Counting::default();
        let mut a = Assistant::new(Observer::new(&ocr, &students), &wiki, &students, &types, Settings::default(), None, &counting);
        a.set_game_present(true);
        a.show(Observation { mission: Some(mission.clone()), squad: students.iter().take(4).map(|s| s.id).collect(), ..Observation::new(Scene::Squad) });
        let card = counting.last.borrow().clone().unwrap();
        let width = baocr_overlay::panel_width(1280) as u32;
        let t_render = time(RUNS, || render_panel_hits(width, &card, &fonts));
        let t_debug = time(RUNS, || format!("{card:?}").len());
        let rows = card.rows.len();
        println!("overlay per tick: render_panel_hits({rows}-row main card at {width}px) {t_render:.2} ms, Debug string of it {t_debug:.3} ms");
    }

    if std::env::var_os("LIVE").is_some() {
        let h = Hcu::native();
        let t = Instant::now();
        let region = detect_region(&h)?;
        println!("live: detect_region {:.0} ms -> {region:.1?}", t.elapsed().as_secs_f64() * 1000.0);
        let first = game_capture(&h, region)?;
        let t_cap = time(10, || game_capture(&h, region).unwrap());
        let t_key = time(RUNS, || frame_key(&first));
        println!("live: game_capture {t_cap:.0} ms (median of 10), frame_key {t_key:.2} ms");
    }

    if let Some(secs) = std::env::var("OVERLAY").ok().and_then(|s| s.parse::<u64>().ok()) {
        // a plain window stands in for the game: the in-game layers are drawn over it as over the game
        std::env::set_var("BAOCR_OVERLAY_TITLE", "perf_loop stand-in");
        let _stand_in = stand_in_window("perf_loop stand-in");
        let overlay = Overlay::start_hidden();
        let mut a = Assistant::new(Observer::new(&ocr, &students), &wiki, &students, &types, Settings::default(), None, overlay);
        a.set_game_present(true);
        let cards = students.iter().skip(4).take(6).enumerate().map(|(i, s)| baocr_extract::CardName { text: s.name.clone(), x: 629.0 + 120.0 * (i % 3) as f32, y0: 428.0 + 200.0 * (i / 3) as f32 }).collect();
        a.show(Observation { mission: Some(mission), squad: students.iter().take(4).map(|s| s.id).collect(), cards, ..Observation::new(Scene::QuickFormation) });
        std::thread::sleep(Duration::from_secs(2));
        let (c0, t0) = (cpu_seconds(), Instant::now());
        std::thread::sleep(Duration::from_secs(secs));
        let (c1, wall) = (cpu_seconds(), t0.elapsed().as_secs_f64());
        println!("overlay idle (card + in-game layers, nothing changing): {:.1} ms CPU/s = {:.1}% of one core ({secs}s window)", (c1 - c0) / wall * 1000.0, (c1 - c0) / wall * 100.0);
    }
    Ok(())
}
