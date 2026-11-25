//! A cheap look at whether the game's picture has changed: the client area shrunk by the GDI itself to 64x48 pixels.
//!
//! The add-on's loop used to take a full capture (two halves of the client area, each copied, converted and resized
//! with a good filter: about 110 ms) every 20 to 300 ms only to learn that nothing had moved. This reads the same
//! screen in a few milliseconds. It decides nothing about what is on screen: when it says the picture changed, the
//! full capture and the readers run as before.
use anyhow::{anyhow, bail, Result};

/// Width and height of a probe, and the bytes of one (RGB).
pub const PROBE_W: usize = 64;
pub const PROBE_H: usize = 48;
pub const PROBE_LEN: usize = PROBE_W * PROBE_H * 3;

/// One probe: 64x48 RGB.
pub type Probe = Vec<u8>;

/// Mean absolute difference of two probes, per value (0 to 255): how far apart two looks at the screen are.
pub fn probe_distance(a: &[u8], b: &[u8]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return f32::MAX;
    }
    a.iter().zip(b).map(|(x, y)| (*x as i32 - *y as i32).unsigned_abs() as u64).sum::<u64>() as f32 / a.len() as f32
}

/// How many of the 64x48 points differ by more than `by` (summed over the three channels, divided by three) between two
/// probes. A student placed in the squad changes a dozen points a lot and the mean of the picture hardly at all.
pub fn probe_changed_points(a: &[u8], b: &[u8], by: u32) -> usize {
    if a.len() != b.len() {
        return usize::MAX;
    }
    a.chunks_exact(3).zip(b.chunks_exact(3)).filter(|(x, y)| (0..3).map(|i| (x[i] as i32 - y[i] as i32).unsigned_abs()).sum::<u32>() / 3 > by).count()
}

#[cfg(windows)]
mod sys {
    #[repr(C)]
    pub struct BitmapInfoHeader {
        pub size: u32,
        pub width: i32,
        pub height: i32,
        pub planes: u16,
        pub bit_count: u16,
        pub compression: u32,
        pub size_image: u32,
        pub x_ppm: i32,
        pub y_ppm: i32,
        pub clr_used: u32,
        pub clr_important: u32,
    }
    #[link(name = "gdi32")]
    extern "system" {
        pub fn CreateCompatibleDC(dc: isize) -> isize;
        pub fn CreateCompatibleBitmap(dc: isize, w: i32, h: i32) -> isize;
        pub fn SelectObject(dc: isize, obj: isize) -> isize;
        pub fn StretchBlt(dst: isize, dx: i32, dy: i32, dw: i32, dh: i32, src: isize, sx: i32, sy: i32, sw: i32, sh: i32, rop: u32) -> i32;
        pub fn SetStretchBltMode(dc: isize, mode: i32) -> i32;
        pub fn GetDIBits(dc: isize, bmp: isize, start: u32, lines: u32, bits: *mut std::ffi::c_void, info: *mut BitmapInfoHeader, usage: u32) -> i32;
        pub fn DeleteObject(obj: isize) -> i32;
        pub fn DeleteDC(dc: isize) -> i32;
    }
    #[link(name = "user32")]
    extern "system" {
        pub fn GetDC(hwnd: isize) -> isize;
        pub fn ReleaseDC(hwnd: isize, dc: isize) -> i32;
    }
    pub const SRCCOPY_CAPTUREBLT: u32 = 0x00CC_0020 | 0x4000_0000;
}

/// Stretching modes of `SetStretchBltMode`: how source pixels are combined.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Every source pixel counts (slow for a large reduction).
    Halftone = 4,
    /// Pixels are dropped.
    ColorOnColor = 3,
    /// Pixels are ORed or ANDed together.
    BlackOnWhite = 1,
}

/// Holds the bitmap a probe is drawn into, so a probe does not allocate.
#[cfg(windows)]
pub struct Prober {
    dc: isize,
    bmp: isize,
    old: isize,
    mode: Mode,
}

