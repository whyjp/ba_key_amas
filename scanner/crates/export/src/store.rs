//! Latest record of every student plus an append-only change history (store.go).
use super::{bool_cell, string_cell};
use anyhow::{bail, Result};
use baocr_core::model::Student;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

/// One field that differs from the stored record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Change {
    pub field: String,
    pub from: String,
    pub to: String,
}

/// One line of history.jsonl.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub time: DateTime<Utc>,
    pub key: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub new: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub changes: Vec<Change>,
}

/// Dictionary id when unambiguous, otherwise "n" + 12 hex chars of sha1(name).
pub fn student_key(s: &Student) -> String {
    if let Some(f) = &s.fixed_meta {
        if f.dictionary_id > 0 && !f.ambiguous {
            return f.dictionary_id.to_string();
        }
    }
    let digest = Sha1::digest(s.name.as_bytes());
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    format!("n{}", &hex[..12])
}

fn ints(p: Option<i32>) -> String {
    p.map(|v| v.to_string()).unwrap_or_default()
}

/// Only the variable-stage fields (the ones a background updater keeps fresh).
pub fn variable_view(s: &Student) -> BTreeMap<String, String> {
    let mut v = BTreeMap::new();
    let mut put = |k: &str, x: String| {
        v.insert(k.to_string(), x);
    };
    put("level", ints(s.level));
    put("rarity_stars", ints(s.rarity_stars));
    put("ex_skill_level", ints(s.ex_skill_level));
    put("normal_skill_level", ints(s.normal_skill_level));
    put("passive_skill_level", ints(s.passive_skill_level));
    put("sub_skill_level", ints(s.sub_skill_level));
    put("bond_rank", ints(s.bond_rank));
    put("unique_weapon_stars", ints(s.unique_weapon_stars));
    put("unique_weapon_level", ints(s.unique_weapon_level));
    put("is_pinned", bool_cell(s.is_pinned));
    put("max_hp", ints(s.stats.max_hp));
    put("attack", ints(s.stats.attack));
    put("defense", ints(s.stats.defense));
    put("healing", ints(s.stats.healing));
    put("exp_current", ints(s.exp.current));
    put("exp_required", ints(s.exp.required));
    for (i, eq) in s.equipment.iter().enumerate() {
        let p = format!("equipment_{}_", i + 1);
        put(&format!("{p}equipped"), bool_cell(eq.equipped));
        put(&format!("{p}item_name"), string_cell(&eq.item_name));
        put(&format!("{p}tier"), ints(eq.tier));
        put(&format!("{p}level"), ints(eq.level));
    }
    v
}

/// Lists the variable fields that changed. A field the new read could not
/// determine (empty) never overwrites a known value.
pub fn diff_variable(old: &Student, cur: &Student) -> (Vec<Change>, Student) {
    let mut merged = cur.clone();
    let (a, b) = (variable_view(old), variable_view(cur));
    let mut changes = Vec::new();
    for (k, new) in &b {
        let was = a.get(k).cloned().unwrap_or_default();
        if new.is_empty() || &was == new {
            continue;
        }
        changes.push(Change { field: k.clone(), from: was, to: new.clone() });
    }
    fn keep(dst: &mut Option<i32>, old: Option<i32>) {
        if dst.is_none() {
            *dst = old;
        }
    }
    keep(&mut merged.level, old.level);
    keep(&mut merged.rarity_stars, old.rarity_stars);
    keep(&mut merged.ex_skill_level, old.ex_skill_level);
    keep(&mut merged.normal_skill_level, old.normal_skill_level);
    keep(&mut merged.passive_skill_level, old.passive_skill_level);
    keep(&mut merged.sub_skill_level, old.sub_skill_level);
    keep(&mut merged.bond_rank, old.bond_rank);
    keep(&mut merged.unique_weapon_stars, old.unique_weapon_stars);
    keep(&mut merged.unique_weapon_level, old.unique_weapon_level);
    keep(&mut merged.stats.max_hp, old.stats.max_hp);
    keep(&mut merged.stats.attack, old.stats.attack);
    keep(&mut merged.stats.defense, old.stats.defense);
    keep(&mut merged.stats.healing, old.stats.healing);
    (changes, merged)
}

/// `<dir>/students/<key>.json` (latest) and `<dir>/history.jsonl`.
pub struct Store {
    dir: PathBuf,
    lock: Mutex<()>,
}

impl Store {
    pub fn open(dir: impl Into<PathBuf>) -> Result<Store> {
        let dir = dir.into();
        std::fs::create_dir_all(dir.join("students"))?;
        Ok(Store { dir, lock: Mutex::new(()) })
    }

    pub fn dir(&self) -> &std::path::Path {
        &self.dir
    }

    /// Stores `s` and reports what changed against the previous record: (changes, is_new).
    pub fn upsert(&self, s: &Student, at: DateTime<Utc>) -> Result<(Vec<Change>, bool)> {
        if s.name.is_empty() {
            bail!("refusing to store a student without a name");
        }
        let _g = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let key = student_key(s);
        let path = self.dir.join("students").join(format!("{key}.json"));
        let (changes, merged, is_new) = match std::fs::read(&path) {
            Ok(b) => {
                let old: Student = serde_json::from_slice(&b)?;
                let (changes, merged) = diff_variable(&old, s);
                if changes.is_empty() {
                    return Ok((Vec::new(), false));
                }
                (changes, merged, false)
            }
            Err(_) => (Vec::new(), s.clone(), true),
        };
        std::fs::write(&path, serde_json::to_string_pretty(&merged)?)?;
        let mut line = serde_json::to_string(&HistoryEntry {
            time: at,
            key,
            name: s.name.clone(),
            new: is_new,
            changes: changes.clone(),
        })?;
        line.push('\n');
        let mut f = std::fs::OpenOptions::new().append(true).create(true).open(self.dir.join("history.jsonl"))?;
        f.write_all(line.as_bytes())?;
        Ok((changes, is_new))
    }

    /// Every stored student, sorted by name.
    pub fn all(&self) -> Result<Vec<Student>> {
        let mut out = Vec::new();
        for e in std::fs::read_dir(self.dir.join("students"))? {
            let p = e?.path();
            if p.extension().and_then(|x| x.to_str()) == Some("json") {
                out.push(serde_json::from_slice::<Student>(&std::fs::read(&p)?)?);
            }
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(out)
    }
}
