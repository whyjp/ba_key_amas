//! The widgets the assistant offers.
use crate::widget::{is_mission_screen, Context, Kind, Lookup, Output, Widget, WidgetId};
use baocr_extract::scene::Scene;
use baocr_extract::CardName;
use baocr_overlay::{Chip, Icon, Mark, Row, Tone};
use baocr_wikidb::{evaluate, FormationScore, MissionBrief, StudentBrief, Tally, Verdict, Wiki};

fn chips(list: &[Tally], suffix: &str) -> Vec<Chip> {
    list.iter().map(|t| Chip { text: format!("{} {}{suffix}", t.ko, t.n), tone: Tone::from_code(&t.code) }).collect()
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
                mission_rows(&mut rows, brief);
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

fn mission_rows(rows: &mut Vec<Row>, b: &MissionBrief) {
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
    rows.push(Row::Chips { label: "적 방어".into(), chips: chips(&b.armor, "") });
    rows.push(Row::Chips { label: "추천 공격".into(), chips: chips(&b.best_attack.iter().take(3).cloned().collect::<Vec<_>>(), "%") });
}

/// "+42%" / "-13%" / "평균" for a figure relative to the average (1.0).
fn relative_text(ratio: f64) -> String {
    let pct = ((ratio - 1.0) * 100.0).round() as i32;
    if pct.abs() < 3 {
        "평균".to_string()
    } else {
        format!("{pct:+}%")
    }
}

/// The verdict and the three bars for a squad on a mission.
pub fn formation_rows(score: &FormationScore, terrain_known: bool) -> Vec<Row> {
    let (text, tone) = match score.verdict {
        Verdict::Favorable => ("유리", Tone::Good),
        Verdict::Average => ("평균", Tone::Mid),
        Verdict::Unfavorable => ("불리", Tone::Bad),
    };
    let permille = |r: f64| (r * 1000.0).round() as i32;
    let mut rows = vec![
        Row::Verdict { label: "편성 판정".into(), text: text.into(), tone },
        Row::Meter { label: "공격 상성".into(), permille: permille(score.attack), text: relative_text(score.attack) },
        Row::Meter { label: "방어 상성".into(), permille: permille(score.defense), text: relative_text(score.defense) },
    ];
    if terrain_known {
        rows.push(Row::Meter { label: "지형 적응".into(), permille: permille(score.terrain), text: relative_text(score.terrain) });
    }
    rows.push(Row::Note(format!("인식한 학생 {}명 기준 · 전체 학생 평균 대비 참고용", score.members)));
    rows
}

/// Whether the squad suits the mission: a verdict and bars.
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
        if !matches!(cx.obs.scene, Scene::Squad | Scene::QuickFormation) {
            return None;
        }
        let brief = cx.brief()?;
        let score = evaluate(brief, cx.terrain(), &cx.team(), cx.students, cx.types)?;
        Some(Output::Card { title: String::new(), rows: formation_rows(&score, cx.terrain().is_some()) })
    }
}

/// Icons stand in a column on a portrait's right edge, half on the portrait and half off
/// it, so they stay off the face. The column starts this far above the name label's top,
/// which is just under the squad-number badge in the portrait's top right corner.
const COLUMN_TOP_ABOVE_NAME: i32 = 68;
/// Distance from a card's centre to its portrait's right edge (frame pixels).
const CARD_HALF_WIDTH: i32 = 52;
/// The grid's viewport: a card whose portrait is mostly scrolled out of it gets no icons.
const GRID_ICONS_TOP: i32 = 322;
const GRID_ICONS_BOTTOM: i32 = 600;

fn icons_of(s: &StudentBrief, terrain: Option<&str>) -> Vec<Icon> {
    let mut icons = Vec::new();
    if let Some(level) = terrain.and_then(|t| s.adaptation(t)) {
        icons.push(Icon::Face(level));
    }
    icons.push(Icon::Attack(s.bullet_type.clone()));
    icons.push(Icon::Armor(s.armor_type.clone()));
    icons
}

/// Icons for the student cards on screen: attack and armor type, and, when the mission's
/// terrain is known, how well the student fits it (the game's own face).
pub fn card_marks(cards: &[CardName], students: &[StudentBrief], terrain: Option<&str>, kind: Option<&str>) -> Vec<Mark> {
    cards
        .iter()
        .filter_map(|c| {
            let top = c.y0.round() as i32 - COLUMN_TOP_ABOVE_NAME;
            if !(GRID_ICONS_TOP..=GRID_ICONS_BOTTOM).contains(&top) {
                return None;
            }
            let s = Wiki::match_student_as(students, &c.text, kind)?;
            Some(Mark::column_from_top(c.x.round() as i32 + CARD_HALF_WIDTH, top, icons_of(&s, terrain)))
        })
        .collect()
}

