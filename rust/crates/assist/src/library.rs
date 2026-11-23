//! What the add-on has stored about the player's students: when the list was last updated and
//! how many students it holds. The list is `students.json`, written by the roster scan; the add-on
//! only shows when it was refreshed, and the player decides when to scan again.
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LibraryInfo {
    /// Local time of the last update, "2026-11-23 18:02"; None when nothing has been scanned.
    pub updated: Option<String>,
    pub count: usize,
}

impl LibraryInfo {
    /// One line for the menu.
    pub fn describe(&self) -> String {
        match &self.updated {
            Some(t) => format!("{t} 갱신 · {}명", self.count),
            None => "스캔 기록 없음".to_string(),
        }
    }
}

/// Read the state of the student list in `dir` (the scan's output folder).
pub fn read_library(dir: &Path) -> LibraryInfo {
    let file = dir.join("students.json");
    let updated = std::fs::metadata(&file).and_then(|m| m.modified()).ok().map(|t| chrono::DateTime::<chrono::Local>::from(t).format("%Y-%m-%d %H:%M").to_string());
    let count = std::fs::read(&file)
        .ok()
        .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
        .and_then(|v| v.get("entries").and_then(|e| e.as_array()).map(|a| a.len()))
        .unwrap_or(0);
    LibraryInfo { updated: updated.filter(|_| file.exists()), count }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_scan_yet_and_a_scanned_list_are_told_apart() {
        let dir = std::env::temp_dir().join(format!("baocr-library-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let none = read_library(&dir);
        assert_eq!((none.updated.clone(), none.count), (None, 0));
        assert_eq!(none.describe(), "스캔 기록 없음");
        std::fs::write(dir.join("students.json"), r#"{"entries":[{"id":"001"},{"id":"002"},{"id":"003"}]}"#).unwrap();
        let got = read_library(&dir);
        assert_eq!(got.count, 3);
        let stamp = got.updated.clone().expect("a time");
        assert!(stamp.len() == 16 && stamp.chars().nth(4) == Some('-') && stamp.chars().nth(13) == Some(':'), "{stamp}");
        assert!(got.describe().ends_with("갱신 · 3명"), "{}", got.describe());
        // a damaged file still has a time but no count
        std::fs::write(dir.join("students.json"), "not json").unwrap();
        assert_eq!(read_library(&dir).count, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
