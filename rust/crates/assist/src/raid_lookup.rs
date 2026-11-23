//! A raid encounter read from the screen, resolved in the wiki.
//!
//! The screen gives a boss name, sometimes a difficulty, the boss's armor and attack words and a
//! level (`RaidRef`). The wiki knows the raids by name, their difficulties and seasons. This joins them
//! into the `MissionBrief` the matchup tables and the squad verdict read.
//!
//! The screen is the truth for what the player sees: where it names the boss's armor, attack type or
//! level, those replace the wiki's values in the brief, and where the two differ the difference is
//! reported (`Disagreement`), because it says something about the data or about the reading.
//! Nothing is picked silently: several raids of one name, or no difficulty on the screen, show as such.
use anyhow::Result;
use baocr_extract::raid::{RaidRef, DIFFICULTIES};
use baocr_extract::scene::RaidMode;
use baocr_wikidb::{pick_season, MissionBrief, RaidCandidate, RaidDifficulty, RaidKind, SeasonMode, SeasonPick, Wiki};

const LABEL_ARMOR: &str = "ArmorType";
const LABEL_ATTACK: &str = "BulletType";

/// The kind of encounter a mode plays: 총력전 and 모의 총력전 the same bosses, 제약해제결전 the floor raids.
pub fn kind_of(mode: RaidMode) -> RaidKind {
    match mode {
        RaidMode::TotalAssault | RaidMode::MockTotalAssault => RaidKind::Raid,
        RaidMode::UnrestrictedRaid => RaidKind::MultiFloorRaid,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum RaidLookup {
    /// No raid is in play.
    None,
    /// The name matches no raid of this mode that is released on Global.
    NotFound,
    /// Several raids carry this name and nothing on the screen tells them apart.
    Ambiguous(Vec<RaidCandidate>),
    Found(Box<RaidFound>),
}

/// Where the screen and the wiki disagree about the same thing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Disagreement {
    /// "방어 타입", "공격 타입", "레벨", "난이도".
    pub what: &'static str,
    pub screen: String,
    pub wiki: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RaidFound {
    pub raid: RaidCandidate,
    /// Several raids carry the boss's name: the armor on the screen told this one from the others.
    pub picked_by_armor: bool,
    /// The wiki's `idx` of the difficulty the screen names. None: the screen names none (a boss list shows them all).
    pub difficulty: Option<i64>,
    /// Every difficulty of the raid, for the screens that show none (attack types differ between them).
    pub difficulties: Vec<RaidDifficulty>,
    /// The mission as the matchup tables read it: the screen's difficulty, else the raid's first one. The boss's
    /// armor, attack type and the level are the screen's where it names them.
    pub brief: MissionBrief,
    /// The total assault's season running now (or nearest), for its terrain.
    pub season: Option<SeasonPick>,
    pub disagreements: Vec<Disagreement>,
    /// Armor or attack words of the screen that are in no label table. The brief did not take them.
    pub unknown_words: Vec<String>,
}

fn join(words: &[String]) -> String {
    words.join(" · ")
}

/// Resolve the encounter. `now` is unix seconds.
pub fn lookup(wiki: &Wiki, raid: &RaidRef, now: i64) -> Result<RaidLookup> {
    let kind = kind_of(raid.mode);
    let candidates: Vec<RaidCandidate> = wiki.raids_by_name(&raid.boss)?.into_iter().filter(|c| c.kind == kind).collect();
    if candidates.is_empty() {
        return Ok(RaidLookup::NotFound);
    }
    let armors = wiki.decode_labels(LABEL_ARMOR, &raid.armors)?;
    let attack_words: Vec<String> = raid.attack.iter().cloned().collect();
    let attack = wiki.decode_labels(LABEL_ATTACK, &attack_words)?;
    let unknown_words: Vec<String> = armors.unknown.iter().chain(&attack.unknown).cloned().collect();

    let lead_armor = |c: &RaidCandidate| -> Result<Option<String>> { Ok(wiki.raid_difficulties(c.kind, c.id)?.into_iter().next().and_then(|d| d.boss).and_then(|b| b.armor_type)) };
    let (chosen, picked_by_armor) = match candidates.as_slice() {
        [only] => (only.clone(), false),
        several => {
            let mut matching = Vec::new();
            for c in several {
                if lead_armor(c)?.is_some_and(|a| armors.codes.contains(&a)) {
                    matching.push(c.clone());
                }
            }
            match matching.as_slice() {
                [one] => (one.clone(), true),
                _ => return Ok(RaidLookup::Ambiguous(candidates)),
            }
        }
    };

    let difficulties = wiki.raid_difficulties(chosen.kind, chosen.id)?;
    let mut disagreements = Vec::new();
    let named = raid.difficulty.map(|i| i as i64);
    let on_screen = named.filter(|i| difficulties.iter().any(|d| d.idx == *i));
    if let (Some(i), None) = (named, on_screen) {
        disagreements.push(Disagreement { what: "난이도", screen: DIFFICULTIES[i as usize].into(), wiki: "없음".into() });
    }
    let Some(idx) = on_screen.or_else(|| difficulties.first().map(|d| d.idx)) else { return Ok(RaidLookup::NotFound) };

    // only the total assault has seasons, and the same raid also has 대결전 seasons: those are no answer here
    let season = if raid.mode == RaidMode::TotalAssault {
        let mut seasons = wiki.raid_seasons(chosen.id)?;
        seasons.retain(|s| s.mode == SeasonMode::Raid);
        pick_season(&seasons, now)
    } else {
        None
    };
    let Some(brief) = wiki.raid_mission(chosen.kind, chosen.id, idx, season.as_ref().map(|p| &p.season))? else { return Ok(RaidLookup::NotFound) };
    let boss = difficulties.iter().find(|d| d.idx == idx).and_then(|d| d.boss.as_ref());

    if let Some(db) = boss.and_then(|b| b.armor_type.as_deref()).filter(|_| !armors.codes.is_empty()) {
        if armors.codes != [db] {
            disagreements.push(Disagreement { what: "방어 타입", screen: join(&raid.armors), wiki: wiki.label(LABEL_ARMOR, db) });
        }
    }
    if let (Some(code), Some(word)) = (attack.codes.first(), &raid.attack) {
        // a screen with no difficulty may show any tier's attack type
        let known: Vec<&str> = difficulties.iter().filter(|d| on_screen.is_none() || d.idx == idx).filter_map(|d| d.boss.as_ref()?.bullet_type.as_deref()).collect();
        if !known.contains(&code.as_str()) {
            let wiki_side: Vec<String> = known.iter().map(|c| wiki.label(LABEL_ATTACK, c)).collect();
            disagreements.push(Disagreement { what: "공격 타입", screen: word.clone(), wiki: wiki_side.join(" · ") });
        }
    }
    if let (Some(screen), Some(db)) = (raid.level, brief.recommended_level) {
        if i64::from(screen) != db {
            disagreements.push(Disagreement { what: "레벨", screen: screen.to_string(), wiki: db.to_string() });
        }
    }

    let mut brief = wiki.with_screen_types(brief, boss, &armors.codes, attack.codes.first().map(String::as_str))?;
    brief.recommended_level = raid.level.map(i64::from).or(brief.recommended_level);
    if on_screen.is_none() {
        // the title would name the first difficulty the brief happens to be built from
        brief.name = chosen.name.clone();
    }
    Ok(RaidLookup::Found(Box::new(RaidFound { raid: chosen, picked_by_armor, difficulty: on_screen, difficulties, brief, season, disagreements, unknown_words })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use baocr_core::ocr_api::TextBox;
    use baocr_extract::raid::{mock_boss_from_list, room_from_info, unrestricted_stage};
    use rusqlite::Connection;

    fn boxes(name: &str) -> Vec<TextBox> {
        let path = format!("{}/../../../fixtures/screens/{name}.boxes.txt", env!("CARGO_MANIFEST_DIR"));
        std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{path}: {e}"))
            .lines()
            .filter_map(|l| {
                let (pos, text) = l.split_once('\t')?;
                let v: Vec<f32> = pos.split(',').filter_map(|n| n.parse().ok()).collect();
                (v.len() == 4).then(|| TextBox { x0: v[0], y0: v[1], x1: v[2], y1: v[3], text: text.trim_end_matches('\r').to_string(), score: 1.0 })
            })
            .collect()
    }

    /// The fourteen total assaults of the database (ids 1 to 14) with the armor of their Normal lead enemy,
    /// as read from the real database. The mock list's fifteen screens (the first boss twice) are checked against it.
    const RAIDS: [(i64, &str, &str); 14] = [
        (1, "비나", "HeavyArmor"),
        (2, "헤세드", "HeavyArmor"),
        (3, "시로&쿠로", "Unarmed"),
        (4, "예로니무스", "LightArmor"),
        (5, "KAITEN FX Mk.0", "LightArmor"),
        (6, "페로로지라", "Unarmed"),
        (7, "호드", "HeavyArmor"),
        (8, "고즈", "Unarmed"),
        (9, "그레고리오", "LightArmor"),
        (10, "호버크래프트", "HeavyArmor"),
        (11, "쿠로카게", "ElasticArmor"),
        (12, "게부라", "LightArmor"),
        (13, "예소드", "ElasticArmor"),
        (14, "드럼통 게", "Unarmed"),
    ];

    /// The mock list's screens in order and the one that is known to disagree with the database.
    const DUAL_ARMOR_SCREEN: usize = 11;

    fn wiki() -> Wiki {
        let conn = Connection::open_in_memory().unwrap();
        baocr_wikidb::create(&conn).unwrap();
        let mut sql = String::from(
            "INSERT INTO source VALUES (1,'t','u','2026-11-23',NULL,'n');
             INSERT INTO label VALUES ('Terrain','Street','시가지'),('Terrain','Outdoor','야외'),('Terrain','Indoor','실내'),
               ('ArmorType','LightArmor','경장갑'),('ArmorType','HeavyArmor','중장갑'),('ArmorType','Unarmed','특수장갑'),('ArmorType','ElasticArmor','탄력장갑'),('ArmorType','Normal','일반장갑'),
               ('BulletType','Normal','일반'),('BulletType','Explosion','폭발'),('BulletType','Pierce','관통'),('BulletType','Mystic','신비');
             INSERT INTO type_effectiveness VALUES ('Explosion','LightArmor',20000),('Explosion','HeavyArmor',10000),('Explosion','Unarmed',5000),('Pierce','LightArmor',5000),('Pierce','HeavyArmor',20000),('Pierce','Unarmed',10000),
               ('Mystic','LightArmor',10000),('Mystic','HeavyArmor',5000),('Mystic','Unarmed',20000);
             INSERT INTO student VALUES (1,'a','a','s1','','',3,'Main','','','Explosion','Normal',1,1,1,'',1,1,'{}'),(2,'b','b','s2','','',3,'Main','','','Pierce','Normal',1,1,1,'',1,1,'{}'),(3,'c','c','s3','','',3,'Main','','','Mystic','Normal',1,1,1,'',1,1,'{}');",
        );
        for (id, name, armor) in RAIDS {
            let (en, hard) = (1000 + id, 2000 + id);
            // Normal attacks with the normal type, Insane (idx 5) with explosion
            sql += &format!(
                "INSERT INTO enemy VALUES ({en},'e','{name}','Boss','Main','Normal','{armor}','S',1,'{{}}'),({hard},'e','{name}','Boss','Main','Explosion','{armor}','S',1,'{{}}');
                 INSERT INTO raid (id, kind, path_name, name_ko, display_ko, dungeon_type, terrain, released_global, source_id, raw) VALUES ({id},'Raid','r{id}','{name}','{name}',NULL,'[\"Street\",\"Indoor\"]',1,1,'{{}}');
                 INSERT INTO raid_difficulty (raid_kind, raid_id, idx, label, level, floor_start, battle_duration, boss_enemy_id, released_global) VALUES ('Raid',{id},0,'Normal',NULL,NULL,180,{en},1),('Raid',{id},5,'Insane',NULL,NULL,180,{hard},1);
                 INSERT INTO raid_difficulty_enemy VALUES ('Raid',{id},0,0,{en}),('Raid',{id},5,0,{hard});"
            );
        }
        sql += "INSERT INTO enemy VALUES (3001,'s','세트의 분노','Boss','Main','Normal','Unarmed','S',1,'{}'),(3002,'s','세트의 분노','Boss','Main','Normal','LightArmor','S',1,'{}');
             INSERT INTO raid (id, kind, path_name, name_ko, display_ko, dungeon_type, terrain, released_global, source_id, raw) VALUES
               (1000000,'MultiFloorRaid','set_special','세트의 분노','세트의 분노',NULL,'[\"Outdoor\"]',1,1,'{}'),(1000001,'MultiFloorRaid','set_light','세트의 분노','세트의 분노',NULL,'[\"Outdoor\"]',1,1,'{}'),
               (8540000,'InteractiveWorldRaid','binah_854','비나','비나',NULL,'[\"Outdoor\"]',1,1,'{}');
             INSERT INTO raid_difficulty (raid_kind, raid_id, idx, label, level, floor_start, battle_duration, boss_enemy_id, released_global) VALUES
               ('MultiFloorRaid',1000000,0,NULL,NULL,1,180,3001,1),('MultiFloorRaid',1000001,0,NULL,NULL,1,180,3002,1),('InteractiveWorldRaid',8540000,0,'A',17,NULL,180,1001,1);
             INSERT INTO raid_difficulty_enemy VALUES ('MultiFloorRaid',1000000,0,0,3001),('MultiFloorRaid',1000001,0,0,3002),('InteractiveWorldRaid',8540000,0,0,1001);
             INSERT INTO raid_season (mode, season_id, season_display, raid_kind, raid_id, terrain, start_at, end_at, open_difficulty) VALUES
               ('Raid',1,'1','Raid',4,'Street',100,200,NULL),('Raid',2,'2','Raid',4,'Indoor',300,400,NULL),('Eliminate',9,'9','Raid',4,'Outdoor',500,600,NULL);";
        conn.execute_batch(&sql).unwrap();
        Wiki::from_connection(conn)
    }

    fn found(l: RaidLookup) -> RaidFound {
        match l {
            RaidLookup::Found(f) => *f,
            other => panic!("{other:?}"),
        }
    }

    fn armor_codes(f: &RaidFound) -> Vec<&str> {
        f.brief.armor.iter().map(|t| t.code.as_str()).collect()
    }

    #[test]
    fn each_boss_of_the_mock_list_is_found_and_its_armor_agrees_with_the_database_but_for_the_dual_armor() {
        let w = wiki();
        let mut disagreeing = Vec::new();
        for i in 0..=14 {
            let screen = mock_boss_from_list(&boxes(&format!("raid-mock-m{i:02}"))).unwrap();
            let f = found(lookup(&w, &screen, 0).unwrap_or_else(|e| panic!("m{i:02}: {e}")));
            assert!(f.unknown_words.is_empty(), "m{i:02}: {:?}", f.unknown_words);
            if !f.disagreements.is_empty() {
                disagreeing.push((i, f.disagreements.clone()));
            }
            assert_eq!(f.difficulty, None, "a boss list names no difficulty");
        }
        // 드럼통게 shows 특수장갑/경장갑 and the database holds only 특수장갑 for its lead enemy
        assert_eq!(disagreeing, vec![(DUAL_ARMOR_SCREEN, vec![Disagreement { what: "방어 타입", screen: "특수장갑 · 경장갑".into(), wiki: "특수장갑".into() }])]);
    }

    #[test]
    fn the_dual_armor_of_the_screen_goes_into_the_brief_and_the_attack_table_follows_it() {
        let w = wiki();
        let f = found(lookup(&w, &mock_boss_from_list(&boxes("raid-mock-m11")).unwrap(), 0).unwrap());
        assert_eq!(armor_codes(&f), ["LightArmor", "Unarmed"], "both armors, the screen being what the player sees");
        let plain = found(lookup(&w, &mock_boss_from_list(&boxes("raid-mock-m03")).unwrap(), 0).unwrap());
        assert_eq!(armor_codes(&plain), ["Unarmed"]);
        assert_eq!(plain.brief.best_attack[0].code, "Mystic");
        assert_ne!(f.brief.best_attack.iter().map(|t| t.n).collect::<Vec<_>>(), plain.brief.best_attack.iter().map(|t| t.n).collect::<Vec<_>>());
    }

    #[test]
    fn the_room_popup_gives_the_difficulty_and_the_level_and_a_boss_list_gives_neither() {
        let w = wiki();
        let room = room_from_info(&boxes("raid-mock-room-info")).unwrap();
        let f = found(lookup(&w, &room, 0).unwrap());
        assert_eq!((f.difficulty, f.brief.difficulty, f.brief.recommended_level, f.brief.name.as_str()), (Some(0), 0, Some(17), "시로&쿠로 (Normal)"));
        assert!(f.disagreements.is_empty(), "{:?}", f.disagreements);
        // Insane is the sixth row: idx 5, where the attack type switches
        let insane = found(lookup(&w, &RaidRef { difficulty: Some(5), attack: None, ..room.clone() }, 0).unwrap());
        assert_eq!((insane.difficulty, insane.brief.attack[0].code.as_str()), (Some(5), "Explosion"));
        // the same boss on the list (no difficulty): titled by the boss alone, not by the first difficulty it was built from
        let list = found(lookup(&w, &mock_boss_from_list(&boxes("raid-mock-m00")).unwrap(), 0).unwrap());
        assert_eq!((list.difficulty, list.brief.name.as_str(), list.difficulties.len()), (None, "시로&쿠로", 2));
        // a difficulty the wiki does not have is said so, and not mapped to another
        let lunatic = found(lookup(&w, &RaidRef { difficulty: Some(7), ..room }, 0).unwrap());
        assert_eq!((lunatic.difficulty, lunatic.disagreements[0].what), (None, "난이도"));
    }

    #[test]
    fn the_screens_attack_type_and_level_win_and_a_difference_is_reported() {
        let w = wiki();
        let room = room_from_info(&boxes("raid-mock-room-info")).unwrap();
        let wrong = found(lookup(&w, &RaidRef { attack: Some("신비".into()), level: Some(30), ..room }, 0).unwrap());
        assert_eq!(wrong.brief.attack[0].code, "Mystic", "the screen says mystic");
        assert_eq!(wrong.brief.recommended_level, Some(30));
        assert_eq!(wrong.disagreements, vec![Disagreement { what: "공격 타입", screen: "신비".into(), wiki: "일반".into() }]);
    }

    #[test]
    fn a_word_that_is_no_label_is_reported_and_the_brief_keeps_the_wikis_value() {
        let w = wiki();
        let mut screen = mock_boss_from_list(&boxes("raid-mock-m03")).unwrap();
        screen.armors = vec!["특수장갓".into()];
        let f = found(lookup(&w, &screen, 0).unwrap());
        assert_eq!((f.unknown_words.as_slice(), armor_codes(&f)), (["특수장갓".to_string()].as_slice(), vec!["Unarmed"]));
    }

    #[test]
    fn a_name_shared_by_several_raids_shows_as_ambiguous_unless_the_armor_tells_them_apart() {
        let w = wiki();
        let stage = unrestricted_stage(&boxes("unrestricted-raid-stage01")).unwrap();
        let f = found(lookup(&w, &stage, 0).unwrap());
        assert_eq!((f.raid.id, f.picked_by_armor), (1000001, true), "경장갑 is the light-armor 세트의 분노");
        assert_eq!(f.brief.recommended_level, Some(25));
        // the select screen shows the boss and no armor: two candidates and no pick
        let select = baocr_extract::raid::boss_from_unrestricted_select(&boxes("unrestricted-raid-select")).unwrap();
        match lookup(&w, &select, 0).unwrap() {
            RaidLookup::Ambiguous(c) => assert_eq!(c.iter().map(|c| c.id).collect::<Vec<_>>(), vec![1000000, 1000001]),
            other => panic!("{other:?}"),
        }
        // an armor that fits neither stays ambiguous
        let other = RaidRef { armors: vec!["중장갑".into()], ..stage };
        assert!(matches!(lookup(&w, &other, 0).unwrap(), RaidLookup::Ambiguous(_)));
    }

    #[test]
    fn the_kind_follows_the_mode_so_a_name_in_two_kinds_is_not_ambiguous() {
        let w = wiki();
        let binah = mock_boss_from_list(&boxes("raid-mock-m12")).unwrap();
        let f = found(lookup(&w, &binah, 0).unwrap());
        assert_eq!((f.raid.kind, f.raid.id), (RaidKind::Raid, 1), "the interactive world raid of the same name is another mode");
        let nobody = RaidRef { boss: "없는보스".into(), ..binah };
        assert_eq!(lookup(&w, &nobody, 0).unwrap(), RaidLookup::NotFound);
    }

    #[test]
    fn a_total_assault_takes_its_terrain_from_a_total_assault_season_never_a_대결전_one() {
        let w = wiki();
        let boss = RaidRef { mode: RaidMode::TotalAssault, ..mock_boss_from_list(&boxes("raid-mock-m01")).unwrap() };
        let terrain = |now| found(lookup(&w, &boss, now).unwrap()).brief.terrain.map(|t| t.code);
        assert_eq!(terrain(350), Some("Indoor".into()), "season 2 is running");
        assert_eq!(terrain(550), Some("Indoor".into()), "only a 대결전 season runs: the last total assault season is the nearest one");
        assert_eq!(found(lookup(&w, &boss, 150).unwrap()).season.map(|p| p.relation), Some(baocr_wikidb::SeasonRelation::Active));
        // a mock raid has no season: the terrain is the player's choice, unknown here
        let mock = mock_boss_from_list(&boxes("raid-mock-m01")).unwrap();
        assert_eq!(found(lookup(&w, &mock, 350).unwrap()).brief.terrain, None);
    }

    /// The same check on the real database: `BAOCR_WIKI_DB=D:/work-github/bluearchive-ocr/wikidb/wiki.db cargo test -p baocr-assist real_database -- --nocapture`.
    #[test]
    fn real_database_armor_of_every_mock_boss() {
        let Some(path) = std::env::var_os("BAOCR_WIKI_DB") else {
            eprintln!("SKIPPED real_database_armor_of_every_mock_boss: BAOCR_WIKI_DB is not set");
            return;
        };
        let w = Wiki::open(std::path::Path::new(&path)).unwrap();
        let mut mismatches = Vec::new();
        for i in 0..=14 {
            let screen = mock_boss_from_list(&boxes(&format!("raid-mock-m{i:02}"))).unwrap();
            let f = found(lookup(&w, &screen, 0).unwrap());
            eprintln!("m{i:02} {} {:?} -> raid {} {:?} {:?}", screen.boss, screen.armors, f.raid.id, f.raid.name, f.disagreements);
            assert!(f.unknown_words.is_empty(), "m{i:02}");
            mismatches.extend(f.disagreements.iter().map(|d| (i, d.clone())));
        }
        assert_eq!(mismatches.iter().map(|(i, d)| (*i, d.what)).collect::<Vec<_>>(), vec![(DUAL_ARMOR_SCREEN, "방어 타입")], "{mismatches:?}");
    }
}
