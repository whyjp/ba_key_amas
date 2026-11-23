//! Head count against the HP-weighted mix of armor and attack types, side by side, for a few stages
//! and raid difficulties, and what each does to the fit tier of sample students.
//! `cargo run -p baocr-wikidb --example weight_report -- <wiki.db>` (default `../wikidb/wiki.db`).
//! Reads the database only: nothing here touches the game.
use anyhow::Result;
use baocr_wikidb::{fit_tier, matchup, Fit, MissionBrief, RaidKind, Share, StudentBrief, TypeTable, Wiki};
use std::path::PathBuf;

/// Stages worth looking at: a swarm with champions, a mixed campaign map, a map with waves, a school dungeon.
const STAGES: [i64; 5] = [30501, 30401, 1031401, 1141102, 60304];
/// (kind, raid id, difficulty idx): the drum crab, a single boss, one with relics, one with five elites.
const RAIDS: [(RaidKind, i64, i64); 7] = [
    (RaidKind::Raid, 14, 0),
    (RaidKind::Raid, 14, 5),
    (RaidKind::Raid, 1, 0),
    (RaidKind::Raid, 1, 5),
    (RaidKind::Raid, 4, 5),
    (RaidKind::Raid, 12, 0),
    (RaidKind::Raid, 5, 7),
];

fn mix(list: &[Share], by_heads: bool) -> String {
    let total: f64 = list.iter().map(|s| if by_heads { s.heads as f64 } else { s.share }).sum();
    let mut parts: Vec<(f64, &str)> = list.iter().map(|s| ((if by_heads { s.heads as f64 } else { s.share }) / total * 100.0, s.ko.as_str())).collect();
    parts.sort_by(|a, b| b.0.total_cmp(&a.0));
    parts.iter().map(|(p, ko)| format!("{ko} {p:.0}%")).collect::<Vec<_>>().join(" / ")
}

/// The same brief with shares made of head counts: the old model.
fn by_heads(b: &MissionBrief) -> MissionBrief {
    let redo = |list: &[Share]| {
        let total: f64 = list.iter().map(|s| s.heads as f64).sum();
        list.iter().map(|s| Share { share: s.heads as f64 / total, ..s.clone() }).collect::<Vec<_>>()
    };
    MissionBrief { armor: redo(&b.armor), attack: redo(&b.attack), ..b.clone() }
}

fn tier(f: Fit) -> &'static str {
    match f {
        Fit::Excellent => "강적합",
        Fit::Good => "적합",
        Fit::Average => "보통",
        Fit::Poor => "부적합",
    }
}

/// One striker of each attack type with a plain armor and an average terrain, so only the mix differs.
fn samples(students: &[StudentBrief]) -> Vec<StudentBrief> {
    ["Explosion", "Pierce", "Mystic", "Sonic"]
        .iter()
        .filter_map(|b| students.iter().find(|s| s.squad_type == "Main" && s.bullet_type == *b && s.role == "DamageDealer").cloned())
        .collect()
}

fn report(name: &str, brief: &MissionBrief, types: &TypeTable, who: &[StudentBrief]) {
    let heads = by_heads(brief);
    println!("\n{name} (Lv.{})", brief.recommended_level.map_or("?".into(), |l| l.to_string()));
    println!("  armor  heads: {}", mix(&brief.armor, true));
    println!("  armor  HP   : {}", mix(&brief.armor, false));
    println!("  attack heads: {}", mix(&brief.attack, true));
    println!("  attack HP   : {}", mix(&brief.attack, false));
    let terrain = brief.terrain.as_ref().map(|t| t.code.as_str());
    let cells: Vec<String> = who
        .iter()
        .map(|s| {
            let (a, b) = (fit_tier(matchup(s, &heads, terrain, types)), fit_tier(matchup(s, brief, terrain, types)));
            let (ma, mb) = (matchup(s, &heads, terrain, types).attack, matchup(s, brief, terrain, types).attack);
            format!("{} x{ma:.2}→x{mb:.2} {}{}", s.bullet_type, if a == b { tier(a).to_string() } else { format!("{}→{}", tier(a), tier(b)) }, if a == b { "" } else { " *" })
        })
        .collect();
    println!("  fit (heads→HP): {}", cells.join(" | "));
}

fn main() -> Result<()> {
    let path: PathBuf = std::env::args().nth(1).unwrap_or_else(|| "../wikidb/wiki.db".into()).into();
    let wiki = Wiki::open(&path)?;
    let types = wiki.type_table()?;
    let who = samples(&wiki.students()?);
    println!("{}  (* = the tier changes with the weighting)", path.display());
    for id in STAGES {
        match wiki.mission_by_id(id)? {
            Some(b) => report(&format!("stage {id} {}", b.name), &b, &types, &who),
            None => println!("\nstage {id}: not in the database"),
        }
    }
    for (kind, id, idx) in RAIDS {
        match wiki.raid_mission(kind, id, idx, None)? {
            Some(b) => report(&format!("raid {} {id} {}", kind.as_str(), b.name), &b, &types, &who),
            None => println!("\nraid {id} idx {idx}: not in the database"),
        }
    }
    Ok(())
}
