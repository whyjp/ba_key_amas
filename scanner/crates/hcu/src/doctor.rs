//! `baocr --doctor`: reads the desktop and says, in plain Korean lines, whether the game can be
//! driven from here. Sends no input. The facts are read by [`probe`]; [`evaluate`] is pure.

use baocr_core::layout::Layout;

/// What was found out about the game window.
#[derive(Debug, Clone, PartialEq)]
pub struct GameFacts {
    pub hwnd: isize,
    /// Windows whose title is exactly the game's (more than one makes the choice ambiguous).
    pub same_title: usize,
    pub minimized: bool,
    /// Client area: left, top, right, bottom in desktop pixels.
    pub client: [i32; 4],
    /// Whether the game process runs with an elevated token; None when that could not be read.
    pub elevated: Option<bool>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Facts {
    pub game: Option<GameFacts>,
    pub dpi_aware: bool,
    /// System DPI (96 = 100 %).
    pub dpi: u32,
    pub monitors: Vec<[i32; 4]>,
    /// left, top, right, bottom of the virtual desktop.
    pub virtual_desktop: [i32; 4],
    /// Whether this program runs elevated; None when unknown.
    pub own_elevated: Option<bool>,
    pub foreground: isize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Check {
    pub ok: bool,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    pub checks: Vec<Check>,
}

impl Report {
    /// One line per check: `OK` or `경고`, then the finding.
    pub fn render(&self) -> String {
        self.checks.iter().map(|c| format!("{}  {}", if c.ok { "OK  " } else { "경고" }, c.text)).collect::<Vec<_>>().join("\n")
    }

    pub fn healthy(&self) -> bool {
        self.checks.iter().all(|c| c.ok)
    }
}

fn area(r: [i32; 4]) -> i64 {
    (r[2] - r[0]).max(0) as i64 * (r[3] - r[1]).max(0) as i64
}

fn on_a_monitor(monitors: &[[i32; 4]], x: i32, y: i32) -> bool {
    monitors.iter().any(|m| x >= m[0] && x < m[2] && y >= m[1] && y < m[3])
}

pub fn evaluate(f: &Facts) -> Report {
    let mut checks = Vec::new();
    let mut add = |ok: bool, text: String| checks.push(Check { ok, text });

    add(
        f.dpi_aware,
        if f.dpi_aware { format!("DPI 인식이 켜져 있습니다 (화면 배율 {}%). 좌표는 실제 픽셀입니다.", f.dpi * 100 / 96) } else { "DPI 인식이 꺼져 있어 좌표가 배율만큼 어긋날 수 있습니다.".into() },
    );

    let (vw, vh) = (f.virtual_desktop[2] - f.virtual_desktop[0], f.virtual_desktop[3] - f.virtual_desktop[1]);
    let covered: i64 = f.monitors.iter().map(|m| area(*m)).sum();
    let dead = area(f.virtual_desktop) - covered;
    add(!f.monitors.is_empty(), format!("모니터 {}개, 가상 데스크톱 {vw}x{vh}.", f.monitors.len()));
    if dead > 0 {
        add(false, format!("가상 데스크톱에 모니터가 없는 빈 영역이 있습니다 ({}%). 그 영역으로는 입력을 보내지 않습니다.", dead * 100 / area(f.virtual_desktop).max(1)));
    } else {
        add(true, "모니터가 가상 데스크톱을 빈틈없이 채웁니다.".into());
    }

    let Some(g) = &f.game else {
        add(false, "제목이 정확히 \"Blue Archive\"인 창을 찾지 못했습니다. 게임을 창 모드로 실행하세요.".into());
        return Report { checks };
    };
    add(
        g.same_title == 1,
        if g.same_title == 1 { "게임 창을 찾았습니다.".into() } else { format!("제목이 같은 창이 {}개입니다. 하나만 남기세요.", g.same_title) },
    );
    add(!g.minimized, if g.minimized { "게임 창이 최소화되어 있습니다.".into() } else { "게임 창이 최소화되어 있지 않습니다.".into() });

    let (cw, ch) = (g.client[2] - g.client[0], g.client[3] - g.client[1]);
    let shape = Layout::detect([0.0, 0.0, cw as f64, ch as f64]);
    add(
        shape.is_some(),
        match shape {
            Some(l) => format!("게임 화면 크기 {cw}x{ch}, 화면 비율 {}.", l.label()),
            None => format!("게임 화면 크기 {cw}x{ch}는 4:3도 16:9도 아닙니다. 게임 옵션 > 그래픽에서 화면 비율을 4:3 또는 16:9로 하고 창 모드로 실행하세요."),
        },
    );
    // a 5x5 grid of points, so a gap between two monitors under the middle of the window is seen too
    let inside = (0..5).all(|i| (0..5).all(|j| on_a_monitor(&f.monitors, g.client[0] + (cw - 1) * i / 4, g.client[1] + (ch - 1) * j / 4)));
    add(inside, if inside { "게임 화면이 모니터 안에 있습니다.".into() } else { "게임 화면의 일부가 모니터 밖에 있습니다. 창을 화면 안으로 옮기세요.".into() });

    match (g.elevated, f.own_elevated) {
        (Some(true), Some(false) | None) => add(false, "게임이 관리자 권한으로 실행 중이라 이 프로그램의 마우스 입력이 조용히 무시됩니다. 이 프로그램도 관리자로 실행하거나 게임을 일반 권한으로 실행하세요.".into()),
        (Some(_), Some(_)) => add(true, "게임과 이 프로그램의 권한 수준이 입력을 보내기에 충분합니다.".into()),
        _ => add(false, "게임 또는 이 프로그램의 권한을 읽지 못했습니다. 관리자 권한으로 실행 중이면 입력이 무시됩니다.".into()),
    }

    add(
        f.foreground == g.hwnd,
        if f.foreground == g.hwnd { "게임 창이 활성 창입니다.".into() } else { "게임 창이 활성 창이 아닙니다 (스캔을 시작할 때 활성화합니다).".into() },
    );
    Report { checks }
}

#[cfg(windows)]
pub fn probe() -> Facts {
    use std::ffi::c_void;
    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct Rect {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }
    #[repr(C)]
    struct Point {
        x: i32,
        y: i32,
    }
    #[link(name = "user32")]
    extern "system" {
        fn SetProcessDPIAware() -> i32;
        fn IsProcessDPIAware() -> i32;
        fn GetDpiForSystem() -> u32;
        fn GetSystemMetrics(index: i32) -> i32;
        fn EnumDisplayMonitors(dc: isize, clip: *const Rect, cb: unsafe extern "system" fn(isize, isize, *const Rect, isize) -> i32, data: isize) -> i32;
        fn EnumWindows(cb: unsafe extern "system" fn(isize, isize) -> i32, l: isize) -> i32;
        fn IsWindowVisible(hwnd: isize) -> i32;
        fn GetWindowTextW(hwnd: isize, buf: *mut u16, max: i32) -> i32;
        fn IsIconic(hwnd: isize) -> i32;
        fn GetClientRect(hwnd: isize, r: *mut Rect) -> i32;
        fn ClientToScreen(hwnd: isize, p: *mut Point) -> i32;
        fn GetWindowThreadProcessId(hwnd: isize, pid: *mut u32) -> u32;
        fn GetForegroundWindow() -> isize;
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> isize;
        fn GetCurrentProcess() -> isize;
        fn CloseHandle(h: isize) -> i32;
    }
    #[link(name = "advapi32")]
    extern "system" {
        fn OpenProcessToken(process: isize, access: u32, token: *mut isize) -> i32;
        fn GetTokenInformation(token: isize, class: u32, info: *mut c_void, len: u32, returned: *mut u32) -> i32;
    }
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    const TOKEN_QUERY: u32 = 0x8;
    const TOKEN_ELEVATION: u32 = 20;

    unsafe fn token_elevated(process: isize) -> Option<bool> {
        let mut token = 0isize;
        if OpenProcessToken(process, TOKEN_QUERY, &mut token) == 0 {
            return None;
        }
        let (mut elevation, mut returned) = (0u32, 0u32);
        let ok = GetTokenInformation(token, TOKEN_ELEVATION, &mut elevation as *mut u32 as *mut c_void, 4, &mut returned) != 0;
        CloseHandle(token);
        ok.then_some(elevation != 0)
    }

    unsafe extern "system" fn each_monitor(_m: isize, _dc: isize, r: *const Rect, out: isize) -> i32 {
        let list = &mut *(out as *mut Vec<[i32; 4]>);
        list.push([(*r).left, (*r).top, (*r).right, (*r).bottom]);
        1
    }
    struct Search {
        title: Vec<u16>,
        found: Vec<isize>,
    }
    unsafe extern "system" fn each_window(hwnd: isize, out: isize) -> i32 {
        let s = &mut *(out as *mut Search);
        if IsWindowVisible(hwnd) != 0 {
            let mut buf = [0u16; 256];
            let n = GetWindowTextW(hwnd, buf.as_mut_ptr(), 256).max(0) as usize;
            if buf[..n] == s.title[..] {
                s.found.push(hwnd);
            }
        }
        1
    }

    unsafe {
        SetProcessDPIAware(); // what the add-on does at start-up
        let mut monitors: Vec<[i32; 4]> = Vec::new();
        EnumDisplayMonitors(0, std::ptr::null(), each_monitor, &mut monitors as *mut _ as isize);
        let (x, y) = (GetSystemMetrics(76), GetSystemMetrics(77));
        let virtual_desktop = [x, y, x + GetSystemMetrics(78), y + GetSystemMetrics(79)];

        let mut search = Search { title: crate::GAME_TITLE.encode_utf16().collect(), found: Vec::new() };
        EnumWindows(each_window, &mut search as *mut _ as isize);
        let game = search.found.first().map(|&hwnd| {
            let mut r = Rect::default();
            GetClientRect(hwnd, &mut r);
            let mut p = Point { x: 0, y: 0 };
            ClientToScreen(hwnd, &mut p);
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, &mut pid);
            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            let elevated = if process == 0 { None } else { token_elevated(process) };
            if process != 0 {
                CloseHandle(process);
            }
            GameFacts { hwnd, same_title: search.found.len(), minimized: IsIconic(hwnd) != 0, client: [p.x, p.y, p.x + r.right - r.left, p.y + r.bottom - r.top], elevated }
        });
        Facts {
            game,
            dpi_aware: IsProcessDPIAware() != 0,
            dpi: GetDpiForSystem(),
            monitors,
            virtual_desktop,
            own_elevated: token_elevated(GetCurrentProcess()),
            foreground: GetForegroundWindow(),
        }
    }
}

/// Read the desktop and judge it. Sends no input.
#[cfg(windows)]
pub fn run() -> Report {
    evaluate(&probe())
}

#[cfg(not(windows))]
pub fn run() -> Report {
    Report { checks: vec![Check { ok: false, text: "진단은 Windows에서만 지원됩니다.".into() }] }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn healthy() -> Facts {
        Facts {
            game: Some(GameFacts { hwnd: 7, same_title: 1, minimized: false, client: [100, 100, 1380, 1060], elevated: Some(false) }),
            dpi_aware: true,
            dpi: 144,
            monitors: vec![[0, 0, 1920, 1080], [1920, 0, 3840, 1080]],
            virtual_desktop: [0, 0, 3840, 1080],
            own_elevated: Some(false),
            foreground: 7,
        }
    }

    #[test]
    fn a_good_setup_is_all_ok() {
        let r = evaluate(&healthy());
        assert!(r.healthy(), "{}", r.render());
        assert!(r.render().lines().all(|l| l.starts_with("OK")));
    }

    #[test]
    fn an_elevated_game_is_the_warning_that_input_would_be_dropped() {
        let mut f = healthy();
        f.game.as_mut().unwrap().elevated = Some(true);
        let r = evaluate(&f);
        assert_eq!(r.checks.iter().filter(|c| !c.ok).count(), 1);
        assert!(r.render().contains("관리자 권한으로 실행 중"), "{}", r.render());
        f.own_elevated = Some(true);
        assert!(evaluate(&f).healthy(), "both elevated: input reaches the game");
        f.game.as_mut().unwrap().elevated = None;
        assert!(!evaluate(&f).healthy(), "an unreadable token is a warning, not a pass");
    }

    #[test]
    fn dead_space_a_window_off_screen_and_a_missing_game_are_reported() {
        let mut f = healthy();
        f.monitors = vec![[0, 0, 1920, 1080], [2400, 0, 3840, 1080]];
        assert!(evaluate(&f).render().contains("빈 영역"));
        f.game.as_mut().unwrap().client = [1800, 100, 3080, 1060]; // spans the gap
        assert!(evaluate(&f).render().contains("모니터 밖"));
        f.game = None;
        let r = evaluate(&f);
        assert!(r.render().contains("찾지 못했습니다") && !r.healthy());
    }

    #[test]
    fn dpi_aspect_minimized_and_focus_are_checked() {
        let mut f = healthy();
        f.dpi_aware = false;
        f.game.as_mut().unwrap().minimized = true;
        f.game.as_mut().unwrap().client = [0, 0, 1000, 1000];
        f.foreground = 1;
        let bad = evaluate(&f).checks.iter().filter(|c| !c.ok).count();
        assert_eq!(bad, 4);
    }

    #[test]
    fn both_ratios_are_ok_and_the_ratio_is_reported() {
        let mut f = healthy();
        let r = evaluate(&f);
        assert!(r.render().contains("화면 비율 4:3"), "{}", r.render());
        f.game.as_mut().unwrap().client = [100, 100, 1380, 820]; // 1280x720
        let r = evaluate(&f);
        assert!(r.healthy(), "{}", r.render());
        assert!(r.render().contains("화면 비율 16:9"), "{}", r.render());
        f.game.as_mut().unwrap().client = [100, 100, 1380, 900]; // 16:10
        let r = evaluate(&f);
        assert!(!r.healthy() && r.render().contains("4:3도 16:9도 아닙니다"), "{}", r.render());
    }
}
