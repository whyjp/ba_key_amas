//! The widgets the assistant offers.
use crate::matchup::{all_attack_types_by_edge, attack_types_by_edge, defense_matrix, fit_entry, grade_of, legend, offense_matrix, striker_armors};
use crate::widget::{is_mission_screen, Context, Kind, Lookup, Output, Widget, WidgetId};
use baocr_extract::scene::Scene;
use baocr_extract::CardName;
use baocr_overlay::{Chip, Frame, Icon, Mark, Row, Tone};
use baocr_wikidb::{evaluate, rate, FormationScore, MissionBrief, Rating, StudentBrief, Tally, TypeTable, Verdict, Wiki};

fn chips(list: &[Tally], suffix: &str) -> Vec<Chip> {
    list.iter().map(|t| Chip { text: format!("{} {}{suffix}", t.ko, t.n), tone: Tone::from_code(&t.code) }).collect()
}

fn on_formation_screen(scene: Scene) -> bool {
    matches!(scene, Scene::Squad | Scene::QuickFormation)
}

/// The mission's name, terrain, level and what its enemies are.
pub struct MissionInfo;

impl Widget for MissionInfo {
    fn id(&self) -> WidgetId {
        WidgetId::MissionInfo
    }
    fn title(&self) -> &'static str {
        "임무 카드"
    }
    fn kind(&self) -> Kind {
        Kind::Card
    }
    fn render(&self, cx: &Context) -> Option<Output> {
        let mission = cx.obs.mission.as_ref().filter(|_| is_mission_screen(cx.obs.scene))?;
        let mut rows = Vec::new();
        match &cx.lookup {
            Lookup::Found { brief, alternatives } => {
                mission_rows(&mut rows, brief, cx.types, !on_formation_screen(cx.obs.scene));
                if *alternatives > 1 {
                    rows.push(Row::Note("Normal 기준입니다. Hard는 적 레벨이 다릅니다".into()));
                }
            }
            Lookup::PlaceUnknown => rows.push(Row::Note("장소를 알 수 없습니다. 스테이지 목록 화면부터 열어 주세요".into())),
            Lookup::NotFound => rows.push(Row::Note("위키DB에서 이 임무를 찾지 못했습니다".into())),
            Lookup::None => {}
        }
        Some(Output::Card { title: mission.display(), rows })
    }
}

/// The mission's facts. `with_table`: which attack type hits these enemies best, as a table
/// (the squad screens show that in the formation card instead, for the squad's own types).
fn mission_rows(rows: &mut Vec<Row>, b: &MissionBrief, types: &TypeTable, with_table: bool) {
    if b.area.is_some() {
        rows.push(Row::Info { label: "난이도".into(), value: if b.difficulty == 0 { "Normal" } else { "Hard" }.into() });
    }
    if let Some(t) = &b.terrain {
        rows.push(Row::Info { label: "지형".into(), value: t.ko.clone() });
    }
    if let Some(l) = b.recommended_level {
        rows.push(Row::Info { label: "추천 Lv".into(), value: l.to_string() });
    }
    rows.push(Row::Chips { label: "적 공격".into(), chips: chips(&b.attack, "") });
    let table = with_table.then(|| offense_matrix(b, &all_attack_types_by_edge(b, types), types)).flatten();
    match table {
        Some(table) => rows.extend([table, legend()]),
        None => rows.push(Row::Chips { label: "적 방어".into(), chips: chips(&b.armor, "") }),
    }
}

