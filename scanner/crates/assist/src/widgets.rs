//! The widgets the assistant offers.
use crate::matchup::{all_attack_types_by_edge, attack_types_by_edge, defense_matrix, fit_entry, grade_of, legend, offense_matrix, share_labels, striker_armors, terrain_ko, tidy, MAX_CHIPS};
use crate::raid_lookup::{RaidFound, RaidLookup};
use crate::widget::{is_mission_screen, is_raid_screen, Context, Kind, Lookup, Output, Widget, WidgetId};
use baocr_extract::raid::{RaidRef, DIFFICULTIES};
use baocr_extract::scene::{Layout, RaidMode, Scene};
use baocr_extract::CardName;
use baocr_overlay::{Chip, Frame, Hue, Icon, Mark, Row, Tone, TypeTag};
use baocr_wikidb::{evaluate, rate, FormationScore, MissionBrief, Rating, SeasonRelation, Share, StudentBrief, Tally, TypeTable, Verdict, Wiki};

/// The types of an encounter with their share of the enemy HP, the small ones merged into "기타".
fn chips(list: &[Share]) -> Vec<Chip> {
    share_labels(&tidy(list, MAX_CHIPS)).into_iter().map(|(text, code)| Chip { text, tone: code.map_or(Tone::Even, |c| Tone::from_code(&c)) }).collect()
}

/// Said under the chips: what the percentages are.
const SHARE_NOTE: &str = "비율은 적 체력(HP) 합계에서 차지하는 몫입니다. 소환물은 제외";

fn on_formation_screen(scene: Scene) -> bool {
    matches!(scene, Scene::Squad | Scene::QuickFormation | Scene::RaidFormation(_))
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
    if !b.attack.is_empty() {
        rows.push(Row::Chips { label: "적 공격".into(), chips: chips(&b.attack) });
    }
    let table = with_table.then(|| offense_matrix(b, &all_attack_types_by_edge(b, types), types)).flatten();
    match table {
        Some(table) => rows.extend([table, legend()]),
        None => {
            rows.push(Row::Chips { label: "적 방어".into(), chips: chips(&b.armor) });
            rows.push(Row::Note(SHARE_NOTE.into()));
        }
    }
}

/// Said on a raid formation card: the verdict judges one party.
const PARTY_NOTE: &str = "총력전은 최대 4개 부대(1부대~4부대)를 편성합니다. 이 판정은 마지막으로 읽은 한 부대만 기준이며, 부대 간 조합은 판단하지 않습니다";
/// Said on a raid formation screen that names no student.
const NO_NAMES_NOTE: &str = "편성 화면에는 학생 이름이 나오지 않습니다. 빠른 편성 창을 열면 선택한 학생을 읽어 판정합니다";

/// How the difficulties attack, grouped where neighbours share a type: "Normal~Hardcore 일반".
fn attack_ranges(found: &RaidFound) -> Vec<Chip> {
    let mut groups: Vec<(&str, &str, &Tally)> = Vec::new();
    for (label, tally) in &found.attack_by_difficulty {
        match groups.last_mut() {
            Some(last) if last.2.code == tally.code => last.1 = label,
            _ => groups.push((label, label, tally)),
        }
    }
    groups
        .into_iter()
        .map(|(from, to, t)| Chip { text: if from == to { format!("{from} {}", t.ko) } else { format!("{from}~{to} {}", t.ko) }, tone: Tone::from_code(&t.code) })
        .collect()
}

