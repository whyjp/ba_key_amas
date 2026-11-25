//! namedict: builds the Korean student dictionary used for name correction and
//! as fixed-meta reference (SchaleDB community data; bluearchive.wiki has no
//! Korean names). Run after a game patch: `cargo run -p baocr-namedict`.
use anyhow::Result;
use serde_json::{json, Map, Value};

const STUDENTS: &str = "https://schaledb.com/data/kr/students.min.json";
const ITEMS: &str = "https://schaledb.com/data/kr/items.min.json";

fn tactic(k: &str) -> &'static str {
    match k { "DamageDealer" => "딜러", "Tanker" => "탱커", "Healer" => "힐러", "Supporter" => "서포터", "Vehicle" => "T.S", _ => "" }
}
fn position(k: &str) -> &'static str {
    match k { "Front" => "FRONT", "Middle" => "MIDDLE", "Back" => "BACK", _ => "" }
}
fn attack(k: &str) -> &'static str {
    match k { "Explosion" => "폭발", "Pierce" => "관통", "Mystic" => "신비", "Sonic" => "진동", _ => "" }
}
fn armor(k: &str) -> &'static str {
    match k { "LightArmor" => "경장갑", "HeavyArmor" => "중장갑", "Unarmed" => "특수장갑", "ElasticArmor" => "탄력장갑", "CompositeArmor" => "복합장갑", _ => "" }
}

/// "리오(무장)" -> ("리오", "무장"); "시로코*테러" stays whole (a different character).
fn split_name(n: &str) -> (String, String) {
    if let (Some(i), true) = (n.find('('), n.ends_with(')')) {
        if i > 0 {
            return (n[..i].to_string(), n[i + 1..n.len() - 1].to_string());
        }
    }
    (n.to_string(), String::new())
}

fn s(v: &Value, k: &str) -> String {
    v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string()
}
fn i(v: &Value, k: &str) -> i64 {
    v.get(k).and_then(|x| x.as_i64()).unwrap_or(0)
}

fn main() -> Result<()> {
    let out = std::env::args().nth(1).unwrap_or_else(|| "crates/names/data/student_names_kr.json".into());
    let students: Map<String, Value> = ureq::get(STUDENTS).call()?.into_json()?;
    let items: Map<String, Value> = ureq::get(ITEMS).call()?.into_json()?;
    let item_name = |id: i64| items.get(&id.to_string()).map(|v| s(v, "Name")).unwrap_or_default();

    let mut names: Vec<Value> = Vec::new();
    for r in students.values() {
        let name = s(r, "Name");
        let (base, outfit) = split_name(&name);
        let role = match s(r, "SquadType").as_str() { "Main" => "STRIKER", "Support" => "SPECIAL", _ => "" };
        // distinct materials any skill level needs, in first-seen order
        let mut seen: Vec<String> = Vec::new();
        for key in ["SkillExMaterial", "SkillMaterial"] {
            for group in r.get(key).and_then(|g| g.as_array()).into_iter().flatten() {
                for id in group.as_array().into_iter().flatten().filter_map(|x| x.as_i64()) {
                    let n = item_name(id);
                    if !n.is_empty() && !seen.contains(&n) {
                        seen.push(n);
                    }
                }
            }
        }
        let weapon = r.get("Weapon").cloned().unwrap_or(Value::Null);
        let full = format!("{} {}", s(r, "FamilyName"), s(r, "PersonalName")).trim().to_string();
        let mut e = Map::new();
        e.insert("id".into(), json!(i(r, "Id")));
        e.insert("name".into(), json!(name));
        e.insert("base".into(), json!(base));
        if !outfit.is_empty() {
            e.insert("outfit".into(), json!(outfit));
        }
        e.insert("role".into(), json!(role));
        e.insert("tactic_role".into(), json!(tactic(&s(r, "TacticRole"))));
        e.insert("position".into(), json!(position(&s(r, "Position"))));
        e.insert("attack_type".into(), json!(attack(&s(r, "BulletType"))));
        e.insert("armor_type".into(), json!(armor(&s(r, "ArmorType"))));
        e.insert("weapon_type".into(), json!(s(r, "WeaponType")));
        e.insert("star_grade".into(), json!(i(r, "StarGrade")));
        e.insert("school".into(), json!(s(r, "School")));
        e.insert("club".into(), json!(s(r, "Club")));
        for (k, src) in [("school_year", "SchoolYear"), ("age", "CharacterAge"), ("birthday", "Birthday"), ("height", "CharHeightMetric"),
                         ("hobby", "Hobby"), ("designer", "Designer"), ("illustrator", "Illustrator"), ("voice", "CharacterVoice")] {
            let v = s(r, src);
            if !v.is_empty() {
                e.insert(k.into(), json!(v));
            }
        }
        if !full.is_empty() {
            e.insert("full_name".into(), json!(full));
        }
        e.insert("terrain_urban".into(), json!(i(r, "StreetBattleAdaptation")));
        e.insert("terrain_outdoor".into(), json!(i(r, "OutdoorBattleAdaptation")));
        e.insert("terrain_indoor".into(), json!(i(r, "IndoorBattleAdaptation")));
        let slots: Vec<Value> = r.get("Equipment").and_then(|x| x.as_array()).cloned().unwrap_or_default();
        if !slots.is_empty() {
            e.insert("equipment_slots".into(), Value::Array(slots));
        }
        let pot = item_name(i(r, "PotentialMaterial"));
        if !pot.is_empty() {
            e.insert("potential_material".into(), json!(pot));
        }
        if !seen.is_empty() {
            e.insert("skill_materials".into(), json!(seen));
        }
        let wt = s(&weapon, "AdaptationType");
        if !wt.is_empty() {
            e.insert("weapon_terrain".into(), json!(wt));
            e.insert("weapon_terrain_bonus".into(), json!(i(&weapon, "AdaptationValue")));
        }
        names.push(Value::Object(e));
    }
    names.sort_by_key(|v| v["id"].as_i64().unwrap_or(0));
    let dict = json!({ "source": STUDENTS, "fetched": chrono::Local::now().format("%Y-%m-%d").to_string(), "names": names });
    std::fs::write(&out, serde_json::to_string_pretty(&dict)?)?;
    println!("wrote {} names to {out}", names.len());
    Ok(())
}
