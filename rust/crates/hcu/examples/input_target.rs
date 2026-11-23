//! A small ordinary top-level window that records the mouse input it receives, so the native input
//! path can be tried against something that is not the game.
//!
//! `input_target [--title T] [--at X,Y] [--size W,H] [--log FILE] [--flat]`
//!
//! Default: title "HCU Test Target", client area 640x480 (4:3) at (100,100), topmost, four coloured
//! quadrants with a white square 100,100..140,140 in client pixels. Every left press/release and wheel
//! turn is printed and appended to `--log` as one line with the SCREEN position, which is what
//! `input_selftest` compares with where it aimed. `--flat` paints one grey (used as a covering window).
//!
//! It sends no input itself.

#[cfg(windows)]
mod app {
    use std::ffi::c_void;
    use std::io::Write;
    use std::sync::OnceLock;

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct Rect {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }
    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct Point {
        x: i32,
        y: i32,
    }
    #[repr(C)]
    struct Msg {
        hwnd: isize,
        message: u32,
        wparam: usize,
        lparam: isize,
        time: u32,
        pt: Point,
    }
    #[repr(C)]
    struct WndClassW {
        style: u32,
        wnd_proc: unsafe extern "system" fn(isize, u32, usize, isize) -> isize,
        cls_extra: i32,
        wnd_extra: i32,
        instance: isize,
        icon: isize,
        cursor: isize,
        background: isize,
        menu_name: *const u16,
        class_name: *const u16,
    }
    #[repr(C)]
    struct PaintStruct {
        hdc: isize,
        erase: i32,
        rc: Rect,
        restore: i32,
        inc_update: i32,
        reserved: [u8; 32],
    }

    #[link(name = "user32")]
    extern "system" {
        fn SetProcessDPIAware() -> i32;
        fn RegisterClassW(c: *const WndClassW) -> u16;
        fn CreateWindowExW(ex: u32, class: *const u16, title: *const u16, style: u32, x: i32, y: i32, w: i32, h: i32, parent: isize, menu: isize, inst: isize, param: *const c_void) -> isize;
        fn DefWindowProcW(hwnd: isize, msg: u32, w: usize, l: isize) -> isize;
        fn GetMessageW(m: *mut Msg, hwnd: isize, min: u32, max: u32) -> i32;
        fn TranslateMessage(m: *const Msg) -> i32;
        fn DispatchMessageW(m: *const Msg) -> isize;
        fn PostQuitMessage(code: i32);
        fn ShowWindow(hwnd: isize, cmd: i32) -> i32;
        fn BeginPaint(hwnd: isize, ps: *mut PaintStruct) -> isize;
        fn EndPaint(hwnd: isize, ps: *const PaintStruct) -> i32;
        fn FillRect(dc: isize, r: *const Rect, brush: isize) -> i32;
        fn GetClientRect(hwnd: isize, r: *mut Rect) -> i32;
        fn ClientToScreen(hwnd: isize, p: *mut Point) -> i32;
        fn AdjustWindowRect(r: *mut Rect, style: u32, menu: i32) -> i32;
        fn LoadCursorW(inst: isize, name: *const u16) -> isize;
    }
    #[link(name = "gdi32")]
    extern "system" {
        fn CreateSolidBrush(color: u32) -> isize;
        fn DeleteObject(o: isize) -> i32;
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GetModuleHandleW(name: *const u16) -> isize;
    }

    const WM_DESTROY: u32 = 0x0002;
    const WM_PAINT: u32 = 0x000F;
    const WM_LBUTTONDOWN: u32 = 0x0201;
    const WM_LBUTTONUP: u32 = 0x0202;
    const WM_MOUSEWHEEL: u32 = 0x020A;
    const WS_OVERLAPPEDWINDOW: u32 = 0x00CF_0000;
    const WS_VISIBLE: u32 = 0x1000_0000;
    const WS_EX_TOPMOST: u32 = 0x8;