/// The encounter's facts, or why they are not known. `on_formation`: the formation card shows the tables.
pub fn raid_rows(raid: &RaidRef, found: &RaidLookup, types: &TypeTable, on_formation: bool, squad_read: bool) -> Vec<Row> {
    let mut rows = Vec::new();
    match found {
        RaidLookup::Found(f) => {
            let unrestricted = raid.mode == RaidMode::UnrestrictedRaid;
            match (raid.difficulty, unrestricted) {
                (Some(i), _) => rows.push(Row::Info { label: "난이도".into(), value: DIFFICULTIES[i].into() }),
                (None, true) => rows.push(Row::Info { label: "단계".into(), value: "선택한 단계는 읽지 못합니다".into() }),
                (None, false) => rows.push(Row::Info { label: "난이도".into(), value: "선택 전 (방 정보 창에서 표시)".into() }),
            }
            if f.brief.terrain.is_none() {
                let value = if f.raid.terrains.is_empty() { "위키에 없음".to_string() } else { format!("{} 중 선택", f.raid.terrains.iter().map(|t| terrain_ko(t)).collect::<Vec<_>>().join(" · ")) };
                rows.push(Row::Info { label: "지형".into(), value });
            }
            let mut brief = f.brief.clone();
            let attack_unknown = raid.difficulty.is_none() && !unrestricted;
            if attack_unknown {
                brief.attack.clear();
            }
            mission_rows(&mut rows, &brief, types, !on_formation);
            if attack_unknown {
                rows.push(Row::Chips { label: "적 공격".into(), chips: attack_ranges(f) });
                if on_formation {
                    let first = f.attack_by_difficulty.first().map_or("첫", |(l, _)| l.as_str());
                    rows.push(Row::Note(format!("난이도를 읽지 못해 {first} 기준으로 판정합니다")));
                }
            }
            if let Some(p) = f.season.as_ref().filter(|p| p.relation != SeasonRelation::Active) {
                rows.push(Row::Note(format!("진행 중인 시즌이 없어 가장 가까운 시즌({})의 지형입니다", p.season.display.as_deref().unwrap_or("?"))));
            }
            if f.picked_by_armor {
                rows.push(Row::Note("같은 이름의 레이드가 여럿이라 화면의 방어 타입으로 골랐습니다".into()));
            }
            for d in &f.disagreements {
                rows.push(Row::Note(format!("{}: 화면 {} / 위키 {} (위키 기준으로 계산, 위키 모델 점검 필요)", d.what, d.screen, d.wiki)));
            }
            if !f.unknown_words.is_empty() {
                rows.push(Row::Note(format!("모르는 단어라 반영하지 못했습니다: {}", f.unknown_words.join(", "))));
            }
        }
        RaidLookup::Ambiguous(candidates) => {
            rows.push(Row::Note(format!("같은 이름의 레이드가 {}개입니다. 방어 타입이 보이는 화면(보스 선택, 단계 선택)에서 구분합니다", candidates.len())));
        }
        RaidLookup::NotFound => rows.push(Row::Note("위키DB에서 이 보스를 찾지 못했습니다".into())),
        RaidLookup::None => {}
    }
    if on_formation && !squad_read {
        rows.push(Row::Note(NO_NAMES_NOTE.into()));
    }
    rows
}

/// The raid encounter: boss, difficulty, terrain, level, what it attacks and wears, and the matchup tables.
pub struct RaidInfo;

