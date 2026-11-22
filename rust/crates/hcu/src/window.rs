//! Finds the game's client rectangle by itself, in HCU frame coordinates.
//!
//! HCU's screenshot is the whole virtual desktop scaled into one image. The game
//! window's client rectangle (Win32, physical pixels) is mapped into that image,
//! so no `--region` has to be typed and moving the window does not break reading.
use anyhow::{anyhow, bail, Result};

/// Pure mapping: a client rectangle in screen pixels -> HCU frame coordinates.
///
/// `virt` is the virtual desktop (x, y, w, h) and `frame` the HCU screenshot size.
pub fn to_frame_region(client: (i32, i32, i32, i32), virt: (i32, i32, i32, i32), frame: (u32, u32)) -> [f64; 4] {
    let sx = frame.0 as f64 / virt.2 as f64;
    let sy = frame.1 as f64 / virt.3 as f64;
    let (cx, cy, cw, ch) = client;
    [
        (cx - virt.0) as f64 * sx,
        (cy - virt.1) as f64 * sy,
        (cx - virt.0 + cw) as f64 * sx,
        (cy - virt.1 + ch) as f64 * sy,
    ]
}

/// (x, y, width, height) of the game's client area and of the virtual desktop, in physical pixels.
#[cfg(windows)]
pub fn game_client_and_desktop(title: &str) -> Result<((i32, i32, i32, i32), (i32, i32, i32, i32))> {
    #[repr(C)]
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
        fn FindWindowW(class: *const u16, title: *const u16) -> isize;
        fn GetClientRect(hwnd: isize, rect: *mut Rect) -> i32;
        fn ClientToScreen(hwnd: isize, p: *mut Point) -> i32;
        fn GetSystemMetrics(index: i32) -> i32;
        fn IsIconic(hwnd: isize) -> i32;
    }
    let wide: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        SetProcessDPIAware(); // physical pixels, matching what HCU captures
        let hwnd = FindWindowW(std::ptr::null(), wide.as_ptr());
        if hwnd == 0 {
            bail!("no window titled exactly \"{title}\" (is the game running?)");
        }
        if IsIconic(hwnd) != 0 {
            bail!("the game window is minimised");
        }
        let mut r = Rect { left: 0, top: 0, right: 0, bottom: 0 };
        if GetClientRect(hwnd, &mut r) == 0 {
            bail!("GetClientRect failed");
        }
        let mut p = Point { x: 0, y: 0 };
        if ClientToScreen(hwnd, &mut p) == 0 {
            bail!("ClientToScreen failed");
        }
        let virt = (GetSystemMetrics(76), GetSystemMetrics(77), GetSystemMetrics(78), GetSystemMetrics(79));
        if virt.2 <= 0 || virt.3 <= 0 {
            bail!("could not read the virtual desktop size");
        }
        Ok(((p.x, p.y, r.right - r.left, r.bottom - r.top), virt))
    }
}

#[cfg(not(windows))]
pub fn game_client_and_desktop(_title: &str) -> Result<((i32, i32, i32, i32), (i32, i32, i32, i32))> {
    Err(anyhow!("automatic window detection is only implemented for Windows; pass --region"))
}

/// The game's client rectangle in HCU frame coordinates, found automatically.
pub fn auto_region(frame: (u32, u32)) -> Result<[f64; 4]> {
    let (client, virt) = game_client_and_desktop(crate::GAME_TITLE)?;
    if client.2 <= 0 || client.3 <= 0 {
        bail!("the game window has an empty client area");
    }
    Ok(to_frame_region(client, virt, frame))
}

/// Detect the region with a passive HCU screenshot (no input) for the frame size.
pub fn detect_region(h: &crate::Hcu) -> Result<[f64; 4]> {
    let rec = h.screenshot()?;
    auto_region((rec.screen.width as u32, rec.screen.height as u32))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_client_rect_into_the_scaled_desktop() {
        // a 2560x1536 virtual desktop shown as 1280x768 (scale 0.5), client at (320,130) 910x682
        let r = to_frame_region((320, 130, 910, 682), (0, 0, 2560, 1536), (1280, 768));
        assert_eq!(r, [160.0, 65.0, 615.0, 406.0]);
    }

    #[test]
    fn handles_a_virtual_desktop_with_negative_origin() {
        // a monitor to the left of the primary: the virtual desktop starts at x=-1920
        let r = to_frame_region((-1600, 100, 800, 600), (-1920, 0, 3840, 1080), (1920, 540));
        assert_eq!(r, [160.0, 50.0, 560.0, 350.0]);
    }
}