/// The verdict, who in the squad suits the battle, and the tables behind it.
pub fn formation_rows(score: &FormationScore, brief: &MissionBrief, terrain: Option<&str>, team: &[StudentBrief], types: &TypeTable) -> Vec<Row> {
    let (text, tone) = match score.verdict {
        Verdict::Favorable => ("유리", Tone::Good),
        Verdict::Average => ("평균", Tone::Even),
        Verdict::Unfavorable => ("불리", Tone::Bad),
    };
    let entries = team.iter().map(|s| fit_entry(s, rate(s, brief, terrain, types))).collect();
    let rated: Vec<&StudentBrief> = team.iter().filter(|s| baocr_wikidb::rates_by_type(s)).collect();
    let attacks = attack_types_by_edge(brief, rated.iter().map(|s| s.bullet_type.as_str()), types);
    let mut rows = vec![Row::Verdict { label: "편성 판정".into(), text: text.into(), tone }, Row::Fits(entries)];
    rows.extend(offense_matrix(brief, &attacks, types));
    rows.extend(defense_matrix(brief, &striker_armors(team), types));
    rows.push(legend());
    let skipped = if score.unrated > 0 { format!(", 스페셜 {}명은 스킬 역할로 판단하므로 제외", score.unrated) } else { String::new() };
    rows.push(Row::Note(format!("인식한 학생 {}명 기준{skipped}. 상성은 필요조건일 뿐 스킬·코스트·육성도 함께 보세요", score.members)));
    rows
}

/// Fewest recognised students before a verdict is given.
const MIN_SQUAD_FOR_VERDICT: usize = 3;

/// Whether the squad suits the mission: a verdict and who fits.
pub struct Formation;

impl Widget for Formation {
    fn id(&self) -> WidgetId {
        WidgetId::Formation
    }
    fn title(&self) -> &'static str {
        "편성 판정"
    }
    fn kind(&self) -> Kind {
        Kind::Card
    }
    fn render(&self, cx: &Context) -> Option<Output> {
        if !on_formation_screen(cx.obs.scene) {
            return None;
        }
        let brief = cx.brief()?;
        let team = cx.team();
        // one or two students read so far (the rest scrolled out of view, or the 편성 screen missed)
        // would give a verdict about a squad that is not the player's
        if team.len() < MIN_SQUAD_FOR_VERDICT {
            return None;
        }
        let score = evaluate(brief, cx.terrain(), &team, cx.students, cx.types)?;
        Some(Output::Card { title: String::new(), rows: formation_rows(&score, brief, cx.terrain(), &team, cx.types) })
    }
}

/// Icons stand in a column on a portrait's right edge, half on the portrait and half off
/// it, so they stay off the face. The column starts this far above the name label's top,
/// which is just under the squad-number badge in the portrait's top right corner.
const COLUMN_TOP_ABOVE_NAME: i32 = 68;
/// Distance from a card's centre to its portrait's right edge (frame pixels).
const CARD_HALF_WIDTH: i32 = 52;
/// A grid card's outline relative to its name label: (left, top, right, bottom) around its centre
/// and above/below the label's top.
const CARD_FRAME: (i32, i32, i32, i32) = (-55, -106, 55, 41);
/// The grid's viewport: a card whose portrait is mostly scrolled out of it gets no icons.
const GRID_ICONS_TOP: i32 = 322;
const GRID_ICONS_BOTTOM: i32 = 600;

/// What goes on one student's portrait: how well it suits the battle and the terrain face,
/// or just "SP" for a special that is not rated. Without a known mission nothing can be
/// rated, and only the terrain face (when the terrain is known) remains.
fn marks_of(s: &StudentBrief, brief: Option<&MissionBrief>, terrain: Option<&str>, types: &TypeTable) -> (Vec<Icon>, Option<baocr_overlay::Grade>) {
    let face = terrain.and_then(|t| s.adaptation(t)).map(Icon::Face);
    match brief.map(|b| rate(s, b, terrain, types)) {
        Some(Rating::Rated(fit)) => {
            let grade = grade_of(fit);
            (std::iter::once(Icon::Fit(grade)).chain(face).collect(), Some(grade))
        }
        Some(Rating::NotRated(_)) => (vec![Icon::Special], None),
        None => (face.into_iter().collect(), None),
    }
}

