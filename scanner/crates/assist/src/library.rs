//! What the add-on has stored about the player's students: when the list was last updated and
//! how many students it holds. The list is `students.json`, written by the roster scan; the add-on
//! only shows how stale it is, and the player decides when to scan again.
//!
//! A calendar date is hard to read as "stale or not" at a glance, so the card shows the time that
//! has passed since the scan ("3일 2시간 경과"); the exact date and time are in a tooltip.
use std::path::Path;
use std::time::{Duration, SystemTime};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LibraryInfo {
    /// When the list was last written; None when nothing has been scanned.
    pub updated: Option<SystemTime>,
    pub count: usize,
}

const MINUTE: u64 = 60;
const HOUR: u64 = 60 * MINUTE;
const DAY: u64 = 24 * HOUR;

/// "3일 2시간 경과", "5시간 12분 경과", "7분 경과", "방금 전".
fn elapsed_text(age: Duration) -> String {
    let s = age.as_secs();
    if s >= DAY {
        format!("{}일 {}시간 경과", s / DAY, s % DAY / HOUR)
    } else if s >= HOUR {
        format!("{}시간 {}분 경과", s / HOUR, s % HOUR / MINUTE)
    } else if s >= MINUTE {
        format!("{}분 경과", s / MINUTE)
    } else {
        "방금 전".to_string()
    }
}

fn local_text(t: SystemTime) -> String {
    chrono::DateTime::<chrono::Local>::from(t).format("%Y-%m-%d %H:%M").to_string()
}

impl LibraryInfo {
    /// How long ago the list was updated, for the card.
    pub fn age_text(&self, now: SystemTime) -> String {
        match self.updated {
            Some(t) => format!("{} · {}명", elapsed_text(now.duration_since(t).unwrap_or_default()), self.count),
            None => "스캔 기록 없음".to_string(),
        }
    }

    /// The exact date and time, for the tooltip.
    pub fn exact_text(&self) -> String {
        match self.updated {
            Some(t) => format!("마지막 스캔\n{}", local_text(t)),
            None => "아직 스캔한 적이 없습니다\n메뉴에서 학생 정보 스캔을 시작하세요".to_string(),
        }
    }
}

/// Read the state of the student list in `dir` (the scan's output folder).
pub fn read_library(dir: &Path) -> LibraryInfo {
    let file = dir.join("students.json");
    let updated = std::fs::metadata(&file).and_then(|m| m.modified()).ok();
    let count = std::fs::read(&file)
        .ok()
        .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
        .and_then(|v| v.get("entries").and_then(|e| e.as_array()).map(|a| a.len()))
        .unwrap_or(0);
    LibraryInfo { updated, count }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ago(secs: u64) -> (LibraryInfo, SystemTime) {
        let now = SystemTime::now();
        (LibraryInfo { updated: Some(now - Duration::from_secs(secs)), count: 106 }, now)
    }

    #[test]
    fn staleness_is_told_in_days_and_hours_then_hours_minutes_then_minutes() {
        let day = 24 * 3600;
        for (secs, want) in [(2 * day + 3 * 3600 + 59, "2일 3시간 경과 · 106명"), (day, "1일 0시간 경과 · 106명"), (5 * 3600 + 12 * 60, "5시간 12분 경과 · 106명"), (7 * 60 + 30, "7분 경과 · 106명"), (20, "방금 전 · 106명")] {
            let (info, now) = ago(secs);
            assert_eq!(info.age_text(now), want, "{secs} s");
        }
        assert_eq!(LibraryInfo::default().age_text(SystemTime::now()), "스캔 기록 없음");
        // a clock that went backwards does not panic or print nonsense
        let (info, now) = ago(0);
        assert_eq!(info.age_text(now - Duration::from_secs(5)), "방금 전 · 106명");
    }

    #[test]
    fn the_tooltip_has_the_exact_local_date_and_time() {
        let (info, _) = ago(3600);
        let tip = info.exact_text();
        let mut lines = tip.lines();
        assert_eq!(lines.next(), Some("마지막 스캔"));
        let stamp = lines.next().expect("a time line");
        assert!(stamp.len() == 16 && stamp.chars().nth(4) == Some('-') && stamp.chars().nth(13) == Some(':'), "{stamp}");
        assert!(LibraryInfo::default().exact_text().contains("아직 스캔한 적이 없습니다"));
    }

    #[test]
    fn the_stored_list_is_read_from_the_scan_output_folder() {
        let dir = std::env::temp_dir().join(format!("baocr-library-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(read_library(&dir), LibraryInfo::default());
        std::fs::write(dir.join("students.json"), r#"{"entries":[{"id":"001"},{"id":"002"},{"id":"003"}]}"#).unwrap();
        let got = read_library(&dir);
        assert_eq!(got.count, 3);
        assert!(got.updated.is_some_and(|t| t.elapsed().unwrap_or_default() < Duration::from_secs(60)));
        // a damaged file still has a time but no count
        std::fs::write(dir.join("students.json"), "not json").unwrap();
        assert_eq!(read_library(&dir).count, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