    static LOG: OnceLock<Option<String>> = OnceLock::new();
    static FLAT: OnceLock<bool> = OnceLock::new();

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn log(line: &str) {
        println!("{line}");
        if let Some(Some(path)) = LOG.get() {
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
                let _ = writeln!(f, "{line}");
            }
        }
    }

    unsafe fn paint(hwnd: isize) {
        let mut ps: PaintStruct = std::mem::zeroed();
        let dc = BeginPaint(hwnd, &mut ps);
        let mut c = Rect::default();
        GetClientRect(hwnd, &mut c);
        let fill = |r: Rect, rgb: (u8, u8, u8)| {
            let brush = CreateSolidBrush(rgb.0 as u32 | (rgb.1 as u32) << 8 | (rgb.2 as u32) << 16);
            FillRect(dc, &r, brush);
            DeleteObject(brush);
        };
        if FLAT.get().copied().unwrap_or(false) {
            fill(c, (90, 90, 90));
        } else {
            let (mx, my) = ((c.right - c.left) / 2, (c.bottom - c.top) / 2);
            fill(Rect { left: 0, top: 0, right: mx, bottom: my }, (200, 40, 40));
            fill(Rect { left: mx, top: 0, right: c.right, bottom: my }, (40, 180, 40));
            fill(Rect { left: 0, top: my, right: mx, bottom: c.bottom }, (40, 60, 200));
            fill(Rect { left: mx, top: my, right: c.right, bottom: c.bottom }, (220, 200, 40));
            fill(Rect { left: 100, top: 100, right: 140, bottom: 140 }, (255, 255, 255));
        }
        EndPaint(hwnd, &ps);
    }

    unsafe extern "system" fn wnd_proc(hwnd: isize, msg: u32, w: usize, l: isize) -> isize {
        let xy = |l: isize| ((l & 0xFFFF) as u16 as i16 as i32, ((l >> 16) & 0xFFFF) as u16 as i16 as i32);
        match msg {
            WM_PAINT => {
                paint(hwnd);
                0
            }
            WM_LBUTTONDOWN | WM_LBUTTONUP => {
                let (cx, cy) = xy(l);
                let mut p = Point { x: cx, y: cy };
                ClientToScreen(hwnd, &mut p);
                log(&format!("{} x={} y={} client={cx},{cy}", if msg == WM_LBUTTONDOWN { "down" } else { "up" }, p.x, p.y));
                0
            }
            WM_MOUSEWHEEL => {
                let (sx, sy) = xy(l); // screen coordinates for the wheel
                log(&format!("wheel delta={} x={sx} y={sy}", (w >> 16) as u16 as i16));
                0
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                0
            }
            _ => DefWindowProcW(hwnd, msg, w, l),
        }
    }

    pub fn run() {
        let (mut title, mut at, mut size, mut logfile, mut flat) = ("HCU Test Target".to_string(), (100, 100), (640, 480), None, false);
        let mut args = std::env::args().skip(1);
        let pair = |s: Option<String>| -> Option<(i32, i32)> {
            let s = s?;
            let (a, b) = s.split_once(',')?;
            Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
        };
        while let Some(a) = args.next() {
            match a.as_str() {
                "--title" => title = args.next().unwrap_or(title),
                "--at" => at = pair(args.next()).unwrap_or(at),
                "--size" => size = pair(args.next()).unwrap_or(size),
                "--log" => logfile = args.next(),
                "--flat" => flat = true,
                other => eprintln!("ignored argument {other}"),
            }
        }
        let _ = LOG.set(logfile);
        let _ = FLAT.set(flat);
        unsafe {
            SetProcessDPIAware();
            let inst = GetModuleHandleW(std::ptr::null());
            let class = wide("HcuTestTargetClass");
            let wc = WndClassW { style: 0, wnd_proc, cls_extra: 0, wnd_extra: 0, instance: inst, icon: 0, cursor: LoadCursorW(0, 32512 as *const u16), background: 0, menu_name: std::ptr::null(), class_name: class.as_ptr() };
            RegisterClassW(&wc);
            let mut r = Rect { left: 0, top: 0, right: size.0, bottom: size.1 };
            AdjustWindowRect(&mut r, WS_OVERLAPPEDWINDOW, 0);
            let hwnd = CreateWindowExW(WS_EX_TOPMOST, class.as_ptr(), wide(&title).as_ptr(), WS_OVERLAPPEDWINDOW | WS_VISIBLE, at.0, at.1, r.right - r.left, r.bottom - r.top, 0, 0, inst, std::ptr::null());
            if hwnd == 0 {
                eprintln!("CreateWindowExW failed");
                return;
            }
            ShowWindow(hwnd, 5);
            println!("ready \"{title}\"");
            let mut m: Msg = std::mem::zeroed();
            while GetMessageW(&mut m, 0, 0, 0) > 0 {
                TranslateMessage(&m);
                DispatchMessageW(&m);
            }
        }
    }
}

fn main() {
    #[cfg(windows)]
    app::run();
    #[cfg(not(windows))]
    eprintln!("input_target is only built for Windows");
}
