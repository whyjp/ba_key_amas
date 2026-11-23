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

/// A student as the formation helpers need it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StudentBrief {
    pub id: i64,
    pub name: String,
    /// Attack type code (Explosion, Pierce, Mystic, Sonic).
    pub bullet_type: String,
    /// Armor type code.
    pub armor_type: String,
    /// Terrain adaptation, 0 (worst) to 4 (best): the game's five faces.
    pub street: u8,
    pub outdoor: u8,
    pub indoor: u8,
    /// "Main" (STRIKER) or "Support" (SPECIAL).
    pub squad_type: String,
}

impl StudentBrief {
    /// Adaptation for a terrain code (Street | Outdoor | Indoor).
    pub fn adaptation(&self, terrain: &str) -> Option<u8> {
        match terrain {
            "Street" => Some(self.street),
            "Outdoor" => Some(self.outdoor),
            "Indoor" => Some(self.indoor),
            _ => None,
        }
    }
}

/// A student who interacts with a piece of furniture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FurnitureStudent {
    pub id: i64,
    pub name: String,
}

/// A furniture item and the students who interact with it in the cafe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FurnitureBrief {
    pub id: i64,
    pub name: String,
    pub category: String,
    pub sub_category: Option<String>,
    /// Ordered by student id.
    pub students: Vec<FurnitureStudent>,
}

pub struct Wiki {
    conn: Connection,
}

