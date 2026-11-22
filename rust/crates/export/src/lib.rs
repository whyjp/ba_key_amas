//! baocr-export: CSV / HTML / JSON export, the roster report, the change store
//! and the extraction index. Ports cmd/baocr main.go (export), meta.go
//! (metaColumns), roster_scan.go (saveReport), store.go and pipeline.go (Index).
//!
//! All OCR text is HTML-escaped (Go html/template rules); the viewer renders OCR
//! as text, never as markup.
mod html;
mod index;
mod store;

pub use index::{load_student, rebuild_report, Index, STATUS_CAPTURED, STATUS_EXTRACTED, STATUS_FAILED};
pub use store::{diff_variable, student_key, variable_view, Change, HistoryEntry, Store};

use anyhow::Result;
use baocr_core::model::{Equipment, FixedMeta, RosterReport, Student};
use std::path::Path;

/// One exported value. CSV header, HTML label and roster table share this list.
#[derive(Debug, Clone, PartialEq)]
pub struct MetaCol {
    pub key: &'static str,
    pub label: &'static str,
    pub value: String,
}

pub fn cell(n: Option<i32>) -> String {
    n.map(|v| v.to_string()).unwrap_or_default()
}

pub fn string_cell(s: &Option<String>) -> String {
    s.clone().unwrap_or_default()
}

pub fn equipped_cell(v: Option<bool>) -> &'static str {
    match v {
        None => "미확인",
        Some(true) => "착용",
        Some(false) => "미착용",
    }
}

pub fn pin_cell(v: Option<bool>) -> &'static str {
    match v {
        None => "미확인",
        Some(true) => "고정",
        Some(false) => "미고정",
    }
}

/// Filter token for the report's pin dropdown.
pub fn pin_key(v: Option<bool>) -> &'static str {
    match v {
        None => "unknown",
        Some(true) => "true",
        Some(false) => "false",
    }
}

pub fn bool_cell(v: Option<bool>) -> String {
    v.map(|b| b.to_string()).unwrap_or_default()
}

pub fn fixed_source(s: &Student) -> String {
    s.fixed_meta.as_ref().map(|f| f.source.clone()).unwrap_or_default()
}

pub fn fixed_str(s: &Student, f: impl Fn(&FixedMeta) -> String) -> String {
    s.fixed_meta.as_ref().map(f).unwrap_or_default()
}

/// Equipment at slot index `i` (0-based); an empty slot with its number if absent.
pub fn equipment_at(s: &Student, i: usize) -> Equipment {
    s.equipment.get(i).cloned().unwrap_or(Equipment { slot: i as i32 + 1, ..Default::default() })
}

pub fn meta_columns(s: &Student) -> Vec<MetaCol> {
    let nm = s.name_match.as_ref().map(|n| n.status.clone()).unwrap_or_default();
    let ref_default = Default::default();
    let r = s.reference.as_ref().unwrap_or(&ref_default);
    let id = &s.identity;
    let ref_id = if s.reference.is_some() { r.id.to_string() } else { String::new() };
    let c = |key, label, value: String| MetaCol { key, label, value };
    vec![
        c("base_name", "원본 이름", id.base_name.clone()),
        c("outfit", "복장", id.outfit.clone()),
        c("outfit_key", "복장 키", id.outfit_key.clone()),
        c("is_distinct_character", "별도 캐릭터(*)", id.is_distinct_character.to_string()),
        c("name_status", "이름 판독", nm),
        c("max_hp", "최대체력", cell(s.stats.max_hp)),
        c("attack", "공격력", cell(s.stats.attack)),
        c("defense", "방어력", cell(s.stats.defense)),
        c("healing", "치유력", cell(s.stats.healing)),
        c("exp_current", "경험치", cell(s.exp.current)),
        c("exp_required", "필요 경험치", cell(s.exp.required)),
        c("tactic_role", "포지션", s.loadout.tactic_role.clone()),
        c("position", "배치", s.loadout.position.clone()),
        c("attack_type", "공격 타입", s.loadout.attack_type.clone()),
        c("armor_type", "방어 타입", s.loadout.armor_type.clone()),
        c("weapon_class", "무기 종류", s.loadout.weapon_class.clone()),
        c("terrain_urban", "도시 적응(4=S..0=D)", cell(s.terrain.urban)),
        c("terrain_outdoor", "야외 적응", cell(s.terrain.outdoor)),
        c("terrain_indoor", "실내 적응", cell(s.terrain.indoor)),
        c("badge_basic_info", "알림: 기본 정보 탭", bool_cell(s.badges.basic_info_tab)),
        c("badge_level_up", "알림: 레벨 업 탭", bool_cell(s.badges.level_up_tab)),
        c("badge_mystic_release", "알림: 신비 해방 탭", bool_cell(s.badges.mystic_release_tab)),
        c("badge_skill_growth", "알림: 스킬 성장", bool_cell(s.badges.skill_growth)),
        c("badge_weapon_growth", "알림: 무기 성장", bool_cell(s.badges.weapon_growth)),
        c("badge_equipment_growth", "알림: 장비 성장", bool_cell(s.badges.equipment_growth)),
        c("lock_ability_unlock", "능력개방 잠금", bool_cell(s.locks.ability_unlock_locked)),
        c("lock_simsang_bloom", "심상개화 잠금", bool_cell(s.locks.simsang_bloom_locked)),
        c("ref_id", "참조 ID(SchaleDB)", ref_id),
        c("ref_school", "참조: 학교", r.school.clone()),
        c("ref_club", "참조: 동아리", r.club.clone()),
        c("ref_full_name", "참조: 본명", r.full_name.clone()),
        c("ref_birthday", "참조: 생일", r.birthday.clone()),
        c("ref_height", "참조: 키", r.height.clone()),
        c("ref_hobby", "참조: 취미", r.hobby.clone()),
        c("ref_voice", "참조: CV", r.voice.clone()),
        c("fixed_meta_source", "고정 메타 출처", fixed_source(s)),
        c("potential_material", "능력개방 재료(고정)", fixed_str(s, |f| f.potential_material.clone())),
        c("skill_materials", "스킬 재료(고정)", fixed_str(s, |f| f.skill_materials.join(" / "))),
        c("equipment_slots", "장비 슬롯 종류(고정)", fixed_str(s, |f| f.equipment_slots.join(" / "))),
        c(
            "weapon_terrain",
            "고유무기 지형 보너스(고정)",
            fixed_str(s, |f| {
                if f.weapon_terrain.is_empty() {
                    String::new()
                } else {
                    format!("{} +{}", f.weapon_terrain, f.weapon_terrain_bonus)
                }
            }),
        ),
        c(
            "ref_base_star_grade",
            "참조: 기본 성급",
            if r.base_star_grade == 0 { String::new() } else { r.base_star_grade.to_string() },
        ),
        c("warnings", "경고", s.warnings.join("; ")),
    ]
}