/// Icons over the portraits in the grid of the 빠른 편성 popup.
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
        (cx.obs.scene == Scene::QuickFormation).then(|| Output::Marks(card_marks(&cx.obs.cards, cx.students, cx.terrain(), cx.obs.grid_kind)))
    }
}

/// Right edges of the six slots of the squad strip at the bottom of the 빠른 편성 popup:
/// four strikers, then two specials (frame pixels). The icon columns straddle these edges.
const SLOT_RIGHT_EDGES: [i32; 6] = [111, 205, 300, 392, 490, 579];
/// Top of the column: below the leader badge of the first slot.
const SLOT_COLUMN_TOP: i32 = 700;

/// Slot centres for a squad: the strikers fill the first four, the specials the last two.
pub fn slot_marks(team: &[StudentBrief], terrain: Option<&str>) -> Vec<Mark> {
    let strikers = team.iter().filter(|s| s.squad_type == "Main").take(4).enumerate().map(|(i, s)| (i, s));
    let specials = team.iter().filter(|s| s.squad_type != "Main").take(2).enumerate().map(|(i, s)| (4 + i, s));
    strikers.chain(specials).map(|(slot, s)| Mark::column_from_top(SLOT_RIGHT_EDGES[slot], SLOT_COLUMN_TOP, icons_of(s, terrain))).collect()
}

/// Icons over the squad strip at the bottom of the 빠른 편성 popup.
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
        (cx.obs.scene == Scene::QuickFormation).then(|| Output::Marks(slot_marks(&cx.team(), cx.terrain())))
    }
}

/// Most students named on one furniture card; with more, the rest become a "+N" tag.
const MAX_NAME_TAGS: usize = 3;
/// Space between a card's inside edge and its tags (frame pixels).
const TAG_INSET: i32 = 4;

/// Name tags for one furniture card: every student when there are few, otherwise the first
/// ones and how many more there are, so the tags never grow past what a card can hold.
pub fn furniture_tags(students: &[String]) -> Vec<(String, bool)> {
    if students.len() <= MAX_NAME_TAGS {
        return students.iter().map(|n| (n.clone(), false)).collect();
    }
    let shown = MAX_NAME_TAGS - 1;
    let mut tags: Vec<(String, bool)> = students.iter().take(shown).map(|n| (n.clone(), false)).collect();
    tags.push((format!("+{}", students.len() - shown), true));
    tags
}

/// The students that interact with each furniture item, as name tags on its card.
pub struct FurnitureStudents;

impl Widget for FurnitureStudents {
    fn id(&self) -> WidgetId {
        WidgetId::FurnitureStudents
    }
    fn title(&self) -> &'static str {
        "가구 학생 표시"
    }
    fn kind(&self) -> Kind {
        Kind::Marks
    }
    fn render(&self, cx: &Context) -> Option<Output> {
        if cx.obs.scene != Scene::CafeFurniture {
            return None;
        }
        let marks = cx.obs.furniture.iter().filter(|f| !f.students.is_empty()).map(|f| Mark::tag_column(f.x + TAG_INSET, f.y + TAG_INSET, (f.w - 2 * TAG_INSET).max(0), furniture_tags(&f.students))).collect();
        Some(Output::Marks(marks))
    }
}

