//! Prints what the raid tables of a built wiki DB hold, and checks the invariants the raid lookup
//! relies on. `cargo run -p baocr-wikidb --example raid_report -- <wiki.db>` (default
//! `../wikidb/wiki.db`). Exit code 1 when an invariant fails.
use anyhow::Result;
use rusqlite::{Connection, OpenFlags};
use std::path::PathBuf;

fn plain(v: rusqlite::types::Value) -> String {
    use rusqlite::types::Value::*;
    match v {
        Null => "-".into(),
        Integer(i) => i.to_string(),
        Real(f) => f.to_string(),
        Text(t) => t,
        Blob(b) => format!("<{} bytes>", b.len()),
    }
}

fn main() -> Result<()> {
    let path: PathBuf = std::env::args().nth(1).unwrap_or_else(|| "../wikidb/wiki.db".into()).into();
    let conn = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let mut failures: Vec<String> = Vec::new();
    let mut check = |ok: bool, what: String| {
        println!("{} {what}", if ok { "ok  " } else { "FAIL" });
        if !ok {
            failures.push(what);
        }
    };
    let one = |sql: &str| -> Result<i64> { Ok(conn.query_row(sql, [], |r| r.get(0))?) };
    let rows = |sql: &str| -> Result<Vec<Vec<String>>> {
        let mut st = conn.prepare(sql)?;
        let n = st.column_count();
        let out = st.query_map([], |r| (0..n).map(|i| r.get::<_, rusqlite::types::Value>(i).map(|v| plain(v))).collect::<std::result::Result<Vec<_>, _>>())?;
        Ok(out.collect::<std::result::Result<_, _>>()?)
    };
    let text = |sql: &str| -> Result<String> { Ok(rows(sql)?.iter().map(|r| r.join(" ")).collect::<Vec<_>>().join("\n  ")) };

    println!("{}", path.display());
    println!("schema_version {}", text("SELECT value FROM meta WHERE key = 'schema_version'")?);
    println!("\nrow counts");
    for t in ["enemy", "raid", "raid_difficulty", "raid_difficulty_enemy", "raid_season", "label"] {
        println!("  {t}: {}", one(&format!("SELECT COUNT(*) FROM {t}"))?);
    }
    println!("\nraids by kind (kind, raids, released on Global, with difficulty rows)");
    println!("  {}", text(
        "SELECT r.kind, COUNT(*), SUM(COALESCE(r.released_global, 0)), SUM(EXISTS (SELECT 1 FROM raid_difficulty d WHERE d.raid_kind = r.kind AND d.raid_id = r.id))
         FROM raid r GROUP BY r.kind ORDER BY r.kind"
    )?);
    println!("\ndifficulties by kind (kind, rows, released on Global, enemy links)");
    println!("  {}", text(
        "SELECT d.raid_kind, COUNT(*), SUM(d.released_global), (SELECT COUNT(*) FROM raid_difficulty_enemy e WHERE e.raid_kind = d.raid_kind)
         FROM raid_difficulty d GROUP BY d.raid_kind ORDER BY d.raid_kind"
    )?);
    println!("\nseasons by mode (mode, count, first start, last end, terrains)");
    println!("  {}", text("SELECT mode, COUNT(*), MIN(start_at), MAX(end_at), COUNT(DISTINCT terrain) FROM raid_season GROUP BY mode")?);

    for (name, raid_id) in [("Binah", 1), ("Hieronymus", 4)] {
        println!("\n{name} (Raid {raid_id}): idx label boss attack/armor, released on Global");
        println!("  {}", text(&format!("SELECT idx, label, boss_bullet_type, boss_armor_type, released_global FROM v_raid_difficulty WHERE raid_kind = 'Raid' AND raid_id = {raid_id} ORDER BY idx"))?);
    }
    println!("\nTimeAttack display names that more than one row shares (name, ids): the ambiguity a screen lookup has to carry");
    println!("  {}", text("SELECT display_ko, GROUP_CONCAT(id) FROM raid WHERE kind = 'TimeAttack' GROUP BY display_ko HAVING COUNT(*) > 1")?);
    println!("\nsame name in several kinds (name, kinds)");
    println!("  {}", text("SELECT display_ko, GROUP_CONCAT(kind) FROM raid WHERE kind <> 'TimeAttack' GROUP BY display_ko HAVING COUNT(DISTINCT kind) > 1")?);
    println!("\nchecks");

    check(one("SELECT COUNT(*) FROM raid")? > 0, "there are raids".into());
    check(one("SELECT COUNT(*) FROM raid r WHERE NOT EXISTS (SELECT 1 FROM raid_difficulty d WHERE d.raid_kind = r.kind AND d.raid_id = r.id)")? == 0, "every raid has difficulty rows".into());
    check(one("SELECT COUNT(*) FROM raid_difficulty d WHERE NOT EXISTS (SELECT 1 FROM raid_difficulty_enemy e WHERE e.raid_kind = d.raid_kind AND e.raid_id = d.raid_id AND e.idx = d.idx)")? == 0, "every difficulty has enemies".into());
    let unlinked = one(
        "SELECT COUNT(*) FROM enemy e WHERE e.id BETWEEN 7300000 AND 7499999
           AND NOT EXISTS (SELECT 1 FROM raid_difficulty_enemy r WHERE r.enemy_id = e.id) AND NOT EXISTS (SELECT 1 FROM formation_enemy f WHERE f.enemy_id = e.id)",
    )?;
    check(unlinked == 0, format!("every raid enemy (ids 73xxxxx-74xxxxx, {} of them) is linked to a raid difficulty or a stage; {unlinked} are not", one("SELECT COUNT(*) FROM enemy WHERE id BETWEEN 7300000 AND 7499999")?));
    check(one("SELECT COUNT(*) FROM raid_difficulty WHERE released_global = 1 AND boss_enemy_id IS NULL AND raid_kind = 'Raid'")? == 0, "every released total-assault difficulty has a lead enemy".into());
    check(one("SELECT COUNT(*) FROM raid_season s WHERE NOT EXISTS (SELECT 1 FROM raid r WHERE r.kind = 'Raid' AND r.id = s.raid_id)")? == 0, "every season belongs to a raid".into());
    check(one("SELECT COUNT(*) FROM raid_season WHERE terrain NOT IN ('Street', 'Outdoor', 'Indoor')")? == 0, "every season has a known terrain".into());
    check(one("SELECT COUNT(*) FROM raid_season WHERE end_at <= start_at")? == 0, "every season ends after it starts".into());
    check(one("SELECT COUNT(*) FROM raid WHERE kind = 'TimeAttack' AND display_ko = ''")? == 0, "every time attack has a display name".into());
    check(
        one("SELECT COUNT(*) FROM pragma_foreign_key_check")? == 0,
        "foreign keys hold".into(),
    );
    // the facts the lookup is built on: Binah is Normal up to Extreme and Pierce from Insane; Hieronymus Normal then Explosion
    let attack = |raid: i64| text(&format!("SELECT GROUP_CONCAT(COALESCE(boss_bullet_type, '-'), ',') FROM (SELECT * FROM v_raid_difficulty WHERE raid_kind = 'Raid' AND raid_id = {raid} ORDER BY idx)"));
    check(attack(1)? == "Normal,Normal,Normal,Normal,Normal,Pierce,Pierce,Pierce", format!("Binah attack types per difficulty: {}", attack(1)?));
    check(attack(4)? == "Normal,Normal,Normal,Normal,Normal,Explosion,Explosion,Explosion", format!("Hieronymus attack types per difficulty: {}", attack(4)?));

    if failures.is_empty() {
        println!("\nall checks passed");
        Ok(())
    } else {
        println!("\n{} check(s) failed", failures.len());
        std::process::exit(1)
    }
}
