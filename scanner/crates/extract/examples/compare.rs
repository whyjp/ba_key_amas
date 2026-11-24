//! Compare the Rust extractor with a Go scan directory (details/NNN/{detail.png,student.json}).
//! usage: cargo run --release -p baocr-extract --example compare -- <scan-dir> [limit]
use baocr_core::model::Student;
use baocr_extract::{Extractor, Options};
use baocr_ocr::PpOcr;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Instant;

fn show(v: Option<i32>) -> String {
    v.map(|x| x.to_string()).unwrap_or_else(|| "null".into())
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let dir = PathBuf::from(args.get(1).expect("scan dir"));
    let limit: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(usize::MAX);
    let ocr = PpOcr::new(&PpOcr::default_models_dir())?;
    let ex = Extractor::new(&ocr);
    let mut agree: BTreeMap<&str, usize> = BTreeMap::new();
    let mut total: BTreeMap<&str, usize> = BTreeMap::new();
    let mut diffs: Vec<String> = Vec::new();
    let t0 = Instant::now();
    let mut n = 0;
    let mut ids: Vec<_> = std::fs::read_dir(dir.join("details"))?.filter_map(|e| e.ok()).map(|e| e.path()).collect();
    ids.sort();
    for d in ids.into_iter().take(limit) {
        let (Ok(img), Ok(txt)) = (image::open(d.join("detail.png")), std::fs::read_to_string(d.join("student.json"))) else { continue };
        let go: Student = serde_json::from_str(&txt)?;
        let rs = ex.profile(&img.to_rgb8(), "compare", &Options::default())?;
        n += 1;
        let id = d.file_name().unwrap().to_string_lossy().to_string();
        let mut cmp = |field: &'static str, g: String, r: String| {
            *total.entry(field).or_default() += 1;
            if g == r {
                *agree.entry(field).or_default() += 1;
            } else {
                diffs.push(format!("{id} {} {field}: go={g} rust={r}", go.name));
            }
        };
        cmp("name", go.name.clone(), rs.name.clone());
        cmp("bond", show(go.bond_rank), show(rs.bond_rank));
        cmp("level", show(go.level), show(rs.level));
        cmp("ex_skill", show(go.ex_skill_level), show(rs.ex_skill_level));
        cmp("basic_skill", show(go.normal_skill_level), show(rs.normal_skill_level));
        cmp("passive_skill", show(go.passive_skill_level), show(rs.passive_skill_level));
        cmp("sub_skill", show(go.sub_skill_level), show(rs.sub_skill_level));
        cmp("max_hp", show(go.stats.max_hp), show(rs.stats.max_hp));
        cmp("attack", show(go.stats.attack), show(rs.stats.attack));
        cmp("defense", show(go.stats.defense), show(rs.stats.defense));
        cmp("healing", show(go.stats.healing), show(rs.stats.healing));
        cmp("exp_required", show(go.exp.required), show(rs.exp.required));
        cmp("exp_current", show(go.exp.current), show(rs.exp.current));
        cmp("rarity", show(go.rarity_stars), show(rs.rarity_stars));
        cmp("pinned", format!("{:?}", go.is_pinned), format!("{:?}", rs.is_pinned));
        cmp("combat_role", go.combat_role.clone(), rs.combat_role.clone());
        cmp("weapon_stars", show(go.unique_weapon_stars), show(rs.unique_weapon_stars));
        let eq = |s: &Student| s.equipment.iter().map(|e| e.item_name.clone().unwrap_or_default()).collect::<Vec<_>>().join("/");
        cmp("equipment", eq(&go), eq(&rs));
    }
    let secs = t0.elapsed().as_secs_f64();
    println!("students={n} {secs:.0}s ({:.1}s/profile)", secs / n.max(1) as f64);
    println!("{:<14}{:>7}{:>7}", "field", "agree", "of");
    for (f, t) in &total {
        println!("{f:<14}{:>7}{t:>7}", agree.get(f).copied().unwrap_or(0));
    }
    println!("--- diffs ({})", diffs.len());
    for d in diffs.iter().take(80) {
        println!("{d}");
    }
    Ok(())
}
