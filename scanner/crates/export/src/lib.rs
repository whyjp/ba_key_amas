//! baocr-export: JSON export, the roster report, the change store and the extraction index.
//! The output is JSON only: a student's `student.json`, the roster report `students.json`,
//! and `index.json`. (CSV and HTML were dropped: the add-on shows what it needs itself.)
mod index;
mod store;

pub use index::{load_student, rebuild_report, Index, STATUS_CAPTURED, STATUS_EXTRACTED, STATUS_FAILED};
pub use store::{diff_variable, student_key, variable_view, Change, HistoryEntry, Store};

use anyhow::Result;
use baocr_core::model::{RosterReport, Student};
use std::path::Path;

/// "고정" / "미고정" / "미확인" for the pin state (the scan's progress line).
pub fn pin_cell(v: Option<bool>) -> &'static str {
    match v {
        None => "미확인",
        Some(true) => "고정",
        Some(false) => "미고정",
    }
}

pub fn string_cell(s: &Option<String>) -> String {
    s.clone().unwrap_or_default()
}

pub fn bool_cell(v: Option<bool>) -> String {
    v.map(|b| b.to_string()).unwrap_or_default()
}

/// Writes `student.json` (frames are not written here).
pub fn export_student(s: &Student, out: &Path) -> Result<()> {
    std::fs::create_dir_all(out)?;
    std::fs::write(out.join("student.json"), serde_json::to_string_pretty(s)?)?;
    Ok(())
}

/// Writes `students.json`.
pub fn save_report(report: &RosterReport, out: &Path) -> Result<()> {
    std::fs::create_dir_all(out)?;
    std::fs::write(out.join("students.json"), serde_json::to_string_pretty(report)?)?;
    Ok(())
}

#[cfg(test)]
mod tests;
