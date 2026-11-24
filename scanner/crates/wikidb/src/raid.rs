//! Raid lookup: total assault (총력전), 종합전술시험, world raids and the like. Read side only;
//! the collector (`schaledb`) writes `raid`, `raid_difficulty`, `raid_difficulty_enemy` and
//! `raid_season`. The functions answer what a screen can name: a boss name, a difficulty and a
//! moment in time. Anything that can be ambiguous says so in its return type.
use crate::wiki::{norm, MissionBrief, Tally, Wiki};
use anyhow::{anyhow, Result};
use rusqlite::params;

/// The kinds of encounter in the source, in the order candidates are listed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RaidKind {
    Raid,
    TimeAttack,
    WorldRaid,
    MultiFloorRaid,
    InteractiveWorldRaid,
}

impl RaidKind {
    pub fn as_str(self) -> &'static str {
        match self {
            RaidKind::Raid => "Raid",
            RaidKind::TimeAttack => "TimeAttack",
            RaidKind::WorldRaid => "WorldRaid",
            RaidKind::MultiFloorRaid => "MultiFloorRaid",
            RaidKind::InteractiveWorldRaid => "InteractiveWorldRaid",
        }
    }

    fn parse(code: &str) -> Result<RaidKind> {
        [RaidKind::Raid, RaidKind::TimeAttack, RaidKind::WorldRaid, RaidKind::MultiFloorRaid, RaidKind::InteractiveWorldRaid]
            .into_iter()
            .find(|k| k.as_str() == code)
            .ok_or_else(|| anyhow!("unknown raid kind {code}"))
    }
}

/// A raid a name can stand for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RaidCandidate {
    pub kind: RaidKind,
    pub id: i64,
    /// What the game calls it; a time attack, which the source leaves unnamed, is "사격 시험 · 야외 · 경장갑".
    pub name: String,
    /// The whole name matched. False when only the start of the name did (OCR cut it off).
    pub exact: bool,
    /// Terrains the raid can be played on; the season says which one it is.
    pub terrains: Vec<String>,
}

/// The lead enemy of a difficulty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RaidBoss {
    pub name: String,
    pub bullet_type: Option<String>,
    pub armor_type: Option<String>,
}

/// One difficulty of a raid. `idx` is its position in the source (the key for `raid_mission`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RaidDifficulty {
    pub idx: i64,
    /// The source's name for it (Normal .. Lunatic, A .. D); none for time attacks and floor raids.
    pub label: Option<String>,
    pub level: Option<i64>,
    pub floor_start: Option<i64>,
    pub battle_duration: Option<i64>,
    pub released_global: bool,
    pub boss: Option<RaidBoss>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeasonMode {
    /// 총력전.
    Raid,
    /// 대결전.
    Eliminate,
}