/// Marks for the student cards on screen.
pub fn card_marks(cards: &[CardName], students: &[StudentBrief], brief: Option<&MissionBrief>, terrain: Option<&str>, types: &TypeTable, kind: Option<&str>) -> Vec<Mark> {
    cards
        .iter()
        .filter_map(|c| {
            let top = c.y0.round() as i32 - COLUMN_TOP_ABOVE_NAME;
            if !(GRID_ICONS_TOP..=GRID_ICONS_BOTTOM).contains(&top) {
                return None;
            }
            let s = Wiki::match_student_as(students, &c.text, kind)?;
            let (icons, grade) = marks_of(&s, brief, terrain, types);
            if icons.is_empty() {
                return None;
            }
            let (cx, y0) = (c.x.round() as i32, c.y0.round() as i32);
            let mark = Mark::column_from_top(cx + CARD_HALF_WIDTH, top, icons);
            Some(match grade {
                Some(grade) => mark.framed(Frame { left: cx + CARD_FRAME.0, top: y0 + CARD_FRAME.1, right: cx + CARD_FRAME.2, bottom: y0 + CARD_FRAME.3, grade }),
                None => mark,
            })
        })
        .collect()
}

/// Marks over the portraits in the grid of the 빠른 편성 popup.
pub struct PortraitIcons;

impl Widget for PortraitIcons {
    fn id(&self) -> WidgetId {
        WidgetId::PortraitIcons
    }
    fn title(&self) -> &'static str {
        "초상화 아이콘"
    }
    fn kind(&self) -> Kind {
        Kind::Marks
    }
    fn render(&self, cx: &Context) -> Option<Output> {
        (cx.obs.scene == Scene::QuickFormation).then(|| Output::Marks(card_marks(&cx.obs.cards, cx.students, cx.brief(), cx.terrain(), cx.types, cx.obs.grid_kind)))
    }
}

/// Right edges of the six slots of the squad strip at the bottom of the 빠른 편성 popup:
/// four strikers, then two specials (frame pixels). The icon columns straddle these edges.
const SLOT_RIGHT_EDGES: [i32; 6] = [111, 205, 300, 392, 490, 579];
/// Top of the column: below the leader badge of the first slot.
const SLOT_COLUMN_TOP: i32 = 700;
/// A slot's outline: this far left of its right edge, between these rows, this far right of it.
const SLOT_FRAME: (i32, i32, i32, i32) = (-89, 676, 2, 760);

/// Marks for a squad: the strikers fill the first four slots, the specials the last two.
pub fn slot_marks(team: &[StudentBrief], brief: Option<&MissionBrief>, terrain: Option<&str>, types: &TypeTable) -> Vec<Mark> {
    let strikers = team.iter().filter(|s| s.squad_type == "Main").take(4).enumerate().map(|(i, s)| (i, s));
    let specials = team.iter().filter(|s| s.squad_type != "Main").take(2).enumerate().map(|(i, s)| (4 + i, s));
    strikers
        .chain(specials)
        .filter_map(|(slot, s)| {
            let (icons, grade) = marks_of(s, brief, terrain, types);
            if icons.is_empty() {
                return None;
            }
            let edge = SLOT_RIGHT_EDGES[slot];
            let mark = Mark::column_from_top(edge, SLOT_COLUMN_TOP, icons);
            Some(match grade {
                Some(grade) => mark.framed(Frame { left: edge + SLOT_FRAME.0, top: SLOT_FRAME.1, right: edge + SLOT_FRAME.2, bottom: SLOT_FRAME.3, grade }),
                None => mark,
            })
        })
        .collect()
}

/// Marks over the squad strip at the bottom of the 빠른 편성 popup.
pub struct SlotStats;

impl Widget for SlotStats {
    fn id(&self) -> WidgetId {
        WidgetId::SlotStats
    }
    fn title(&self) -> &'static str {
        "슬롯 스탯"
    }
    fn kind(&self) -> Kind {
        Kind::Marks
    }
    fn render(&self, cx: &Context) -> Option<Output> {
        (cx.obs.scene == Scene::QuickFormation).then(|| Output::Marks(slot_marks(&cx.team(), cx.brief(), cx.terrain(), cx.types)))
    }
}