/// CSV header and row for one student (same order as the Go program).
pub fn student_csv(s: &Student) -> (Vec<String>, Vec<String>) {
    let mut header: Vec<String> = [
        "name", "level", "rarity_stars", "ex_skill_level", "normal_skill_level", "passive_skill_level",
        "sub_skill_level", "bond_rank", "unique_weapon_stars", "unique_weapon_level", "combat_role", "is_pinned",
    ]
    .iter()
    .map(|x| x.to_string())
    .collect();
    let mut row = vec![
        s.name.clone(),
        cell(s.level),
        cell(s.rarity_stars),
        cell(s.ex_skill_level),
        cell(s.normal_skill_level),
        cell(s.passive_skill_level),
        cell(s.sub_skill_level),
        cell(s.bond_rank),
        cell(s.unique_weapon_stars),
        cell(s.unique_weapon_level),
        s.combat_role.clone(),
        bool_cell(s.is_pinned),
    ];
    for m in meta_columns(s) {
        header.push(m.key.to_string());
        row.push(m.value);
    }
    for i in 0..4 {
        let p = format!("equipment_{}_", i + 1);
        for k in ["equipped", "type", "item_name", "tier", "level"] {
            header.push(format!("{p}{k}"));
        }
        let eq = equipment_at(s, i);
        row.extend([bool_cell(eq.equipped), string_cell(&eq.kind), string_cell(&eq.item_name), cell(eq.tier), cell(eq.level)]);
    }
    (header, row)
}

/// One CSV record: CRLF terminator and Go encoding/csv quoting rules; no BOM (Go writes none).
pub(crate) fn csv_record(fields: &[String]) -> String {
    let mut out = String::new();
    for (i, f) in fields.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let needs = f == "\\."
            || f.contains([',', '"', '\r', '\n'])
            || f.chars().next().is_some_and(char::is_whitespace);
        if needs {
            out.push('"');
            out.push_str(&f.replace('"', "\"\""));
            out.push('"');
        } else {
            out.push_str(f);
        }
    }
    out.push_str("\r\n");
    out
}

/// Writes student.json / student.csv / student.html (frames are not written here).
pub fn export_student(s: &Student, out: &Path) -> Result<()> {
    std::fs::create_dir_all(out)?;
    std::fs::write(out.join("student.json"), serde_json::to_string_pretty(s)?)?;
    let (header, row) = student_csv(s);
    std::fs::write(out.join("student.csv"), csv_record(&header) + &csv_record(&row))?;
    std::fs::write(out.join("student.html"), html::student_page(s))?;
    Ok(())
}

/// Writes students.json / students.csv / students.html.
pub fn save_report(report: &RosterReport, out: &Path) -> Result<()> {
    std::fs::create_dir_all(out)?;
    std::fs::write(out.join("students.json"), serde_json::to_string_pretty(report)?)?;
    let first = report.entries.first().map(|e| e.student.clone()).unwrap_or_default();
    let (header, _) = student_csv(&first);
    let mut h = vec!["id".to_string(), "page".into(), "status".into(), "detail_path".into()];
    h.extend(header);
    let mut csv = csv_record(&h);
    for e in &report.entries {
        let (_, row) = student_csv(&e.student);
        let mut r = vec![e.id.clone(), e.page.to_string(), e.status.clone(), e.detail_path.clone()];
        r.extend(row);
        csv.push_str(&csv_record(&r));
    }
    std::fs::write(out.join("students.csv"), csv)?;
    std::fs::write(out.join("students.html"), html::report_page(report))?;
    Ok(())
}

#[cfg(test)]
mod tests;