/// A Global season of a raid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RaidSeason {
    pub mode: SeasonMode,
    pub season_id: i64,
    /// The number the game shows, or "BETA".
    pub display: Option<String>,
    pub raid_id: i64,
    pub terrain: String,
    /// Unix seconds; the season runs from `start_at` up to, not including, `end_at`.
    pub start_at: i64,
    pub end_at: i64,
    /// Eliminate seasons: the highest open difficulty per armor type, as the source's JSON.
    pub open_difficulty: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeasonRelation {
    Active,
    /// Starts after `now`.
    Upcoming,
    /// Ended before `now`.
    Past,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeasonPick {
    pub season: RaidSeason,
    pub relation: SeasonRelation,
}

/// The season running at `now`; else the one closest to it in time (a tie goes to the one that
/// ended, which is the one a player has just seen). Several running at once: the latest start.
pub fn pick_season(seasons: &[RaidSeason], now: i64) -> Option<SeasonPick> {
    let active = seasons.iter().filter(|s| s.start_at <= now && now < s.end_at).max_by_key(|s| s.start_at);
    if let Some(season) = active {
        return Some(SeasonPick { season: season.clone(), relation: SeasonRelation::Active });
    }
    // distance in seconds, and whether it is behind us (past sorts before upcoming on a tie)
    let nearest = seasons.iter().map(|s| if s.start_at > now { (s.start_at - now, 1, s) } else { (now - s.end_at, 0, s) }).min_by_key(|(d, side, s)| (*d, *side, s.start_at))?;
    let relation = if nearest.1 == 1 { SeasonRelation::Upcoming } else { SeasonRelation::Past };
    Some(SeasonPick { season: nearest.2.clone(), relation })
}

impl Wiki {
    /// Global-released raids whose name is `name` (spaces and punctuation ignored). When none
    /// has exactly that name, those whose name starts with it. The same name in several kinds
    /// ("비나" is a total assault and an interactive world raid) gives several candidates,
    /// ordered by kind then id; the caller decides, nothing is picked here.
    pub fn raids_by_name(&self, name: &str) -> Result<Vec<RaidCandidate>> {
        let want = norm(name);
        if want.chars().count() < 2 {
            return Ok(Vec::new());
        }
        let mut st = self.conn.prepare("SELECT kind, id, display_ko, terrain FROM raid WHERE released_global = 1")?;
        let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, String>(2)?, r.get::<_, Option<String>>(3)?)))?;
        let mut all = Vec::new();
        for row in rows {
            let (kind, id, display, terrain) = row?;
            let have = norm(&display);
            let exact = have == want;
            if !exact && !have.starts_with(&want) {
                continue;
            }
            let terrains: Vec<String> = terrain.map_or_else(|| Ok(Vec::new()), |t| serde_json::from_str(&t))?;
            all.push(RaidCandidate { kind: RaidKind::parse(&kind)?, id, name: display, exact, terrains });
        }
        if all.iter().any(|c| c.exact) {
            all.retain(|c| c.exact);
        }
        all.sort_by_key(|c| (c.kind, c.id));
        Ok(all)
    }

    /// Every difficulty of a raid, ordered by `idx`, closed ones included (`released_global`).
    pub fn raid_difficulties(&self, kind: RaidKind, id: i64) -> Result<Vec<RaidDifficulty>> {
        let mut st = self.conn.prepare(
            "SELECT idx, label, level, floor_start, battle_duration, released_global, boss_name_ko, boss_bullet_type, boss_armor_type
             FROM v_raid_difficulty WHERE raid_kind = ?1 AND raid_id = ?2 ORDER BY idx",
        )?;
        let rows = st.query_map(params![kind.as_str(), id], |r| {
            let boss_name: Option<String> = r.get(6)?;
            Ok(RaidDifficulty {
                idx: r.get(0)?,
                label: r.get(1)?,
                level: r.get(2)?,
                floor_start: r.get(3)?,
                battle_duration: r.get(4)?,
                released_global: r.get(5)?,
                boss: boss_name.map(|name| RaidBoss { name, bullet_type: r.get(7).ok().flatten(), armor_type: r.get(8).ok().flatten() }),
            })
        })?;
        rows.collect::<std::result::Result<_, _>>().map_err(Into::into)
    }

    /// The Global seasons of a total assault, oldest first.
    pub fn raid_seasons(&self, raid_id: i64) -> Result<Vec<RaidSeason>> {
        let mut st = self.conn.prepare("SELECT mode, season_id, season_display, raid_id, terrain, start_at, end_at, open_difficulty FROM raid_season WHERE raid_id = ?1 ORDER BY start_at, mode")?;
        let rows = st.query_map(params![raid_id], |r| {
            let mode: String = r.get(0)?;
            Ok(RaidSeason {
                mode: if mode == "Eliminate" { SeasonMode::Eliminate } else { SeasonMode::Raid },
                season_id: r.get(1)?,
                display: r.get(2)?,
                raid_id: r.get(3)?,
                terrain: r.get(4)?,
                start_at: r.get(5)?,
                end_at: r.get(6)?,
                open_difficulty: r.get(7)?,
            })
        })?;
        rows.collect::<std::result::Result<_, _>>().map_err(Into::into)
    }

    /// The season of `raid_id` running at `now` (unix seconds), else the nearest one in time:
    /// see `pick_season`. `relation` tells which. None when the raid has no season at all
    /// (every kind but the total assault).
    pub fn current_or_nearest_season(&self, raid_id: i64, now: i64) -> Result<Option<SeasonPick>> {
        Ok(pick_season(&self.raid_seasons(raid_id)?, now))
    }

    /// One difficulty of a raid as the mission shape the formation verdict reads. `None` when
    /// there is no such difficulty. Armor and attack types are the DB's shares of the encounter's
    /// enemy weight (`v_raid_armor_share`, `v_raid_attack_share`), never what a screen shows. The terrain is
    /// the season's; without a season it is the raid's only terrain, or unknown when it can be played on several.
    /// `stage_id` holds the raid id (not a stage) and `difficulty` the difficulty `idx`.
    pub fn raid_mission(&self, kind: RaidKind, id: i64, idx: i64, season: Option<&RaidSeason>) -> Result<Option<MissionBrief>> {
        let Some(difficulty) = self.raid_difficulties(kind, id)?.into_iter().find(|d| d.idx == idx) else { return Ok(None) };
        let (display, terrains): (String, Option<String>) = self.conn.query_row("SELECT display_ko, terrain FROM raid WHERE kind = ?1 AND id = ?2", params![kind.as_str(), id], |r| Ok((r.get(0)?, r.get(1)?)))?;
        let terrains: Vec<String> = terrains.map_or_else(|| Ok(Vec::new()), |t| serde_json::from_str(&t))?;
        let terrain = season.map(|s| s.terrain.clone()).or_else(|| (terrains.len() == 1).then(|| terrains[0].clone()));
        let key: [&dyn rusqlite::ToSql; 3] = [&kind.as_str(), &id, &idx];
        let filter = "raid_kind = ?1 AND raid_id = ?2 AND idx = ?3";
        let armor = self.read_shares("v_raid_armor_share", "armor_type", "ArmorType", filter, &key)?;
        let attack = self.read_shares("v_raid_attack_share", "bullet_type", "BulletType", filter, &key)?;
        // the level the encounter gives its lead enemy (the source's own list, else SchaleDB's default for the difficulty)
        let lead_level: Option<i64> = self
            .conn
            .query_row(
                "SELECT level FROM v_raid_enemy WHERE raid_kind = ?1 AND raid_id = ?2 AND idx = ?3 AND rank IN ('Boss', 'Champion') ORDER BY rank = 'Boss' DESC, ordinal LIMIT 1",
                params![kind.as_str(), id, idx],
                |r| r.get(0),
            )
            .ok()
            .flatten();
        let title = match (&difficulty.label, difficulty.level, difficulty.floor_start) {
            (Some(label), _, _) => format!("{display} ({label})"),
            (None, _, Some(floor)) => format!("{display} ({floor}층~)"),
            (None, Some(level), None) => format!("{display} (Lv.{level})"),
            (None, None, None) => display,
        };
        Ok(Some(MissionBrief {
            stage_id: id,
            name: title,
            area: None,
            stage_no: None,
            difficulty: idx,
            terrain: terrain.map(|code| Tally { ko: self.label("Terrain", &code), code, n: 0 }),
            recommended_level: difficulty.level.or(lead_level),
            best_attack: self.best_attack_against(&armor)?,
            attack,
            armor,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn season(id: i64, start_at: i64, end_at: i64) -> RaidSeason {
        RaidSeason { mode: SeasonMode::Raid, season_id: id, display: None, raid_id: 1, terrain: "Street".into(), start_at, end_at, open_difficulty: None }
    }

    #[test]
    fn the_active_season_wins_and_otherwise_the_nearest_in_time() {
        let all = [season(1, 100, 200), season(2, 300, 400), season(3, 1000, 1100)];
        let pick = |now| pick_season(&all, now).map(|p| (p.season.season_id, p.relation));
        assert_eq!(pick(150), Some((1, SeasonRelation::Active)));
        assert_eq!(pick(100), Some((1, SeasonRelation::Active)), "a season includes its start");
        assert_eq!(pick(400), Some((2, SeasonRelation::Past)), "and excludes its end: 100 s past season 2, 600 s before season 3");
        assert_eq!(pick(210), Some((1, SeasonRelation::Past)));
        assert_eq!(pick(280), Some((2, SeasonRelation::Upcoming)), "20 s before season 2 against 80 s after season 1");
        assert_eq!(pick(250), Some((1, SeasonRelation::Past)), "equally far from both: the one that ended");
        assert_eq!(pick(5), Some((1, SeasonRelation::Upcoming)));
        assert_eq!(pick(5000), Some((3, SeasonRelation::Past)));
        assert_eq!(pick_season(&[], 0), None);
    }

    fn fixture() -> Wiki {
        let conn = Connection::open_in_memory().unwrap();
        crate::create(&conn).unwrap();
        conn.execute_batch(
            "INSERT INTO source VALUES (1,'t','u','2026-11-23',NULL,'n');
             INSERT INTO label VALUES ('Terrain','Street','시가지'),('Terrain','Outdoor','야외'),('ArmorType','HeavyArmor','중장갑'),('ArmorType','LightArmor','경장갑'),
                    ('BulletType','Normal','일반'),('BulletType','Pierce','관통'),('BulletType','Explosion','폭발');
             INSERT INTO type_effectiveness VALUES ('Pierce','HeavyArmor',20000),('Pierce','LightArmor',5000),('Explosion','HeavyArmor',10000),('Explosion','LightArmor',20000);
             INSERT INTO student VALUES (1,'a','a','s1','','',3,'Main','','','Explosion','Normal',1,1,1,'',1,1,'{}'),(2,'b','b','s2','','',3,'Main','','','Pierce','Normal',1,1,1,'',1,1,'{}');
             INSERT INTO enemy VALUES (10,'b','비나','Boss','Main','Normal','HeavyArmor','S',1,'{}'),(11,'b','비나','Boss','Main','Pierce','HeavyArmor','S',1,'{}'),
                    (12,'m','수송선','Student','Main','Normal','Normal','S',1,'{}'),(13,'s','소환물','Summoned','Main','Normal','LightArmor','S',1,'{}'),
                    (14,'t','졸개','Minion','Main','Pierce','LightArmor','S',1,'{}');
             INSERT INTO raid (id, kind, path_name, name_ko, display_ko, dungeon_type, terrain, released_global, source_id, raw) VALUES
                    (1,'Raid','binah','비나','비나',NULL,'[\"Outdoor\",\"Street\"]',1,1,'{}'),
                    (8540000,'InteractiveWorldRaid','binah_854','비나','비나',NULL,'[\"Outdoor\"]',1,1,'{}'),
                    (821001,'WorldRaid','binah_colour','비나 (색채)','비나 (색채)',NULL,'[\"Outdoor\"]',1,1,'{}'),
                    (5,'Raid','nightmare','악몽 열차','악몽 열차',NULL,'[\"Indoor\"]',0,1,'{}'),
                    (1,'TimeAttack',NULL,NULL,'사격 시험 · 야외 · 경장갑','Shooting','[\"Outdoor\"]',1,1,'{}');
             INSERT INTO raid_difficulty (raid_kind, raid_id, idx, label, level, floor_start, battle_duration, boss_enemy_id, released_global) VALUES
                    ('Raid',1,0,'Normal',NULL,NULL,180,10,1),('Raid',1,5,'Insane',NULL,NULL,180,11,1),
                    ('InteractiveWorldRaid',8540000,0,'A',17,NULL,180,10,1),
                    ('TimeAttack',1,0,NULL,25,NULL,180,NULL,1);
             INSERT INTO raid_difficulty_enemy VALUES ('Raid',1,0,0,10),('Raid',1,5,0,11),('Raid',1,5,1,13),('Raid',1,5,2,14),('Raid',1,5,3,14),
                    ('InteractiveWorldRaid',8540000,0,0,10),('InteractiveWorldRaid',8540000,0,1,12),('TimeAttack',1,0,0,14);
             INSERT INTO raid_difficulty_enemy_weight VALUES ('Raid',1,0,0,1,1,100,100,NULL),('Raid',1,5,0,1,1,100,100,NULL),('Raid',1,5,1,1,1,100,0,'rank'),('Raid',1,5,2,1,1,100,100,NULL),('Raid',1,5,3,1,1,100,100,NULL),
                    ('InteractiveWorldRaid',8540000,0,0,17,1,100,100,NULL),('InteractiveWorldRaid',8540000,0,1,17,1,100,0,'rank'),('TimeAttack',1,0,0,25,1,100,100,NULL);
             INSERT INTO raid_season (mode, season_id, season_display, raid_kind, raid_id, terrain, start_at, end_at, open_difficulty) VALUES
                    ('Raid',3,'1','Raid',1,'Outdoor',100,200,NULL),('Raid',9,'7','Raid',1,'Street',300,400,NULL),('Eliminate',2,'BETA','Raid',1,'Street',500,600,'{\"LightArmor\":5}');",
        )
        .unwrap();
        Wiki::from_connection(conn)
    }

    #[test]
    fn one_name_in_several_kinds_gives_every_candidate_not_the_first() {
        let w = fixture();
        let found = w.raids_by_name("비 나").unwrap();
        assert_eq!(found.iter().map(|c| (c.kind, c.id, c.exact)).collect::<Vec<_>>(), vec![(RaidKind::Raid, 1, true), (RaidKind::InteractiveWorldRaid, 8540000, true)], "the colour raid only starts with the name: left out while exact ones exist");
        assert_eq!(found[0].terrains, ["Outdoor", "Street"]);
    }

    #[test]
    fn a_cut_off_name_finds_raids_by_prefix_and_unreleased_raids_are_hidden() {
        let w = fixture();
        assert_eq!(w.raids_by_name("비나(색").unwrap().len(), 1, "prefix when nothing matches exactly");
        assert!(w.raids_by_name("악몽 열차").unwrap().is_empty(), "not released on Global");
        assert!(w.raids_by_name("비").unwrap().is_empty(), "one letter is no name");
        assert!(w.raids_by_name("").unwrap().is_empty());
        let ta = w.raids_by_name("사격 시험").unwrap();
        assert_eq!((ta.len(), ta[0].kind, ta[0].exact), (1, RaidKind::TimeAttack, false), "a time attack is found by its test");
    }

    #[test]
    fn difficulties_carry_the_lead_enemys_attack_type() {
        let w = fixture();
        let d = w.raid_difficulties(RaidKind::Raid, 1).unwrap();
        assert_eq!(d.iter().map(|d| (d.idx, d.label.as_deref(), d.boss.as_ref().and_then(|b| b.bullet_type.as_deref()))).collect::<Vec<_>>(), vec![(0, Some("Normal"), Some("Normal")), (5, Some("Insane"), Some("Pierce"))]);
        assert!(w.raid_difficulties(RaidKind::Raid, 99).unwrap().is_empty());
        let ta = w.raid_difficulties(RaidKind::TimeAttack, 1).unwrap();
        assert_eq!((ta[0].level, ta[0].boss.is_none()), (Some(25), true), "no lead enemy: a defense test");
    }

    #[test]
    fn seasons_come_with_the_terrain_the_raid_is_played_on() {
        let w = fixture();
        let now = |t| w.current_or_nearest_season(1, t).unwrap().map(|p| (p.season.season_id, p.season.terrain, p.relation));
        assert_eq!(now(350), Some((9, "Street".into(), SeasonRelation::Active)));
        assert_eq!(now(150), Some((3, "Outdoor".into(), SeasonRelation::Active)));
        assert_eq!(now(250), Some((3, "Outdoor".into(), SeasonRelation::Past)), "250 is 50 s after season 3 and 50 s before season 9: the one that ended");
        assert_eq!(w.raid_seasons(1).unwrap().iter().map(|s| (s.mode, s.season_id)).collect::<Vec<_>>(), vec![(SeasonMode::Raid, 3), (SeasonMode::Raid, 9), (SeasonMode::Eliminate, 2)]);
        assert_eq!(w.current_or_nearest_season(8540000, 0).unwrap(), None, "only total assaults have seasons");
    }

    #[test]
    fn a_difficulty_becomes_a_mission_the_verdict_can_use() {
        let w = fixture();
        let s = w.raid_seasons(1).unwrap().into_iter().find(|s| s.season_id == 9).unwrap();
        let m = w.raid_mission(RaidKind::Raid, 1, 5, Some(&s)).unwrap().unwrap();
        assert_eq!((m.name.as_str(), m.difficulty, m.terrain.as_ref().map(|t| t.ko.as_str())), ("비나 (Insane)", 5, Some("시가지")), "the season's terrain");
        // summons are left out: the boss and two minions remain, all of the same weight
        let mix = |v: &[crate::wiki::Share]| v.iter().map(|t| (t.code.clone(), t.heads, (t.share * 100.0).round() as i64)).collect::<Vec<_>>();
        assert_eq!(mix(&m.attack), vec![("Pierce".to_string(), 3, 100)]);
        assert_eq!(mix(&m.armor), vec![("LightArmor".to_string(), 2, 67), ("HeavyArmor".to_string(), 1, 33)]);
        // 2 light + 1 heavy: explosion (200+200+100)/3 = 167 %, pierce (50+50+200)/3 = 100 %
        assert_eq!(m.best_attack.iter().map(|t| (t.code.as_str(), t.n)).collect::<Vec<_>>(), vec![("Explosion", 167), ("Pierce", 100)]);
        let normal = w.raid_mission(RaidKind::Raid, 1, 0, None).unwrap().unwrap();
        assert_eq!(normal.terrain, None, "two possible terrains and no season: unknown");
        let students = w.raid_mission(RaidKind::InteractiveWorldRaid, 8540000, 0, None).unwrap().unwrap();
        assert_eq!(students.terrain.map(|t| t.code), Some("Outdoor".into()), "a single possible terrain");
        assert_eq!(students.armor.iter().map(|t| t.heads).sum::<i64>(), 1, "the escorted transport is not an enemy");
        assert_eq!(w.raid_mission(RaidKind::TimeAttack, 1, 0, None).unwrap().unwrap().name, "사격 시험 · 야외 · 경장갑 (Lv.25)");
        assert!(w.raid_mission(RaidKind::Raid, 1, 3, None).unwrap().is_none(), "no such difficulty");
    }

    #[test]
    fn the_raid_mission_feeds_the_formation_verdict() {
        use crate::formation::{evaluate, Verdict};
        let w = fixture();
        let m = w.raid_mission(RaidKind::Raid, 1, 5, None).unwrap().unwrap();
        let pool = w.students().unwrap();
        let types = w.type_table().unwrap();
        // the boss squad is light armor: explosion is the right pick, pierce the wrong one
        let score = |id: i64| evaluate(&m, None, &pool.iter().filter(|s| s.id == id).cloned().collect::<Vec<_>>(), &pool, &types).unwrap().verdict;
        assert_eq!((score(1), score(2)), (Verdict::Favorable, Verdict::Unfavorable));
    }
}