/// All widgets, in the order their cards stack.
pub fn all() -> Vec<Box<dyn Widget>> {
    vec![Box::new(MissionInfo), Box::new(Formation), Box::new(PortraitIcons), Box::new(SlotStats), Box::new(FurnitureStudents)]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn student(id: i64, name: &str, bullet: &str, armor: &str, outdoor: u8, squad: &str) -> StudentBrief {
        StudentBrief { id, name: name.into(), bullet_type: bullet.into(), armor_type: armor.into(), street: 0, outdoor, indoor: 0, squad_type: squad.into() }
    }

    #[test]
    fn card_icons_show_types_and_the_terrain_face_and_skip_rows_scrolled_out_of_the_grid() {
        let students = vec![student(1, "호시노(무장)", "Pierce", "HeavyArmor", 4, "Main"), student(2, "케이", "Mystic", "Unarmed", 1, "Main")];
        let cards = vec![
            CardName { text: "호시노(무장)".into(), x: 736.0, y0: 429.0 },
            CardName { text: "케이".into(), x: 1181.0, y0: 432.0 },
            CardName { text: "케이".into(), x: 1181.0, y0: 370.0 }, // portrait mostly above the grid
            CardName { text: "알수없음".into(), x: 900.0, y0: 430.0 },
        ];
        let marks = card_marks(&cards, &students, Some("Outdoor"), None);
        assert_eq!(marks.len(), 2, "a row scrolled out and an unknown name get no icons");
        assert_eq!(marks[0].icons, vec![Icon::Face(4), Icon::Attack("Pierce".into()), Icon::Armor("HeavyArmor".into())]);
        // the column's middle is on the portrait's right edge, its top just below the badge
        assert_eq!((marks[0].cx, marks[0].top(), marks[0].vertical), (736 + 52, 429 - 68, true));
        assert_eq!(card_marks(&cards, &students, None, None)[0].icons.len(), 2, "no terrain: only the types");
    }

    #[test]
    fn same_named_students_are_told_apart_by_the_grid_tab() {
        let striker = student(10, "호시노(무장)", "Pierce", "HeavyArmor", 1, "Main");
        let special = student(11, "호시노(무장)", "Mystic", "HeavyArmor", 1, "Support");
        let students = vec![striker, special];
        let cards = vec![CardName { text: "호시노(무장)".into(), x: 736.0, y0: 429.0 }];
        assert!(card_marks(&cards, &students, None, None).is_empty(), "no tab known, two different students");
        assert_eq!(card_marks(&cards, &students, None, Some("Support"))[0].icons[0], Icon::Attack("Mystic".into()));
        assert_eq!(card_marks(&cards, &students, None, Some("Main"))[0].icons[0], Icon::Attack("Pierce".into()));
    }

    #[test]
    fn the_squad_strip_puts_strikers_first_and_specials_last() {
        let team = vec![
            student(1, "a", "Pierce", "HeavyArmor", 4, "Main"),
            student(2, "b", "Mystic", "Unarmed", 4, "Main"),
            student(3, "c", "Mystic", "Unarmed", 4, "Support"),
        ];
        let marks = slot_marks(&team, Some("Outdoor"));
        assert_eq!(marks.iter().map(|m| (m.cx, m.top())).collect::<Vec<_>>(), vec![(111, 700), (205, 700), (490, 700)]);
        assert!(slot_marks(&[], None).is_empty());
    }

    #[test]
    fn a_furniture_card_names_all_its_few_students_and_counts_the_rest() {
        let names = |n: usize| (0..n).map(|i| format!("학생{i}")).collect::<Vec<_>>();
        assert_eq!(furniture_tags(&names(1)), vec![("학생0".to_string(), false)]);
        assert_eq!(furniture_tags(&names(3)).len(), 3, "three still fit");
        assert!(furniture_tags(&names(3)).iter().all(|(_, muted)| !muted));
        let many = furniture_tags(&names(6));
        assert_eq!(many, vec![("학생0".to_string(), false), ("학생1".to_string(), false), ("+4".to_string(), true)], "two names and how many more");
        assert!(furniture_tags(&[]).is_empty());
    }

    #[test]
    fn furniture_tags_go_inside_each_card_and_only_on_the_furniture_view() {
        use crate::observe::{FurnitureCard, Observation};
        use baocr_wikidb::TypeTable;
        let card = |x: i32, y: i32, n: usize| FurnitureCard { x, y, w: 150, students: (0..n).map(|i| format!("학생{i}")).collect() };
        let mut obs = Observation::new(Scene::CafeFurniture);
        obs.furniture = vec![card(100, 200, 1), card(300, 200, 4), card(500, 200, 0)];
        let types = TypeTable::default();
        let cx = Context { obs: &obs, lookup: Lookup::None, students: &[], types: &types };
        let Some(Output::Marks(marks)) = FurnitureStudents.render(&cx) else { panic!("marks") };
        assert_eq!(marks.len(), 2, "a card with no students gets nothing");
        assert_eq!((marks[0].cx, marks[0].top(), marks[0].max_w, marks[0].left), (104, 204, 142, true), "inside the card, no wider than it");
        assert_eq!(marks[1].icons.len(), 3, "two names and a count");
        obs.scene = Scene::Squad;
        let cx = Context { obs: &obs, lookup: Lookup::None, students: &[], types: &types };
        assert_eq!(FurnitureStudents.render(&cx), None, "other screens show nothing");
    }

    #[test]
    fn the_verdict_and_bars_read_naturally() {
        let score = FormationScore { attack: 1.42, defense: 0.87, terrain: 1.01, overall: 1.2, members: 5, verdict: Verdict::Favorable };
        let rows = formation_rows(&score, true);
        assert_eq!(rows[0], Row::Verdict { label: "편성 판정".into(), text: "유리".into(), tone: Tone::Good });
        assert_eq!(rows[1], Row::Meter { label: "공격 상성".into(), permille: 1420, text: "+42%".into() });
        assert_eq!(rows[2], Row::Meter { label: "방어 상성".into(), permille: 870, text: "-13%".into() });
        assert_eq!(rows[3], Row::Meter { label: "지형 적응".into(), permille: 1010, text: "평균".into() });
        assert!(matches!(rows.last(), Some(Row::Note(n)) if n.contains("5명")));
        assert_eq!(formation_rows(&score, false).len(), rows.len() - 1, "no terrain bar without a known terrain");
        let bad = FormationScore { verdict: Verdict::Unfavorable, ..score };
        assert!(matches!(&formation_rows(&bad, true)[0], Row::Verdict { text, tone: Tone::Bad, .. } if text == "불리"));
    }
}