impl Widget for RaidInfo {
    fn id(&self) -> WidgetId {
        WidgetId::RaidInfo
    }
    fn title(&self) -> &'static str {
        "레이드 카드"
    }
    fn kind(&self) -> Kind {
        Kind::Card
    }
    fn render(&self, cx: &Context) -> Option<Output> {
        let scene = cx.obs.scene;
        if !is_raid_screen(scene) {
            return None;
        }
        let Some(raid) = cx.obs.raid.as_ref() else {
            // a formation reached without the boss having been seen on the way in
            return matches!(scene, Scene::RaidFormation(_)).then(|| Output::Card { title: scene.label().into(), rows: vec![Row::Note("보스 정보를 읽지 못했습니다. 보스 선택이나 방 정보 화면을 거치면 표시됩니다".into())] });
        };
        let on_formation = matches!(scene, Scene::RaidFormation(_) | Scene::QuickFormation);
        Some(Output::Card { title: raid.display(), rows: raid_rows(raid, &cx.raid, cx.types, on_formation, cx.team().len() >= MIN_SQUAD_FOR_VERDICT) })
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
        let mut rows = formation_rows(&score, brief, cx.terrain(), &team, cx.types);
        if cx.obs.raid.as_ref().is_some_and(|r| r.mode != RaidMode::UnrestrictedRaid) {
            rows.insert(0, Row::Note(PARTY_NOTE.into()));
        }
        Some(Output::Card { title: String::new(), rows })
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
/// The 빠른 편성 popup keeps its size and the order of its parts in 16:9 but is squeezed vertically:
/// its top edge moves from y 182 to y 62 and everything inside it rises by the same 120 px
/// (measured: the first name row at y 431 in 4:3 and 311 in 16:9, the squad strip at 676 and 556).
const POPUP_RISE_169: i32 = 120;

/// The vertical places of the 빠른 편성 popup's parts in one layout.
struct PopupSpots {
    grid_icons_top: i32,
    grid_icons_bottom: i32,
    slot_column_top: i32,
    slot_frame_top: i32,
    slot_frame_bottom: i32,
}

fn popup_spots(layout: Layout) -> PopupSpots {
    let rise = match layout {
        Layout::Ratio43 => 0,
        Layout::Ratio169 => POPUP_RISE_169,
    };
    PopupSpots {
        grid_icons_top: GRID_ICONS_TOP - rise,
        grid_icons_bottom: GRID_ICONS_BOTTOM - rise,
        slot_column_top: SLOT_COLUMN_TOP - rise,
        slot_frame_top: SLOT_FRAME.1 - rise,
        slot_frame_bottom: SLOT_FRAME.3 - rise,
    }
}

/// What goes on one student's portrait, top to bottom: how well it suits the battle (or "SP" for a special
/// that is not rated), then the attack and armor type, then the terrain face. The types are the student's
/// own facts, so they are shown whether or not a mission is known: the verdict never takes their place.
/// Without a known mission nothing can be rated, and the types and the terrain face (when known) remain.
fn marks_of(s: &StudentBrief, brief: Option<&MissionBrief>, terrain: Option<&str>, types: &TypeTable, paired: bool) -> (Vec<Icon>, Option<baocr_overlay::Grade>) {
    let face = terrain.and_then(|t| s.adaptation(t)).map(Icon::Face);
    let kinds = type_icons(s, paired);
    match brief.map(|b| rate(s, b, terrain, types)) {
        Some(Rating::Rated(fit)) => {
            let grade = grade_of(fit);
            (std::iter::once(Icon::Fit(grade)).chain(kinds).chain(face).collect(), Some(grade))
        }
        Some(Rating::NotRated(_)) => (std::iter::once(Icon::Special).chain(kinds).collect(), None),
        None => (kinds.into_iter().chain(face).collect(), None),
    }
}

/// The attack and armor type badges: two stacked ones, or one row when `paired` (the squad strip has
/// little height to spare).
fn type_icons(s: &StudentBrief, paired: bool) -> Vec<Icon> {
    match (paired, attack_icon(&s.bullet_type), armor_icon(&s.armor_type)) {
        (true, Some(Icon::Attack(a)), Some(Icon::Armor(r))) => vec![Icon::Pair(a, r)],
        (_, a, r) => a.into_iter().chain(r).collect(),
    }
}

fn attack_icon(code: &str) -> Option<Icon> {
    let (hue, glyph) = match code {
        "Explosion" => (Hue::Red, '폭'),
        "Pierce" => (Hue::Yellow, '관'),
        "Mystic" => (Hue::Blue, '신'),
        "Sonic" => (Hue::Purple, '진'),
        _ => return None,
    };
    Some(Icon::Attack(TypeTag { hue, glyph }))
}

fn armor_icon(code: &str) -> Option<Icon> {
    let (hue, glyph) = match code {
        "LightArmor" => (Hue::Red, '경'),
        "HeavyArmor" => (Hue::Yellow, '중'),
        "Unarmed" => (Hue::Blue, '특'),
        "ElasticArmor" => (Hue::Purple, '탄'),
        "CompositeArmor" => (Hue::Grey, '복'),
        _ => return None,
    };
    Some(Icon::Armor(TypeTag { hue, glyph }))
}

/// Marks for the student cards on screen (a 4:3 frame).
pub fn card_marks(cards: &[CardName], students: &[StudentBrief], brief: Option<&MissionBrief>, terrain: Option<&str>, types: &TypeTable, kind: Option<&str>) -> Vec<Mark> {
    card_marks_in(cards, students, brief, terrain, types, kind, Layout::Ratio43)
}

/// [`card_marks`] for a frame of the given layout.
pub fn card_marks_in(cards: &[CardName], students: &[StudentBrief], brief: Option<&MissionBrief>, terrain: Option<&str>, types: &TypeTable, kind: Option<&str>, layout: Layout) -> Vec<Mark> {
    let spots = popup_spots(layout);
    cards
        .iter()
        .filter_map(|c| {
            let top = c.y0.round() as i32 - COLUMN_TOP_ABOVE_NAME;
            if !(spots.grid_icons_top..=spots.grid_icons_bottom).contains(&top) {
                return None;
            }
            let s = Wiki::match_student_as(students, &c.text, kind)?;
            let (icons, grade) = marks_of(&s, brief, terrain, types, false);
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
        (cx.obs.scene == Scene::QuickFormation).then(|| Output::Marks(card_marks_in(&cx.obs.cards, cx.students, cx.brief(), cx.terrain(), cx.types, cx.obs.grid_kind, cx.obs.layout)))
    }
}

use crate::portrait::SLOT_RIGHT_EDGES;
/// Top of the column: below the leader badge of the first slot.
const SLOT_COLUMN_TOP: i32 = 700;
/// A slot's outline: this far left of its right edge, between these rows, this far right of it.
const SLOT_FRAME: (i32, i32, i32, i32) = (-89, 676, 2, 760);

/// Marks for a squad: the strikers fill the first four slots, the specials the last two (a 4:3 frame).
pub fn slot_marks(team: &[StudentBrief], brief: Option<&MissionBrief>, terrain: Option<&str>, types: &TypeTable) -> Vec<Mark> {
    slot_marks_in(team, brief, terrain, types, Layout::Ratio43)
}

/// [`slot_marks`] for a frame of the given layout.
pub fn slot_marks_in(team: &[StudentBrief], brief: Option<&MissionBrief>, terrain: Option<&str>, types: &TypeTable, layout: Layout) -> Vec<Mark> {
    let mut slots: [Option<StudentBrief>; 6] = Default::default();
    for (i, s) in team.iter().filter(|s| s.squad_type == "Main").take(4).enumerate() {
        slots[i] = Some(s.clone());
    }
    for (i, s) in team.iter().filter(|s| s.squad_type != "Main").take(2).enumerate() {
        slots[4 + i] = Some(s.clone());
    }
    slot_marks_at(&slots, brief, terrain, types, layout)
}

/// Marks for the squad strip when who stands in each slot is known (an empty or unknown slot gets none).
pub fn slot_marks_at(slots: &[Option<StudentBrief>; 6], brief: Option<&MissionBrief>, terrain: Option<&str>, types: &TypeTable, layout: Layout) -> Vec<Mark> {
    let spots = popup_spots(layout);
    slots
        .iter()
        .enumerate()
        .filter_map(|(slot, s)| s.as_ref().map(|s| (slot, s)))
        .filter_map(|(slot, s)| {
            let (icons, grade) = marks_of(s, brief, terrain, types, true);
            if icons.is_empty() {
                return None;
            }
            let edge = SLOT_RIGHT_EDGES[slot];
            let mark = Mark::column_from_top(edge, spots.slot_column_top, icons);
            Some(match grade {
                Some(grade) => mark.framed(Frame { left: edge + SLOT_FRAME.0, top: spots.slot_frame_top, right: edge + SLOT_FRAME.2, bottom: spots.slot_frame_bottom, grade }),
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
        // the strip's place on a raid's popup is not known: no icons rather than icons on the wrong things
        (cx.obs.scene == Scene::QuickFormation && cx.obs.raid.is_none()).then(|| Output::Marks(slot_marks_at(&cx.slot_students(), cx.brief(), cx.terrain(), cx.types, cx.obs.layout)))
    }
}

/// All widgets, in the order their cards stack.
pub fn all() -> Vec<Box<dyn Widget>> {
    vec![Box::new(MissionInfo), Box::new(RaidInfo), Box::new(Formation), Box::new(PortraitIcons), Box::new(SlotStats)]
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
        let share = |code: &str, ko: &str, share: f64| Share { code: code.into(), ko: ko.into(), share, heads: 1 };
        MissionBrief {
            stage_id: 1,
            name: "19-3".into(),
            area: Some(19),
            stage_no: Some(3),
            difficulty: 0,
            terrain: Some(tally("Outdoor", "야외", 0)),
            recommended_level: Some(85),
            attack: vec![share("Mystic", "신비", 0.68), share("Explosion", "폭발", 0.32)],
            armor: vec![share("HeavyArmor", "중장갑", 0.62), share("Unarmed", "특수장갑", 0.28), share("LightArmor", "경장갑", 0.10)],
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
    fn portrait_marks_carry_the_grade_the_attack_and_armor_type_a_terrain_face_and_a_frame_and_skip_rows_scrolled_out() {
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
        let (pierce, heavy) = (Icon::Attack(TypeTag { hue: Hue::Yellow, glyph: '관' }), Icon::Armor(TypeTag { hue: Hue::Yellow, glyph: '중' }));
        let (mystic, unarmed) = (Icon::Attack(TypeTag { hue: Hue::Blue, glyph: '신' }), Icon::Armor(TypeTag { hue: Hue::Blue, glyph: '특' }));
        // the verdict and the student's own types are both there: the verdict does not take the types' place
        assert_eq!(marks[0].icons, vec![Icon::Fit(Grade::Excellent), pierce.clone(), heavy.clone(), Icon::Face(4)]);
        assert_eq!(marks[1].icons, vec![Icon::Fit(Grade::Poor), mystic, unarmed, Icon::Face(2)]);
        // the column's middle is on the portrait's right edge, its top just below the badge
        assert_eq!((marks[0].cx, marks[0].top(), marks[0].vertical), (736 + 52, 429 - 68, true));
        let frame = marks[0].frame.unwrap();
        assert_eq!((frame.left, frame.right, frame.grade), (736 - 55, 736 + 55, Grade::Excellent));
        // no mission known: nothing to rate, so the fixed facts from the database: attack type, armor type, terrain face
        assert_eq!(card_marks(&cards, &students, None, Some("Outdoor"), &t, None)[0].icons, vec![pierce.clone(), heavy.clone(), Icon::Face(4)]);
        assert_eq!(card_marks(&cards, &students, None, None, &t, None)[0].icons, vec![pierce, heavy], "without a terrain the face is left out, the types stay");
    }

    #[test]
    fn a_special_judged_by_skill_gets_sp_and_its_types_but_no_frame() {
        let (b, t) = (brief(), types());
        let students = squad();
        let cards = vec![CardName { text: "아코".into(), x: 736.0, y0: 429.0 }, CardName { text: "이로하".into(), x: 847.0, y0: 429.0 }];
        let marks = card_marks(&cards, &students, Some(&b), Some("Outdoor"), &t, Some("Support"));
        let (mystic, light) = (Icon::Attack(TypeTag { hue: Hue::Blue, glyph: '신' }), Icon::Armor(TypeTag { hue: Hue::Red, glyph: '경' }));
        assert_eq!(marks[0].icons, vec![Icon::Special, mystic, light]);
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
        assert_eq!(card_marks(&cards, &students, Some(&b), None, &t, Some("Support"))[0].icons[0], Icon::Special);
        assert!(matches!(card_marks(&cards, &students, Some(&b), None, &t, Some("Main"))[0].icons[0], Icon::Fit(_)));
    }

    #[test]
    fn the_squad_strip_puts_strikers_first_and_specials_last() {
        let (b, t) = (brief(), types());
        let team = vec![squad()[0].clone(), squad()[3].clone(), squad()[5].clone()];
        let marks = slot_marks(&team, Some(&b), Some("Outdoor"), &t);
        assert_eq!(marks.iter().map(|m| (m.cx, m.top())).collect::<Vec<_>>(), vec![(111, 700), (205, 700), (490, 700)]);
        assert_eq!(marks[2].icons[0], Icon::Special);
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
        assert_eq!(cols.iter().map(|c| c.text.as_str()).collect::<Vec<_>>(), vec!["중장갑 62%", "특수장갑 28%", "경장갑 10%"]);
        assert_eq!(lines.iter().map(|l| l.label.text.as_str()).collect::<Vec<_>>(), vec!["관통", "신비", "폭발"], "the squad's own attack types, best first");
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
