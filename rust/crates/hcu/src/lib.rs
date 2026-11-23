//! Game-control side of the scanner: drives Microsoft HCU (`computer-use.exe`),
//! captures the 4:3 game client as a 1280x960 frame, walks the profile arrows
//! and menus, and watches passively. Ports `main.go` (HCU, gameCapture),
//! `roster_scan.go` (scroll/toFrame), `deepcapture.go` (captureDeep, click half),
//! `deep.go` (settle), `arrow_scan.go` (scanArrows), `capture.go`, `watch.go`.
//!
//! No OCR lives here: name reads and extraction go through `ScanSink` /
//! closures supplied by the caller. Everything above the process boundary
//! talks to HCU through the [`Runner`] trait, so it is testable with a fake.
mod audit;
mod capture;
mod deep;
mod guard;
mod hcu;
mod native;
mod roster;
mod scan;
mod watch;
mod window;

#[cfg(test)]
mod fake;

pub use capture::{check_aspect, game_capture, layout_looks_calibrated, new_capture, parse_frame_region, to_frame};
pub use deep::{capture_deep, click_game, settle, settle_quiet};
pub use audit::{AuditSink, FileSink};
pub use guard::{Refusal, SendError, WinInfo};
pub use native::{delivered_size, to_absolute, to_native, zoom_geometry, Desktop, NativeRunner, Settle};
#[cfg(windows)]
pub use native::WinDesktop;
pub use hcu::{find_bin, window_active, window_ref, Exclusive, Hcu, HcuSession, ProcessRunner, Record, Runner, GAME_TITLE};
pub use roster::{count_roster, ensure_profile, go_to_roster, merge_new_cards, open_first_student, prepare_scan, screen_kind, scroll_to_top, Screen};
pub use scan::{scan_arrows, wait_stable_next, ScanOutcome};
pub use window::{auto_region, detect_region, to_frame_region};
pub use watch::{FrameSource, HcuSource, Update, Watcher, STILL_FRAMES};
