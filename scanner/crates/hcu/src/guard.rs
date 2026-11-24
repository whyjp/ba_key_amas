//! Facts and checks the input boundary is built from. Pure: the desktop supplies the facts
//! ([`WinInfo`]), nothing here talks to Windows.
use serde_json::json;

/// A visible top-level window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WinInfo {
    pub hwnd: isize,
    /// left, top, right, bottom in virtual-desktop pixels.
    pub rect: [i32; 4],
    pub class: String,
    pub title: String,
    /// Belongs to this process (the add-on's own card and overlays).
    pub own: bool,
}

impl WinInfo {
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.rect[0] && x < self.rect[2] && y >= self.rect[1] && y < self.rect[3]
    }

    fn intersects(&self, area: [i32; 4]) -> bool {
        self.rect[0] < area[2] && area[0] < self.rect[2] && self.rect[1] < area[3] && area[1] < self.rect[3]
    }
}

/// The windows over `area` (left, top, right, bottom), front to back, as the facts a frame's
/// coordinates depend on: which windows, where, in which order. Titles are left out: they change
/// without changing what a click would hit. The add-on's own windows are left out too: they are
/// click-through, and they redraw all the time, which would make every frame stale.
pub fn windows_over(layout: &[WinInfo], area: [i32; 4]) -> Vec<(isize, [i32; 4])> {
    layout.iter().filter(|w| !w.own && w.intersects(area)).map(|w| (w.hwnd, w.rect)).collect()
}

/// Whether the windows over `area` are the same in both layouts (port of `RegionStillValid`).
pub fn same_over(a: &[WinInfo], b: &[WinInfo], area: [i32; 4]) -> bool {
    windows_over(a, area) == windows_over(b, area)
}

/// Why input was not sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// The frame the coordinates came from no longer describes the desktop.
    Stale(String),
    /// The point is in dead space of the virtual desktop (no monitor shows it).
    OffMonitor,
    /// The window under the point is not the one input is meant for.
    WrongWindow(String),
}

impl Refusal {
    pub fn code(&self) -> &'static str {
        match self {
            Refusal::Stale(_) => "gui_frame_stale",
            Refusal::OffMonitor => "target_off_monitor",
            Refusal::WrongWindow(_) => "target_window_mismatch",
        }
    }

    pub fn hint(&self) -> String {
        match self {
            Refusal::Stale(why) => format!("{why}; take a new screenshot"),
            Refusal::OffMonitor => "the point lies on no monitor".into(),
            Refusal::WrongWindow(why) => why.clone(),
        }
    }
}

/// Failure of one send to the desktop, saying whether anything already reached the system
/// (a pointer move counts: it is input the player can see).
#[derive(Debug)]
pub struct SendError {
    pub emitted: bool,
    pub error: anyhow::Error,
}

impl SendError {
    pub fn not_sent(e: impl Into<anyhow::Error>) -> Self {
        Self { emitted: false, error: e.into() }
    }
    pub fn sent(e: impl Into<anyhow::Error>) -> Self {
        Self { emitted: true, error: e.into() }
    }
}

/// The answer for input that did not (completely) happen, in the CLI's failure record shape.
pub fn failure_record(code: &str, hint: &str, input_emitted: bool, completed: usize) -> String {
    json!({"ok": false, "code": code, "hint": hint, "input_emitted": input_emitted, "completed": completed}).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    pub fn win(hwnd: isize, rect: [i32; 4]) -> WinInfo {
        WinInfo { hwnd, rect, class: "C".into(), title: format!("t{hwnd}"), own: false }
    }

    #[test]
    fn only_windows_touching_the_area_count_and_their_order_matters() {
        let a = [win(1, [0, 0, 100, 100]), win(2, [50, 50, 200, 200]), win(3, [500, 500, 600, 600])];
        let area = [60, 60, 70, 70];
        assert_eq!(windows_over(&a, area).iter().map(|w| w.0).collect::<Vec<_>>(), vec![1, 2]);
        let swapped = [a[1].clone(), a[0].clone(), a[2].clone()];
        assert!(!same_over(&a, &swapped, area), "stacking order changed over the area");
        let moved_elsewhere = [a[0].clone(), a[1].clone(), win(3, [700, 700, 800, 800])];
        assert!(same_over(&a, &moved_elsewhere, area), "a window that never touched the area is irrelevant");
        let moved_over = [a[0].clone(), a[1].clone(), win(3, [65, 65, 80, 80])];
        assert!(!same_over(&a, &moved_over, area));
    }

    #[test]
    fn own_windows_and_titles_do_not_make_a_frame_stale() {
        let a = vec![win(1, [0, 0, 100, 100])];
        let mut b = a.clone();
        b[0].title = "new title".into();
        b.insert(0, WinInfo { own: true, ..win(9, [0, 0, 100, 100]) });
        assert!(same_over(&a, &b, [10, 10, 11, 11]));
    }
}