/// Edit distance between two short strings.
fn distance(a: &[char], b: &[char]) -> usize {
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut cur = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            cur.push((prev[j] + usize::from(ca != cb)).min(prev[j + 1] + 1).min(cur[j] + 1));
        }
        prev = cur;
    }
    prev[b.len()]
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

    /// (attack type, armor type, multiplier where 1.0 is 100 %).
    pub(crate) fn type_rows(&self) -> Result<Vec<(String, String, f64)>> {
        let mut st = self.conn.prepare("SELECT bullet_type, armor_type, multiplier_bp FROM type_effectiveness")?;
        let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)? as f64 / 10000.0)))?;
        rows.collect::<std::result::Result<_, _>>().map_err(Into::into)
    }

    /// Every Global-released student.
    pub fn students(&self) -> Result<Vec<StudentBrief>> {
        let mut st = self.conn.prepare(
            "SELECT id, name_ko, bullet_type, armor_type, COALESCE(street_adapt, 0), COALESCE(outdoor_adapt, 0), COALESCE(indoor_adapt, 0), squad_type
             FROM student WHERE released_global = 1 ORDER BY id",
        )?;
        let rows = st.query_map([], |r| {
            Ok(StudentBrief {
                id: r.get(0)?,
                name: r.get(1)?,
                bullet_type: r.get(2)?,
                armor_type: r.get(3)?,
                street: r.get::<_, i64>(4)?.clamp(0, 4) as u8,
                outdoor: r.get::<_, i64>(5)?.clamp(0, 4) as u8,
                indoor: r.get::<_, i64>(6)?.clamp(0, 4) as u8,
                squad_type: r.get(7)?,
            })
        })?;
        rows.collect::<std::result::Result<_, _>>().map_err(Into::into)
    }

    /// The student a name read by OCR stands for. Punctuation and spaces are ignored
    /// ("시로코*테러" and "시로코테러" are one name); a single wrong or missing character is
    /// tolerated only when exactly one student is that close. Ambiguity gives None: a
    /// wrong match would put the wrong icons on a portrait.
    pub fn match_student(students: &[StudentBrief], ocr: &str) -> Option<StudentBrief> {
        Wiki::match_student_as(students, ocr, None)
    }

    /// Like `match_student`, with the kind of squad slot the name was read in ("Main" for
    /// a striker, "Support" for a special) to tell students that share a name apart.
    pub fn match_student_as(students: &[StudentBrief], ocr: &str, squad_type: Option<&str>) -> Option<StudentBrief> {
        if let Some(kind) = squad_type {
            let of_kind: Vec<StudentBrief> = students.iter().filter(|s| s.squad_type == kind).cloned().collect();
            if let Some(found) = Wiki::closest(&of_kind, ocr) {
                return Some(found);
            }
        }
        Wiki::closest(students, ocr)
    }

    fn closest(students: &[StudentBrief], ocr: &str) -> Option<StudentBrief> {
        let want: Vec<char> = norm(ocr).chars().collect();
        // a one-letter name ("슌") can only be an exact match: the edit rule below needs 3 letters
        if want.is_empty() {
            return None;
        }
        let mut best: Option<(usize, &StudentBrief)> = None;
        let mut tie = false;
        let mut tied: Vec<&StudentBrief> = Vec::new();
        for s in students {
            let have: Vec<char> = norm(&s.name).chars().collect();
            let d = distance(&want, &have);
            if d > 1 || (d == 1 && have.len().min(want.len()) < 3) {
                continue;
            }
            match best {
                Some((bd, _)) if d > bd => {}
                Some((bd, _)) if d == bd => {
                    tie = true;
                    tied.push(s);
                }
                _ => {
                    best = Some((d, s));
                    tie = false;
                    tied = vec![s];
                }
            }
        }
        // students that share a name but look the same to the helpers (same types and
        // adaptation) are not worth refusing over
        let same_icons = |a: &StudentBrief, b: &StudentBrief| (&a.bullet_type, &a.armor_type, a.street, a.outdoor, a.indoor) == (&b.bullet_type, &b.armor_type, b.street, b.outdoor, b.indoor);
        if tie && tied.iter().all(|t| same_icons(t, tied[0])) {
            return Some(tied[0].clone());
        }
        best.filter(|_| !tie).map(|(_, s)| s.clone())
    }

    /// Every Global-released furniture item with at least one interacting (Global-released)
    /// student, ordered by id; each item's students are ordered by student id.
    pub fn furniture_with_students(&self) -> Result<Vec<FurnitureBrief>> {
        let mut st = self.conn.prepare(
            "SELECT f.id, f.name_ko, f.category, f.sub_category, s.id, s.name_ko
             FROM furniture f
             JOIN furniture_interaction fi ON fi.furniture_id = f.id
             JOIN student s ON s.id = fi.student_id AND s.released_global = 1
             WHERE f.released_global = 1 ORDER BY f.id, s.id",
        )?;
        let rows = st.query_map([], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, Option<String>>(3)?, FurnitureStudent { id: r.get(4)?, name: r.get(5)? }))
        })?;
        let mut out: Vec<FurnitureBrief> = Vec::new();
        for row in rows {
            let (id, name, category, sub_category, student) = row?;
            match out.last_mut() {
                Some(last) if last.id == id => last.students.push(student),
                _ => out.push(FurnitureBrief { id, name, category, sub_category, students: vec![student] }),
            }
        }
        Ok(out)
    }

    /// The furniture item a name read by OCR from the game screen stands for. Spaces and
    /// punctuation are ignored; an exact name wins. One wrong or missing character is
    /// tolerated only for names of 4+ characters. Several items sharing the name (or equally
    /// close to it) give None, unless all of them have the same students: showing the wrong
    /// students is worse than showing none.
    pub fn match_furniture(items: &[FurnitureBrief], ocr: &str) -> Option<FurnitureBrief> {
        let want: Vec<char> = norm(ocr).chars().collect();
        if want.is_empty() {
            return None;
        }
        let ids = |f: &FurnitureBrief| f.students.iter().map(|s| s.id).collect::<Vec<_>>();
        let settle = |found: Vec<&FurnitureBrief>| -> Option<FurnitureBrief> {
            let first = *found.first()?;
            found.iter().all(|f| ids(f) == ids(first)).then(|| first.clone())
        };
        let exact: Vec<&FurnitureBrief> = items.iter().filter(|f| norm(&f.name).chars().eq(want.iter().copied())).collect();
        if !exact.is_empty() {
            return settle(exact);
        }
        let near: Vec<&FurnitureBrief> = items
            .iter()
            .filter(|f| {
                let have: Vec<char> = norm(&f.name).chars().collect();
                have.len().min(want.len()) >= 4 && distance(&want, &have) == 1
            })
            .collect();
        settle(near)
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
            // OCR can cut the end of a name off (a dialog over the popup): with the stage number
            // known, a name that is the start of the stored one is accepted too
            let have = norm(&stage_name);
            let agrees = have == want || (number.is_some() && want.chars().count() >= 2 && have.starts_with(&want));
            if !agrees {
                continue;
            }
            out.push(self.brief(id, stage_name, area, stage_no, difficulty, terrain, level)?);
        }
        Ok(out)
    }

    /// Missions of 현상수배, 특별의뢰 and 학원교류회: a place as the game names it
    /// ("현상수배-고가도로", "거점 방어", "트리니티") and a stage number in it.
    pub fn dungeon_missions(&self, place: &str, stage_no: u32) -> Result<Vec<MissionBrief>> {
        let want = norm(place);
        if want.chars().count() < 2 {
            return Ok(Vec::new());
        }
        let mut st = self.conn.prepare(
            "SELECT s.id, l.ko, s.stage_no, s.terrain, s.level FROM stage s JOIN label l ON l.kind = 'StageType' AND l.code = s.type
             WHERE s.category IN ('Bounty', 'WeekDungeon', 'SchoolDungeon') AND s.stage_no = ?1 ORDER BY s.id",
        )?;
        let rows = st.query_map(params![stage_no], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<i64>>(2)?, r.get::<_, Option<String>>(3)?, r.get::<_, Option<i64>>(4)?))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (id, label, stage_no, terrain, level) = row?;
            // the label carries the category in front ("학원교류회 - 트리니티"): the place is its end
            if norm(&label).ends_with(&want) {
                out.push(self.brief(id, label, None, stage_no, 0, terrain, level)?);
            }
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
    fn ocr_names_are_matched_to_students_and_ambiguity_gives_none() {
        let mk = |id: i64, name: &str| StudentBrief { id, name: name.into(), bullet_type: "Pierce".into(), armor_type: "Light".into(), street: 4, outdoor: 2, indoor: 0, squad_type: "Main".into() };
        let all = vec![mk(1, "시로코*테러"), mk(2, "시로코"), mk(3, "호시노(무장)"), mk(4, "호시노"), mk(5, "슌"), mk(6, "하나코(수영복)")];
        let id = |t: &str| Wiki::match_student(&all, t).map(|s| s.id);
        assert_eq!(id("시로코테러"), Some(1), "the star is lost in OCR");
        assert_eq!(id("시로코"), Some(2), "an exact name beats a longer one");
        assert_eq!(id("호시노(무장)"), Some(3));
        assert_eq!(id("호시노무장"), Some(3));
        assert_eq!(id("하나코(수영북)"), Some(6), "one wrong character");
        assert_eq!(id("순"), None, "too short to trust one wrong character");
        assert_eq!(id("슌"), Some(5), "but a one-letter name matches exactly");
        assert_eq!(id("아무개"), None);
        assert_eq!(id(""), None);
        // two students equally close and different to the helpers: no answer
        let mut other = mk(11, "마리(체육목)");
        other.bullet_type = "Mystic".into();
        let twins = vec![mk(10, "마리(체육복)"), other];
        assert_eq!(Wiki::match_student(&twins, "마리(체육북)"), None);
        // the same name twice: the kind of slot decides, and identical-looking twins are not refused
        let mut special = mk(21, "호시노(무장)");
        special.squad_type = "Support".into();
        special.bullet_type = "Mystic".into();
        let pair = vec![mk(20, "호시노(무장)"), special];
        assert_eq!(Wiki::match_student(&pair, "호시노(무장)"), None, "no hint, different students");
        assert_eq!(Wiki::match_student_as(&pair, "호시노(무장)", Some("Support")).map(|s| s.id), Some(21));
        assert_eq!(Wiki::match_student_as(&pair, "호시노(무장)", Some("Main")).map(|s| s.id), Some(20));
        let clones = vec![mk(30, "호시노(무장)"), mk(31, "호시노(무장)")];
        assert_eq!(Wiki::match_student(&clones, "호시노(무장)").map(|s| s.id), Some(30));
    }

    fn furniture_wiki() -> Wiki {
        let conn = Connection::open_in_memory().unwrap();
        crate::create(&conn).unwrap();
        conn.execute_batch(
            "INSERT INTO source VALUES (1,'t','u','2026-11-23',NULL,'n');
             INSERT INTO student VALUES (1,'a','a','아루','','',3,'Main','','','Explosion','Normal',1,1,1,'',1,1,'{}'),(2,'b','b','무츠키','','',3,'Main','','','Pierce','Normal',1,1,1,'',1,1,'{}'),
                                        (3,'c','c','미출시','','',3,'Main','','','Pierce','Normal',1,1,1,'',0,1,'{}');
             INSERT INTO furniture (id,name_ko,category,sub_category,released_global,interaction_global,source_id,raw) VALUES
                 (10,'고급 소파','Furnitures','Sofa',1,1,1,'{}'),(11,'원형 테이블','Furnitures','Table',1,1,1,'{}'),(12,'화분','Decorations',NULL,1,0,1,'{}'),
                 (13,'일본 한정 의자','Furnitures','Chair',0,1,1,'{}'),(14,'원형 테이블','Furnitures','Table',1,1,1,'{}'),(15,'미출시 학생 의자','Furnitures','Chair',1,1,1,'{}');
             INSERT INTO furniture_interaction VALUES (11,2,0),(11,1,0),(10,1,3),(13,1,0),(14,2,0),(15,3,0);",
        )
        .unwrap();
        Wiki::from_connection(conn)
    }

    #[test]
    fn furniture_comes_with_its_students_in_id_order() {
        let list = furniture_wiki().furniture_with_students().unwrap();
        let shape: Vec<(i64, &str, Vec<i64>)> = list.iter().map(|f| (f.id, f.name.as_str(), f.students.iter().map(|s| s.id).collect())).collect();
        // 12 has no interaction, 13 is not released on Global, 15 only has an unreleased student
        assert_eq!(shape, vec![(10, "고급 소파", vec![1]), (11, "원형 테이블", vec![1, 2]), (14, "원형 테이블", vec![2])]);
        assert_eq!(list[1].students[1], FurnitureStudent { id: 2, name: "무츠키".into() });
        assert_eq!((list[0].category.as_str(), list[0].sub_category.as_deref()), ("Furnitures", Some("Sofa")));
    }

    #[test]
    fn ocr_furniture_names_are_matched_and_ambiguity_gives_none() {
        let mk = |id: i64, name: &str, students: &[i64]| FurnitureBrief {
            id,
            name: name.into(),
            category: "Furnitures".into(),
            sub_category: None,
            students: students.iter().map(|&s| FurnitureStudent { id: s, name: format!("s{s}") }).collect(),
        };
        let all = vec![mk(1, "고급 소파", &[1]), mk(2, "원형 테이블", &[1, 2]), mk(3, "원형 테이블", &[2]), mk(4, "벚꽃 화분", &[5]), mk(5, "별 의자", &[6]), mk(6, "별 의지", &[7])];
        let id = |t: &str| Wiki::match_furniture(&all, t).map(|f| f.id);
        assert_eq!(id("고급소파"), Some(1), "spacing is ignored");
        assert_eq!(id("고급 소파!"), Some(1), "punctuation is ignored");
        assert_eq!(id("고급 소화"), Some(1), "one wrong character in a 4+ letter name");
        assert_eq!(id("고급 소"), None, "3 letters left: too short to trust");
        assert_eq!(id("고급 소소파"), Some(1), "one extra character");
        assert_eq!(id("원형 테이블"), None, "same name, different students");
        assert_eq!(id("별 의자"), Some(5), "an exact name beats a one-character neighbour");
        assert_eq!(id("별 의차"), None, "two short items: no error tolerated");
        assert_eq!(id("벚꽃 화본"), Some(4));
        assert_eq!(id("벚꽃 화"), None, "3 letters left");
        assert_eq!(id("아무거나"), None);
        assert_eq!(id(""), None);
        // same name and same students: nothing to disagree about
        let same = vec![mk(10, "책장", &[1]), mk(11, "책장", &[1])];
        assert_eq!(Wiki::match_furniture(&same, "책장").map(|f| f.id), Some(10));
        // two long names equally close with different students
        let close = vec![mk(20, "큰 원목 책상", &[1]), mk(21, "큰 원목 책장", &[2])];
        assert_eq!(Wiki::match_furniture(&close, "큰 원목 책사"), None);
        assert_eq!(Wiki::match_furniture(&close, "큰 원목 책상").map(|f| f.id), Some(20));
    }

    #[test]
    fn a_place_and_stage_number_find_a_dungeon_mission() {
        let conn = Connection::open_in_memory().unwrap();
        crate::create(&conn).unwrap();
        conn.execute_batch(
            "INSERT INTO source VALUES (1,'t','u','2026-11-23',NULL,'n');
             INSERT INTO label VALUES ('StageType','ChaserA','현상수배 - 고가도로'),('StageType','ChaserC','현상수배 - 교실'),('StageType','Blood','거점 방어'),('StageType','SchoolA','학원교류회 - 트리니티'),('Terrain','Street','시가지');
             INSERT INTO stage VALUES (30103,'Bounty','ChaserA',NULL,3,NULL,'현상수배 - 고가도로','Street',20,NULL,NULL,NULL,1),(30303,'Bounty','ChaserC',NULL,3,NULL,'현상수배 - 교실','Indoor',20,NULL,NULL,NULL,1),
                                   (32107,'WeekDungeon','Blood',NULL,7,NULL,'거점 방어','Indoor',60,NULL,NULL,NULL,1),(60103,'SchoolDungeon','SchoolA',NULL,3,NULL,'학원교류회 - 트리니티','Outdoor',40,NULL,NULL,NULL,1);",
        )
        .unwrap();
        let w = Wiki::from_connection(conn);
        let one = |place: &str, n: u32| w.dungeon_missions(place, n).unwrap().iter().map(|m| m.stage_id).collect::<Vec<_>>();
        assert_eq!(one("현상수배-고가도로", 3), vec![30103], "the list card says it the way the label does, spacing aside");
        assert_eq!(one("거점방어", 7), vec![32107]);
        assert_eq!(one("트리니티", 3), vec![60103], "a school alone is the end of its label");
        assert_eq!(one("교실", 3), vec![30303]);
        assert!(one("고가도로", 4).is_empty(), "no such stage number");
        assert!(one("", 3).is_empty());
    }

    #[test]
    fn the_name_alone_finds_a_mission_and_a_wrong_name_finds_none() {
        let w = wiki();
        assert_eq!(w.campaign_missions(None, "설원 지구 제2 지역").unwrap().len(), 2);
        assert!(w.campaign_missions(Some((19, 3)), "다른 이름").unwrap().is_empty());
        assert_eq!(w.campaign_missions(Some((19, 3)), "설원 지구").unwrap().len(), 2, "a cut-off name is fine when the number is known");
        assert!(w.campaign_missions(None, "설원 지구").unwrap().is_empty(), "but not when the name is all there is");
        assert!(w.campaign_missions(Some((18, 3)), "설원 지구 제2 지역").unwrap().is_empty());
    }
}