#[cfg(windows)]
impl Prober {
    pub fn new(mode: Mode) -> Result<Prober> {
        unsafe {
            let screen = sys::GetDC(0);
            let dc = sys::CreateCompatibleDC(screen);
            let bmp = sys::CreateCompatibleBitmap(screen, PROBE_W as i32, PROBE_H as i32);
            sys::ReleaseDC(0, screen);
            if dc == 0 || bmp == 0 {
                bail!("could not create the probe bitmap");
            }
            let old = sys::SelectObject(dc, bmp);
            sys::SetStretchBltMode(dc, mode as i32);
            Ok(Prober { dc, bmp, old, mode })
        }
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// The screen rectangle (x, y, width, height in desktop pixels) as a probe.
    pub fn look(&self, x: i32, y: i32, w: i32, h: i32) -> Result<Probe> {
        if w <= 0 || h <= 0 {
            bail!("empty probe area");
        }
        unsafe {
            let screen = sys::GetDC(0);
            let ok = sys::StretchBlt(self.dc, 0, 0, PROBE_W as i32, PROBE_H as i32, screen, x, y, w, h, sys::SRCCOPY_CAPTUREBLT) != 0;
            sys::ReleaseDC(0, screen);
            if !ok {
                bail!("StretchBlt failed");
            }
            let mut bgra = vec![0u8; PROBE_W * PROBE_H * 4];
            let mut info = sys::BitmapInfoHeader { size: std::mem::size_of::<sys::BitmapInfoHeader>() as u32, width: PROBE_W as i32, height: -(PROBE_H as i32), planes: 1, bit_count: 32, compression: 0, size_image: 0, x_ppm: 0, y_ppm: 0, clr_used: 0, clr_important: 0 };
            // the bitmap must not be selected into a DC for GetDIBits on some drivers; the DC that holds it is the one asked
            let lines = sys::GetDIBits(self.dc, self.bmp, 0, PROBE_H as u32, bgra.as_mut_ptr() as *mut std::ffi::c_void, &mut info, 0);
            if lines as usize != PROBE_H {
                return Err(anyhow!("GetDIBits gave {lines} of {PROBE_H} lines"));
            }
            let mut rgb = Vec::with_capacity(PROBE_LEN);
            for p in bgra.chunks_exact(4) {
                rgb.extend_from_slice(&[p[2], p[1], p[0]]);
            }
            Ok(rgb)
        }
    }
}

#[cfg(windows)]
impl Drop for Prober {
    fn drop(&mut self) {
        unsafe {
            sys::SelectObject(self.dc, self.old);
            sys::DeleteObject(self.bmp);
            sys::DeleteDC(self.dc);
        }
    }
}

/// A probe of the game's client area, or an error when the game is not there.
#[cfg(windows)]
pub fn look_at_game(prober: &Prober) -> Result<Probe> {
    let (client, _) = crate::window::game_client_and_desktop(crate::GAME_TITLE)?;
    prober.look(client.0, client.1, client.2, client.3)
}

#[cfg(not(windows))]
pub struct Prober;

#[cfg(not(windows))]
impl Prober {
    pub fn new(_mode: Mode) -> Result<Prober> {
        bail!("the probe is only implemented for Windows")
    }
}

#[cfg(not(windows))]
pub fn look_at_game(_prober: &Prober) -> Result<Probe> {
    bail!("the probe is only implemented for Windows")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_distance_of_equal_probes_is_zero_and_grows_with_what_changed() {
        let a = vec![10u8; PROBE_LEN];
        let mut b = a.clone();
        assert_eq!(probe_distance(&a, &b), 0.0);
        for v in b.iter_mut().take(PROBE_LEN / 10) {
            *v = 110;
        }
        assert!((probe_distance(&a, &b) - 10.0).abs() < 0.1, "a tenth of the values 100 higher is 10 on average");
        assert_eq!(probe_distance(&a, &a[..5]), f32::MAX, "probes of another size are never close");
    }
}
