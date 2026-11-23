//! Read side of the wiki DB: what the overlay shows about a mission.
use anyhow::{Context, Result};
use rusqlite::{params, Connection, OpenFlags};
use std::path::Path;

/// A code with its Korean name and a count or score.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tally {
    pub code: String,
    pub ko: String,
    pub n: i64,
}

/// What the player wants to know before sending a squad into a mission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissionBrief {
    pub stage_id: i64,
    pub name: String,
    pub area: Option<i64>,
    pub stage_no: Option<i64>,
    /// 0 normal, 1 hard.
    pub difficulty: i64,
    pub terrain: Option<Tally>,
    pub recommended_level: Option<i64>,
    /// Enemies per attack type (bullet type) and per armor type, summons left out.
    pub attack: Vec<Tally>,
    pub armor: Vec<Tally>,
    /// Student attack types ranked by average damage multiplier against this mission's
    /// enemies; `n` is the multiplier in percent (100 = normal damage).
    pub best_attack: Vec<Tally>,
}

pub struct Wiki {
    conn: Connection,
}

/// Name without spaces and punctuation, so OCR spacing differences do not matter.
fn norm(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).collect()
}

impl Wiki {
    pub fn open(path: &Path) -> Result<Wiki> {
        let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).with_context(|| format!("opening {}", path.display()))?;
        Ok(Wiki { conn })
    }

    pub fn from_connection(conn: Connection) -> Wiki {
        Wiki { conn }
    }

    fn label(&self, kind: &str, code: &str) -> String {
        self.conn.query_row("SELECT ko FROM label WHERE kind = ?1 AND code = ?2", params![kind, code], |r| r.get(0)).unwrap_or_else(|_| code.to_string())
    }

    /// Campaign missions matching what the game shows. With the number ("19-3") the
    /// name only has to agree; with the name alone every mission of that name is returned.
    /// Normal before Hard.
    pub fn campaign_missions(&self, number: Option<(u32, u32)>, name: &str) -> Result<Vec<MissionBrief>> {
        let want = norm(name);
        let mut st = self.conn.prepare(
            "SELECT id, name_ko, area, stage_no, difficulty, terrain, level FROM stage
             WHERE category = 'Campaign' AND (?1 IS NULL OR (area = ?1 AND stage_no = ?2)) ORDER BY difficulty, id",
        )?;
        let (a, s) = number.map_or((None, None), |(a, s)| (Some(a as i64), Some(s as i64)));
        let rows = st.query_map(params![a, s], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, Option<i64>>(2)?, r.get::<_, Option<i64>>(3)?, r.get::<_, i64>(4)?, r.get::<_, Option<String>>(5)?, r.get::<_, Option<i64>>(6)?))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (id, stage_name, area, stage_no, difficulty, terrain, level) = row?;
            let stage_name = stage_name.unwrap_or_default();
            if norm(&stage_name) != want {
                continue;
            }
            out.push(self.brief(id, stage_name, area, stage_no, difficulty, terrain, level)?);
        }
        Ok(out)
    }

    #[allow(clippy::too_many_arguments)]
    fn brief(&self, id: i64, name: String, area: Option<i64>, stage_no: Option<i64>, difficulty: i64, terrain: Option<String>, level: Option<i64>) -> Result<MissionBrief> {
        let count_by = |column: &str, kind: &str| -> Result<Vec<Tally>> {
            let sql = format!(
                "SELECT e.{column}, COUNT(*) FROM stage_formation f
                 JOIN formation_enemy fe ON fe.formation_id = f.id JOIN enemy e ON e.id = fe.enemy_id
                 WHERE f.stage_id = ?1 AND e.{column} IS NOT NULL AND e.rank <> 'Summoned' GROUP BY e.{column} ORDER BY COUNT(*) DESC, e.{column}"
            );
            let mut st = self.conn.prepare(&sql)?;
            let rows = st.query_map(params![id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
            rows.map(|r| r.map(|(code, n)| Tally { ko: self.label(kind, &code), code, n }).map_err(Into::into)).collect()
        };
        let armor = count_by("armor_type", "ArmorType")?;
        let attack = count_by("bullet_type", "BulletType")?;
        let mut best = self.conn.prepare(
            "SELECT te.bullet_type, CAST(ROUND(SUM(1.0 * a.enemies * te.multiplier_bp) / SUM(a.enemies) / 100.0) AS INTEGER) AS pct
             FROM v_stage_armor a JOIN type_effectiveness te ON te.armor_type = a.armor_type
             WHERE a.stage_id = ?1 AND te.bullet_type IN (SELECT DISTINCT bullet_type FROM student)
             GROUP BY te.bullet_type ORDER BY pct DESC, te.bullet_type",
        )?;
        let best_attack = best
            .query_map(params![id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?
            .map(|r| r.map(|(code, n)| Tally { ko: self.label("BulletType", &code), code, n }).map_err(Into::into))
            .collect::<Result<Vec<_>>>()?;
        Ok(MissionBrief {
            stage_id: id,
            name,
            area,
            stage_no,
            difficulty,
            terrain: terrain.map(|code| Tally { ko: self.label("Terrain", &code), code, n: 0 }),
            recommended_level: level,
            attack,
            armor,
            best_attack,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wiki() -> Wiki {
        let conn = Connection::open_in_memory().unwrap();
        crate::create(&conn).unwrap();
        conn.execute_batch(
            "INSERT INTO source VALUES (1,'t','u','2026-11-23',NULL,'n');
             INSERT INTO label VALUES ('Terrain','Outdoor','야외'),('ArmorType','LightArmor','경장갑'),('ArmorType','HeavyArmor','중장갑'),('BulletType','Explosion','폭발'),('BulletType','Pierce','관통'),('BulletType','Normal','일반');
             INSERT INTO type_effectiveness VALUES ('Explosion','LightArmor',20000),('Explosion','HeavyArmor',10000),('Pierce','LightArmor',5000),('Pierce','HeavyArmor',20000);
             INSERT INTO enemy VALUES (1,'a','경장갑병','Minion','Main','Normal','LightArmor','S',1,'{}'),(2,'b','중장갑병','Elite','Main','Normal','HeavyArmor','S',1,'{}');
             INSERT INTO stage VALUES (10,'Campaign',NULL,19,3,0,'설원 지구 제2 지역','Outdoor',85,NULL,NULL,NULL,1),(11,'Campaign',NULL,19,3,1,'설원 지구 제2 지역','Outdoor',90,NULL,NULL,NULL,1);
             INSERT INTO stage_formation VALUES (100,10,0,'Grade1',1,1),(110,11,0,'Grade1',1,1);
             INSERT INTO formation_enemy VALUES (100,0,1),(100,1,2),(100,2,2),(110,0,1);
             INSERT INTO student VALUES (1,'a','a','s1','','',3,'Main','','','Explosion','Normal',1,1,1,'',1,1,'{}'),(2,'b','b','s2','','',3,'Main','','','Pierce','Normal',1,1,1,'',1,1,'{}');",
        )
        .unwrap();
        Wiki::from_connection(conn)
    }

    #[test]
    fn a_mission_is_found_by_number_and_name_ignoring_ocr_spacing() {
        let w = wiki();
        let found = w.campaign_missions(Some((19, 3)), "설원 지구제2 지역").unwrap();
        assert_eq!(found.iter().map(|m| (m.stage_id, m.difficulty)).collect::<Vec<_>>(), vec![(10, 0), (11, 1)], "normal before hard");
        let m = &found[0];
        assert_eq!(m.terrain.as_ref().map(|t| t.ko.as_str()), Some("야외"));
        assert_eq!(m.recommended_level, Some(85));
        assert_eq!(m.armor.iter().map(|t| (t.ko.as_str(), t.n)).collect::<Vec<_>>(), vec![("중장갑", 2), ("경장갑", 1)]);
        assert_eq!(m.attack.iter().map(|t| (t.ko.as_str(), t.n)).collect::<Vec<_>>(), vec![("일반", 3)]);
    }

    #[test]
    fn attack_types_are_ranked_by_damage_against_the_missions_enemies() {
        let w = wiki();
        let m = &w.campaign_missions(Some((19, 3)), "설원 지구 제2 지역").unwrap()[0];
        // 1 light + 2 heavy: explosion (200 + 2*100)/3 = 133 %, pierce (50 + 2*200)/3 = 150 %
        assert_eq!(m.best_attack.iter().map(|t| (t.ko.as_str(), t.n)).collect::<Vec<_>>(), vec![("관통", 150), ("폭발", 133)]);
    }

    #[test]
    fn the_name_alone_finds_a_mission_and_a_wrong_name_finds_none() {
        let w = wiki();
        assert_eq!(w.campaign_missions(None, "설원 지구 제2 지역").unwrap().len(), 2);
        assert!(w.campaign_missions(Some((19, 3)), "다른 이름").unwrap().is_empty());
        assert!(w.campaign_missions(Some((18, 3)), "설원 지구 제2 지역").unwrap().is_empty());
    }
}
