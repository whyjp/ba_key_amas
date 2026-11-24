//! out/index.json persistence and report rebuild (pipeline.go).
use anyhow::Result;
use baocr_core::model::{IndexEntry, RosterEntry, RosterReport, Student};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub const STATUS_CAPTURED: &str = "captured";
pub const STATUS_EXTRACTED: &str = "extracted";
pub const STATUS_FAILED: &str = "failed";

#[derive(Serialize, Deserialize)]
struct IndexFile {
    #[serde(default)]
    version: i32,
    #[serde(default)]
    entries: Vec<IndexEntry>,
}

/// The on-disk work list, safe for concurrent workers.
pub struct Index {
    path: PathBuf,
    m: Mutex<BTreeMap<String, IndexEntry>>,
}

impl Index {
    pub fn new(out: &Path) -> Index {
        Index { path: out.join("index.json"), m: Mutex::new(BTreeMap::new()) }
    }

    /// Errors when index.json is missing or malformed.
    pub fn load(out: &Path) -> Result<Index> {
        let ix = Index::new(out);
        let f: IndexFile = serde_json::from_slice(&std::fs::read(&ix.path)?)?;
        {
            let mut m = ix.m.lock().unwrap_or_else(|e| e.into_inner());
            for e in f.entries {
                m.insert(e.id.clone(), e);
            }
        }
        Ok(ix)
    }

    /// Applies `f` to the entry (creating it) and persists the index.
    pub fn update(&self, id: &str, f: impl FnOnce(&mut IndexEntry)) {
        let mut m = self.m.lock().unwrap_or_else(|e| e.into_inner());
        let e = m.entry(id.to_string()).or_insert_with(|| IndexEntry { id: id.to_string(), ..Default::default() });
        f(e);
        let _ = Self::write(&self.path, &m);
    }

    /// Entries ordered by id.
    pub fn sorted(&self) -> Vec<IndexEntry> {
        self.m.lock().unwrap_or_else(|e| e.into_inner()).values().cloned().collect()
    }

    pub fn save(&self) -> Result<()> {
        Self::write(&self.path, &self.m.lock().unwrap_or_else(|e| e.into_inner()))
    }

    fn write(path: &Path, m: &BTreeMap<String, IndexEntry>) -> Result<()> {
        let f = IndexFile { version: 1, entries: m.values().cloned().collect() };
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(&f)?)?;
        Ok(())
    }
}

/// Reads `<dir>/student.json`.
pub fn load_student(dir: &Path) -> Result<Student> {
    Ok(serde_json::from_slice(&std::fs::read(dir.join("student.json"))?)?)
}

/// Assembles the roster report from the index and the per-student files.
pub fn rebuild_report(out: &Path, ix: &Index, mut base: RosterReport) -> RosterReport {
    base.entries = Vec::new();
    base.unresolved = Vec::new();
    for (n, e) in ix.sorted().into_iter().enumerate() {
        let mut entry = RosterEntry {
            id: e.id.clone(),
            column: n as i32,
            detail_path: format!("details/{}", e.id),
            status: "failed".into(),
            ..Default::default()
        };
        match load_student(&out.join("details").join(&e.id)) {
            Ok(s) => {
                entry.status = if s.name.is_empty() { "unresolved_name" } else { "detail_ocr" }.into();
                if s.name.is_empty() {
                    base.unresolved.push(e.id.clone());
                }
                entry.student = s;
            }
            Err(_) => base.unresolved.push(e.id.clone()),
        }
        if e.status == STATUS_FAILED {
            entry.status = "failed".into();
        }
        base.entries.push(entry);
    }
    base
}