/// All widgets, in the order their cards stack.
pub fn all() -> Vec<Box<dyn Widget>> {
    vec![Box::new(MissionInfo), Box::new(Formation), Box::new(PortraitIcons), Box::new(SlotStats)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use baocr_overlay::{ingame_panel_width, load_fonts, render_panel, Grade, PanelState};
    use image::RgbaImage;

    fn student(id: i64, name: &str, bullet: &str, armor: &str, outdoor: u8, squad: &str, role: &str) -> StudentBrief {
        StudentBrief { id, name: name.into(), bullet_type: bullet.into(), armor_type: armor.into(), street: 0, outdoor, indoor: 0, squad_type: squad.into(), role: role.into() }
    }

    /// Mostly heavy armor, shot at with mystic and explosion.
    fn brief() -> MissionBrief {
        let tally = |code: &str, ko: &str, n: i64| Tally { code: code.into(), ko: ko.into(), n };
        MissionBrief {
            stage_id: 1,
            name: "19-3".into(),
            area: Some(19),
            stage_no: Some(3),
            difficulty: 0,
            terrain: Some(tally("Outdoor", "야외", 0)),
            recommended_level: Some(85),
            attack: vec![tally("Mystic", "신비", 21), tally("Explosion", "폭발", 10)],
            armor: vec![tally("HeavyArmor", "중장갑", 20), tally("Unarmed", "특수장갑", 6), tally("LightArmor", "경장갑", 3)],
            best_attack: vec![],
        }
    }

    fn types() -> TypeTable {
        let t = |b: &str, rows: [(&str, f64); 3]| rows.map(|(a, m)| (b.to_string(), a.to_string(), m));
        TypeTable::new(
            [
                t("Explosion", [("LightArmor", 2.0), ("HeavyArmor", 1.0), ("Unarmed", 0.5)]),
                t("Pierce", [("LightArmor", 0.5), ("HeavyArmor", 2.0), ("Unarmed", 1.0)]),
                t("Mystic", [("LightArmor", 1.0), ("HeavyArmor", 0.5), ("Unarmed", 2.0)]),
                t("Sonic", [("LightArmor", 1.0), ("HeavyArmor", 0.5), ("Unarmed", 1.5)]),
            ]
            .into_iter()
            .flatten(),
        )
    }

    /// Four strikers (best to worst) and two specials: an attacker and a buffer.
    fn squad() -> Vec<StudentBrief> {
        vec![
            student(1, "시로코*테러", "Pierce", "HeavyArmor", 4, "Main", "DamageDealer"),
            student(2, "호시노(무장)", "Pierce", "LightArmor", 2, "Main", "Tanker"),
            student(3, "사오리(드레스)", "Explosion", "HeavyArmor", 2, "Main", "DamageDealer"),
            student(4, "하나코(수영복)", "Mystic", "Unarmed", 2, "Main", "DamageDealer"),
            student(5, "이로하", "Pierce", "LightArmor", 3, "Support", "Vehicle"),
            student(6, "아코", "Mystic", "LightArmor", 3, "Support", "Supporter"),
        ]
    }

    fn grades(team: &[StudentBrief]) -> Vec<Option<Grade>> {
        let (b, t) = (brief(), types());
        team.iter()
            .map(|s| match rate(s, &b, Some("Outdoor"), &t) {
                Rating::Rated(f) => Some(grade_of(f)),
                Rating::NotRated(_) => None,
            })
            .collect()
    }

    #[test]
    fn the_sample_squad_covers_every_tier_and_the_unrated_special() {
        assert_eq!(grades(&squad()), vec![Some(Grade::Excellent), Some(Grade::Good), Some(Grade::Average), Some(Grade::Poor), Some(Grade::Excellent), None]);
    }

    #[test]
    fn portrait_marks_carry_the_grade_a_terrain_face_and_a_frame_and_skip_rows_scrolled_out() {
        let (b, t) = (brief(), types());
        let students = squad();
        let cards = vec![
            CardName { text: "시로코*테러".into(), x: 736.0, y0: 429.0 },
            CardName { text: "하나코(수영복)".into(), x: 1181.0, y0: 432.0 },
            CardName { text: "하나코(수영복)".into(), x: 1181.0, y0: 370.0 }, // portrait mostly above the grid
            CardName { text: "알수없음".into(), x: 900.0, y0: 430.0 },
        ];
        let marks = card_marks(&cards, &students, Some(&b), Some("Outdoor"), &t, None);
        assert_eq!(marks.len(), 2, "a row scrolled out and an unknown name get no marks");
        assert_eq!(marks[0].icons, vec![Icon::Fit(Grade::Excellent), Icon::Face(4)]);
        assert_eq!(marks[1].icons, vec![Icon::Fit(Grade::Poor), Icon::Face(2)]);
        // the column's middle is on the portrait's right edge, its top just below the badge
        assert_eq!((marks[0].cx, marks[0].top(), marks[0].vertical), (736 + 52, 429 - 68, true));
        let frame = marks[0].frame.unwrap();
        assert_eq!((frame.left, frame.right, frame.grade), (736 - 55, 736 + 55, Grade::Excellent));
        // no mission known: nothing to rate, only the terrain face (and none without a terrain)
        assert_eq!(card_marks(&cards, &students, None, Some("Outdoor"), &t, None)[0].icons, vec![Icon::Face(4)]);
        assert!(card_marks(&cards, &students, None, None, &t, None).is_empty());
    }

    #[test]
    fn a_special_judged_by_skill_gets_only_sp_and_no_frame() {
        let (b, t) = (brief(), types());
        let students = squad();
        let cards = vec![CardName { text: "아코".into(), x: 736.0, y0: 429.0 }, CardName { text: "이로하".into(), x: 847.0, y0: 429.0 }];
        let marks = card_marks(&cards, &students, Some(&b), Some("Outdoor"), &t, Some("Support"));
        assert_eq!(marks[0].icons, vec![Icon::Special]);
        assert!(marks[0].frame.is_none());
        assert_eq!(marks[1].icons[0], Icon::Fit(Grade::Excellent), "an attacker special is rated like a striker");
    }

    #[test]
    fn same_named_students_are_told_apart_by_the_grid_tab() {
        let (b, t) = (brief(), types());
        let striker = student(10, "호시노(무장)", "Pierce", "HeavyArmor", 1, "Main", "DamageDealer");
        let special = student(11, "호시노(무장)", "Mystic", "HeavyArmor", 1, "Support", "Healer");
        let students = vec![striker, special];
        let cards = vec![CardName { text: "호시노(무장)".into(), x: 736.0, y0: 429.0 }];
        assert!(card_marks(&cards, &students, Some(&b), None, &t, None).is_empty(), "no tab known, two different students");
        assert_eq!(card_marks(&cards, &students, Some(&b), None, &t, Some("Support"))[0].icons, vec![Icon::Special]);
        assert!(matches!(card_marks(&cards, &students, Some(&b), None, &t, Some("Main"))[0].icons[0], Icon::Fit(_)));
    }

    #[test]
    fn the_squad_strip_puts_strikers_first_and_specials_last() {
        let (b, t) = (brief(), types());
        let team = vec![squad()[0].clone(), squad()[3].clone(), squad()[5].clone()];
        let marks = slot_marks(&team, Some(&b), Some("Outdoor"), &t);
        assert_eq!(marks.iter().map(|m| (m.cx, m.top())).collect::<Vec<_>>(), vec![(111, 700), (205, 700), (490, 700)]);
        assert_eq!(marks[2].icons, vec![Icon::Special]);
        assert!(slot_marks(&[], Some(&b), None, &t).is_empty());
    }

    #[test]
    fn the_formation_card_leads_with_the_verdict_then_who_fits_then_why() {
        let (b, t, team) = (brief(), types(), squad());
        let score = evaluate(&b, Some("Outdoor"), &team, &team, &t).unwrap();
        assert_eq!((score.members, score.unrated), (5, 1));
        let rows = formation_rows(&score, &b, Some("Outdoor"), &team, &t);
        assert!(matches!(&rows[0], Row::Verdict { label, .. } if label == "편성 판정"));
        let Row::Fits(entries) = &rows[1] else { panic!("fits row second: {rows:?}") };
        assert_eq!(entries.iter().map(|e| e.badge.as_str()).collect::<Vec<_>>(), vec!["강적합", "적합", "보통", "부적합", "강적합", "SP"]);
        assert!(entries[5].grade.is_none() && entries[5].tags.is_empty() && entries[5].note.contains("스킬"));
        let Row::Matrix { cols, rows: lines, .. } = &rows[2] else { panic!("offense table third: {rows:?}") };
        assert_eq!(cols[0].text, "중장갑 20");
        assert_eq!(lines.iter().map(|l| l.label.text.as_str()).collect::<Vec<_>>(), vec!["관통", "폭발", "신비"], "the squad's own attack types, best first");
        assert!(rows.iter().any(|r| matches!(r, Row::Matrix { heading, .. } if heading.starts_with("방어"))));
        assert!(rows.iter().any(|r| matches!(r, Row::Legend { .. })));
        assert!(matches!(rows.last(), Some(Row::Note(n)) if n.contains("5명") && n.contains("스페셜 1명") && n.contains("스킬")));
        let bad = FormationScore { verdict: Verdict::Unfavorable, ..score };
        assert!(matches!(&formation_rows(&bad, &b, Some("Outdoor"), &team, &t)[0], Row::Verdict { text, tone: Tone::Bad, .. } if text == "불리"));
    }

    #[test]
    fn the_mission_card_shows_the_attack_table_before_a_squad_exists_and_leaves_it_to_the_squad_card_after() {
        let (b, t) = (brief(), types());
        let mut before = Vec::new();
        mission_rows(&mut before, &b, &t, true);
        let Some(Row::Matrix { rows, .. }) = before.iter().find(|r| matches!(r, Row::Matrix { .. })) else { panic!("{before:?}") };
        assert_eq!(rows.len(), 4, "all four attack types");
        assert_eq!(rows[0].label.text, "관통", "best against heavy armor first");
        let mut after = Vec::new();
        mission_rows(&mut after, &b, &t, false);
        assert!(!after.iter().any(|r| matches!(r, Row::Matrix { .. })));
        assert!(after.iter().any(|r| matches!(r, Row::Chips { label, .. } if label == "적 방어")));
    }

    #[test]
    fn previews() {
        let Some(dir) = std::env::var_os("BANNER_PREVIEW_DIR") else { return };
        let Some(f) = load_fonts() else { return };
        let dir = std::path::Path::new(&dir);
        let (b, t, team) = (brief(), types(), squad());
        let score = evaluate(&b, Some("Outdoor"), &team, &team, &t).unwrap();
        let card = |rows: Vec<Row>| PanelState { scene: "편성".into(), title: "19-3 설원 지구 제2 지역".into(), rows, ..Default::default() };
        let mut mission = Vec::new();
        mission_rows(&mut mission, &b, &t, true);
        for (name, rows) in [("mission-card", mission), ("formation-card", formation_rows(&score, &b, Some("Outdoor"), &team, &t))] {
            for w in [300u32, ingame_panel_width(458) as u32] {
                render_panel(w, &card(rows.clone()), &f).save(dir.join(format!("{name}-{w}.png"))).unwrap();
            }
        }
        // marks over the real quick-formation screen: cards named after the sample squad in turn
        let shot = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../fixtures/screens/quick-formation.png");
        let Ok(shot) = image::open(shot) else { return };
        let labels = baocr_pixels::quick_card_labels(&shot.to_rgb8());
        let mut students = squad();
        students.extend((0..6).map(|i| student(20 + i, &format!("학생{i}"), ["Pierce", "Explosion", "Mystic"][i as usize % 3], ["HeavyArmor", "Unarmed", "LightArmor"][i as usize % 3], (i as u8 * 2) % 5, "Main", "DamageDealer")));
        let cards: Vec<CardName> = labels.iter().enumerate().map(|(i, l)| CardName { text: students[i % students.len()].name.clone(), x: l.cx as f32, y0: l.y0 as f32 }).collect();
        let mut marks = card_marks(&cards, &students, Some(&b), Some("Outdoor"), &t, None);
        marks.extend(slot_marks(&team, Some(&b), Some("Outdoor"), &t));
        for w in [1280u32, 640, 458] {
            let h = w * 3 / 4;
            let mut out = image::imageops::resize(&shot.to_rgba8(), w, h, image::imageops::FilterType::Triangle);
            let layer: RgbaImage = baocr_overlay::render_marks(w, h, &marks, &f);
            image::imageops::overlay(&mut out, &layer, 0, 0);
            out.save(dir.join(format!("marks-on-screen-{w}.png"))).unwrap();
        }
    }
}
