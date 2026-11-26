//! Student-name resolution against the embedded dictionary, plus the
//! dictionary-driven fixed meta / cross-check helpers. Port of
//! cmd/baocr/{namematch,fixed}.go and parts of meta.go / deep.go.
use baocr_core::model::*;
use once_cell::sync::Lazy;
use serde::{Deserialize, Deserializer};

static STUDENT_NAMES_JSON: &str = include_str!("../data/student_names_kr.json");

fn null_default<'de, D, T>(d: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Default + Deserialize<'de>,
{
    Ok(Option::<T>::deserialize(d)?.unwrap_or_default())
}

/// Mirrors Go `dictEntry` (cmd/namedict output).
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct DictEntry {
    #[serde(deserialize_with = "null_default")]
    pub id: i32,
    #[serde(deserialize_with = "null_default")]
    pub name: String,
    #[serde(deserialize_with = "null_default")]
    pub base: String,
    #[serde(deserialize_with = "null_default")]
    pub outfit: String,
    #[serde(deserialize_with = "null_default")]
    pub role: String,
    #[serde(deserialize_with = "null_default")]
    pub tactic_role: String,
    #[serde(deserialize_with = "null_default")]
    pub position: String,
    #[serde(deserialize_with = "null_default")]
    pub attack_type: String,
    #[serde(deserialize_with = "null_default")]
    pub armor_type: String,
    #[serde(deserialize_with = "null_default")]
    pub weapon_type: String,
    #[serde(deserialize_with = "null_default")]
    pub star_grade: i32,
    #[serde(deserialize_with = "null_default")]
    pub school: String,
    #[serde(deserialize_with = "null_default")]
    pub club: String,
    #[serde(deserialize_with = "null_default")]
    pub school_year: String,
    #[serde(deserialize_with = "null_default")]
    pub age: String,
    #[serde(deserialize_with = "null_default")]
    pub birthday: String,
    #[serde(deserialize_with = "null_default")]
    pub height: String,
    #[serde(deserialize_with = "null_default")]
    pub full_name: String,
    #[serde(deserialize_with = "null_default")]
    pub hobby: String,
    #[serde(deserialize_with = "null_default")]
    pub designer: String,
    #[serde(deserialize_with = "null_default")]
    pub illustrator: String,
    #[serde(deserialize_with = "null_default")]
    pub voice: String,
    #[serde(deserialize_with = "null_default")]
    pub equipment_slots: Vec<String>,
    #[serde(deserialize_with = "null_default")]
    pub potential_material: String,
    #[serde(deserialize_with = "null_default")]
    pub skill_materials: Vec<String>,
    #[serde(deserialize_with = "null_default")]
    pub weapon_terrain: String,
    #[serde(deserialize_with = "null_default")]
    pub weapon_terrain_bonus: i32,
    #[serde(rename = "terrain_urban", deserialize_with = "null_default")]
    pub urban: i32,
    #[serde(rename = "terrain_outdoor", deserialize_with = "null_default")]
    pub outdoor: i32,
    #[serde(rename = "terrain_indoor", deserialize_with = "null_default")]
    pub indoor: i32,
}

#[derive(Deserialize)]
struct Dict {
    names: Vec<DictEntry>,
}

struct Resolver {
    entries: Vec<DictEntry>,
    jamo: Vec<Vec<char>>,
}

static RESOLVER: Lazy<Resolver> = Lazy::new(|| {
    let d: Dict = serde_json::from_str(STUDENT_NAMES_JSON).expect("embedded student name dictionary is invalid");
    assert!(!d.names.is_empty(), "embedded student name dictionary is empty");
    let jamo = d.names.iter().map(|e| to_jamo(&e.name)).collect();
    Resolver { entries: d.names, jamo }
});

/// The embedded dictionary entries in file order.
pub fn entries() -> &'static [DictEntry] {
    &RESOLVER.entries
}

/// Every dictionary name (file order, duplicates kept, as in the dictionary).
pub fn all_names() -> Vec<String> {
    RESOLVER.entries.iter().map(|e| e.name.clone()).collect()
}

/// Splits Hangul syllables into jamo so a one-stroke misread costs 1.
pub fn to_jamo(s: &str) -> Vec<char> {
    let mut out = Vec::new();
    for c in s.chars() {
        let u = c as u32;
        if (0xAC00..=0xD7A3).contains(&u) {
            let v = u - 0xAC00;
            out.push(char::from_u32(0x1100 + v / 588).unwrap());
            out.push(char::from_u32(0x1161 + (v % 588) / 28).unwrap());
            let t = v % 28;
            if t != 0 {
                out.push(char::from_u32(0x11A7 + t).unwrap());
            }
        } else {
            out.push(c);
        }
    }
    out
}

pub fn edit_distance(a: &[char], b: &[char]) -> usize {
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0usize; b.len() + 1];
    for i in 1..=a.len() {
        cur[0] = i;
        for j in 1..=b.len() {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

/// Keeps only characters a student name can contain. `^` and full-width `＊`
/// are normalised to ASCII `*`.
pub fn clean_name(raw: &str) -> String {
    let raw = raw.replace(['^', '＊'], "*");
    let kept: String = raw
        .chars()
        .filter(|&c| {
            let u = c as u32;
            (0xAC00..=0xD7A3).contains(&u) || c == '(' || c == ')' || c == '*' || c == ' '
        })
        .collect();
    let s = kept.split_whitespace().collect::<Vec<_>>().join(" ");
    s.trim_start_matches([')', '*', ' ']).trim_end_matches(['(', '*', ' ']).to_string()
}

/// Closest dictionary entry, its distance and the gap to the runner-up.
fn nearest(cleaned: &str) -> (usize, i32, i32) {
    let cj = to_jamo(cleaned);
    let (mut best, mut second, mut idx) = (1usize << 30, 1usize << 30, 0usize);
    for (i, j) in RESOLVER.jamo.iter().enumerate() {
        let d = edit_distance(&cj, j);
        if d < best {
            second = best;
            best = d;
            idx = i;
        } else if d < second {
            second = d;
        }
    }
    (idx, best as i32, second as i32 - best as i32)
}

/// Contiguous word runs, widest first.
fn windows(cleaned: &str) -> Vec<String> {
    let w: Vec<&str> = cleaned.split_whitespace().collect();
    let mut out = Vec::new();
    for n in (1..=w.len()).rev() {
        for i in 0..=(w.len() - n) {
            out.push(w[i..i + n].join(" "));
        }
    }
    out
}

fn max_correction(name: &str) -> i32 {
    if to_jamo(name).len() < 5 {
        1
    } else {
        2
    }
}

/// Resolves raw OCR text to a canonical dictionary name.
pub fn resolve(raw: &str) -> (String, NameMatch) {
    let cleaned = clean_name(raw);
    let mut m = NameMatch { cleaned: cleaned.clone(), ..Default::default() };
    if cleaned.is_empty() {
        m.status = "empty".into();
        return (String::new(), m);
    }
    let (mut bi, mut best, mut margin) = (usize::MAX, 1i32 << 30, 0i32);
    let mut used = cleaned.clone();
    for w in windows(&cleaned) {
        let (i, d, mg) = nearest(&w);
        if d < best {
            // widest window wins ties
            bi = i;
            best = d;
            margin = mg;
            used = w;
        }
    }
    m.distance = best;
    m.margin = margin;
    m.candidate = RESOLVER.entries[bi].name.clone();
    if best == 0 {
        m.status = "exact".into();
        return (m.candidate.clone(), m);
    }
    if best <= max_correction(&used) && m.margin >= 2 {
        m.status = "corrected".into();
        return (m.candidate.clone(), m);
    }
    if best <= 2 {
        // Too close to another student to choose: keep the evidence, no name.
        m.status = "ambiguous".into();
        return (String::new(), m);
    }
    m.status = "unlisted".into();
    if used.chars().count() >= 2 {
        return (used, m);
    }
    (String::new(), m)
}

/// A name read off a list screen (duty, MomoTalk), set to the dictionary's spelling. Lists are read in one small
/// font and lose or swap a letter ("하례" for 하레, "키시키" for 키사키, "아네" for 아야네). The closest dictionary name wins
/// when it is within two jamo edits (one for a name of under four jamo) and strictly closer than every other name; a tie or
/// anything farther is not guessed, and `None` leaves the reading as it was.
pub fn snap(raw: &str) -> Option<String> {
    let cleaned = clean_name(raw);
    if cleaned.chars().filter(|c| !c.is_whitespace()).count() < 2 {
        return None;
    }
    let (i, d, margin) = nearest(&cleaned);
    let limit = if to_jamo(&cleaned).len() < 4 { 1 } else { 2 };
    (d <= limit && margin >= 1).then(|| RESOLVER.entries[i].name.clone())
}

/// The plain names (no outfit, no `*`) of exactly `letters` syllables that are closest to `raw`, all of them when several are
/// equally close, and none when the closest is more than two jamo away. For a reading whose length the picture contradicts:
/// the plate is three syllables wide and the reading has two, so the missing syllable is looked for among the names of three.
pub fn nearest_of_length(raw: &str, letters: usize) -> Vec<String> {
    let cleaned = clean_name(raw);
    let cj = to_jamo(&cleaned);
    let mut best = (usize::MAX, Vec::new());
    for (e, j) in RESOLVER.entries.iter().zip(&RESOLVER.jamo) {
        if e.name.contains(['(', '*']) || e.name.chars().count() != letters {
            continue;
        }
        let d = edit_distance(&cj, j);
        if d < best.0 {
            best = (d, vec![e.name.clone()]);
        } else if d == best.0 && !best.1.contains(&e.name) {
            best.1.push(e.name.clone());
        }
    }
    if best.0 <= 2 { best.1 } else { Vec::new() }
}

/// Resolves each candidate and keeps the closest (exact > corrected > unlisted).
pub fn best_name(cands: &[&str]) -> String {
    let rank = |s: &str| match s {
        "exact" => 3,
        "corrected" => 2,
        "unlisted" => 1,
        _ => 0,
    };
    let (mut best, mut best_rank, mut best_dist) = (String::new(), 0, 1i32 << 30);
    for c in cands {
        let (n, m) = resolve(c);
        if n.is_empty() {
            continue;
        }
        let rk = rank(&m.status);
        if rk > best_rank || (rk == best_rank && m.distance < best_dist) {
            best = n;
            best_rank = rk;
            best_dist = m.distance;
        }
    }
    best
}

/// Costume label -> stable key.
pub const OUTFIT_KEYS: &[(&str, &str)] = &[
    ("수영복", "swimsuit"), ("드레스", "dress"), ("무장", "armed"), ("메이드", "maid"), ("체육복", "gym"),
    ("바니걸", "bunny"), ("응원단", "cheer"), ("파자마", "pajama"), ("새해", "new_year"), ("사복", "casual"),
    ("온천", "hot_spring"), ("크리스마스", "christmas"), ("라이딩", "riding"), ("어린이", "kid"), ("교복", "uniform"),
    ("치파오", "qipao"), ("아이돌", "idol"), ("아르바이트", "part_time"), ("매지컬", "magical"), ("레이싱", "racing"),
    ("가이드", "guide"), ("밴드", "band"), ("캠핑", "camping"),
];

pub fn parse_name(name: &str) -> NameParts {
    let mut p = NameParts { base_name: name.to_string(), outfit_key: "base".into(), ..Default::default() };
    let paren = name.find('(').filter(|&i| i > 0);
    if let (Some(i), true) = (paren, name.ends_with(')')) {
        p.base_name = name[..i].to_string();
        p.outfit = name[i + 1..name.len() - 1].to_string();
        p.is_outfit_variant = true;
        p.outfit_key = OUTFIT_KEYS
            .iter()
            .find(|(k, _)| *k == p.outfit)
            .map(|(_, v)| v.to_string())
            .unwrap_or_else(|| "other".into());
    } else if let Some(i) = name.find('*').filter(|&i| i > 0) {
        p.is_distinct_character = true;
        p.distinct_suffix = name[i + 1..].to_string();
        p.outfit_key = "distinct".into();
    }
    p
}

/// Dictionary entry for a resolved name. Duplicate names are split by the
/// role-icon hint; (entry, ambiguous). Ambiguous entries are not to be trusted.
pub fn pick_entry(name: &str, role_hint: &str) -> Option<(DictEntry, bool)> {
    let cands: Vec<&DictEntry> = RESOLVER.entries.iter().filter(|c| c.name == name).collect();
    match cands.len() {
        0 => None,
        1 => Some((cands[0].clone(), false)),
        _ => {
            let fit: Vec<&&DictEntry> =
                cands.iter().filter(|c| !role_hint.is_empty() && c.tactic_role == role_hint).collect();
            if fit.len() == 1 {
                Some(((*fit[0]).clone(), false))
            } else {
                Some((cands[0].clone(), true))
            }
        }
    }
}

pub fn lookup_reference(name: &str, lo: &Loadout) -> Option<Reference> {
    let cands: Vec<&DictEntry> = RESOLVER.entries.iter().filter(|e| e.name == name).collect();
    if cands.is_empty() {
        return None;
    }
    let (mut pick, mut ambiguous) = (cands[0], false);
    if cands.len() > 1 {
        let fit: Vec<&&DictEntry> = cands
            .iter()
            .filter(|e| {
                (lo.tactic_role.is_empty() || e.tactic_role == lo.tactic_role)
                    && (lo.position.is_empty() || e.position == lo.position)
            })
            .collect();
        if !fit.is_empty() {
            pick = fit[0];
        }
        ambiguous = fit.len() != 1;
    }
    Some(Reference {
        source: "schaledb".into(),
        id: pick.id,
        school: pick.school.clone(),
        club: pick.club.clone(),
        school_year: pick.school_year.clone(),
        age: pick.age.clone(),
        birthday: pick.birthday.clone(),
        height: pick.height.clone(),
        full_name: pick.full_name.clone(),
        hobby: pick.hobby.clone(),
        designer: pick.designer.clone(),
        illustrator: pick.illustrator.clone(),
        voice: pick.voice.clone(),
        base_star_grade: pick.star_grade,
        weapon_type: pick.weapon_type.clone(),
        ambiguous_name: ambiguous,
    })
}

/// Fills the fixed groups (fixed_meta + loadout) from the dictionary.
pub fn apply_fixed(s: &mut Student, role_hint: &str) {
    let Some((e, amb)) = pick_entry(&s.name, role_hint) else { return };
    s.fixed_meta = Some(FixedMeta {
        source: "dictionary".into(),
        dictionary_id: e.id,
        ambiguous: amb,
        equipment_slots: e.equipment_slots.clone(),
        potential_material: e.potential_material.clone(),
        skill_materials: e.skill_materials.clone(),
        weapon_terrain: e.weapon_terrain.clone(),
        weapon_terrain_bonus: e.weapon_terrain_bonus,
        terrain_base: Terrain { urban: Some(e.urban), outdoor: Some(e.outdoor), indoor: Some(e.indoor) },
    });
    if amb {
        s.warnings.push("fixed_meta_ambiguous: several dictionary entries share this name".into());
        return; // no fixed loadout rather than a possibly wrong one
    }
    s.loadout = Loadout {
        tactic_role: e.tactic_role,
        position: e.position,
        attack_type: e.attack_type,
        armor_type: e.armor_type,
        weapon_class: e.weapon_type,
    };
}

/// Pixel-vs-dictionary disagreements become warnings; nothing is overwritten.
pub fn cross_check(s: &mut Student) {
    if s.name.is_empty() {
        return;
    }
    let Some(r) = RESOLVER
        .entries
        .iter()
        .find(|e| e.name == s.name && (s.loadout.tactic_role.is_empty() || e.tactic_role == s.loadout.tactic_role))
    else {
        return;
    };
    let mut w: Vec<String> = Vec::new();
    let mut cmp = |field: &str, got: &str, want: &str| {
        if !got.is_empty() && !want.is_empty() && got != want {
            w.push(format!("{field}_mismatch: pixels={got} reference={want}"));
        }
    };
    cmp("position", &s.loadout.position, &r.position);
    cmp("attack_type", &s.loadout.attack_type, &r.attack_type);
    cmp("armor_type", &s.loadout.armor_type, &r.armor_type);
    cmp("weapon_class", &s.loadout.weapon_class, &r.weapon_type);
    for (f, got, want) in [
        ("terrain_urban", s.terrain.urban, r.urban),
        ("terrain_outdoor", s.terrain.outdoor, r.outdoor),
        ("terrain_indoor", s.terrain.indoor, r.indoor),
    ] {
        if let Some(g) = got {
            if g != want {
                w.push(format!("{f}_mismatch: pixels={g} reference={want}"));
            }
        }
    }
    if let Some(rs) = s.rarity_stars {
        if r.star_grade > 0 && rs < r.star_grade {
            w.push("rarity_below_base_star_grade".into());
        }
    }
    s.warnings.extend(w);
}

/// Menu-vs-panel disagreements are warnings; nothing is overwritten.
pub fn deep_cross_check(s: &mut Student) {
    let Some(sd) = s.deep.as_ref().and_then(|d| d.stat_detail.as_ref()) else { return };
    let mut w = Vec::new();
    for (f, a, b) in [
        ("max_hp", s.stats.max_hp, sd.main.max_hp),
        ("attack", s.stats.attack, sd.main.attack),
        ("defense", s.stats.defense, sd.main.defense),
        ("healing", s.stats.healing, sd.main.healing),
    ] {
        if let (Some(a), Some(b)) = (a, b) {
            if a != b {
                w.push(format!("popup_{f}_mismatch: panel={a} popup={b}"));
            }
        }
    }
    s.warnings.extend(w);
}

/// "exp needed for the next level" -> level (pairs observed on a real roster).
pub const EXP_TO_LEVEL: &[(i32, i32)] = &[
    (10, 1), (25, 2), (55, 4), (70, 5), (925, 29), (1685, 39), (9445, 59), (12615, 62),
    (15220, 64), (16685, 65), (18265, 66), (19965, 67), (21790, 68),
];

/// Fills a level the strip OCR could not read.
pub fn level_from_exp(s: &mut Student) {
    if s.level.is_some() {
        return;
    }
    let Some(req) = s.exp.required else { return };
    if let Some((_, lv)) = EXP_TO_LEVEL.iter().find(|(k, _)| *k == req) {
        s.level = Some(*lv);
        s.warnings.push("level_from_exp_table".into());
    }
}

pub const TACTIC_VOCAB: &[&str] = &["딜러", "탱커", "힐러", "서포터", "T.S"];
pub const POSITION_VOCAB: &[&str] = &["FRONT", "MIDDLE", "BACK"];
pub const WEAPON_VOCAB: &[&str] = &["HG", "AR", "SR", "SMG", "SG", "MG", "GL", "RL", "RG", "MT", "FT"];

/// Vocabulary entry nearest to `raw`, or "" unless exactly one entry is within
/// one jamo/letter (a tie is not evidence).
pub fn closest_word(raw: &str, vocab: &[&str]) -> String {
    let raw = raw.trim().to_uppercase();
    let kept: String = raw
        .chars()
        .filter(|&c| {
            let u = c as u32;
            (0xAC00..=0xD7A3).contains(&u) || c.is_ascii_uppercase() || c == '.'
        })
        .collect();
    let cj = to_jamo(&kept);
    let (mut best, mut bw, mut ties) = (1usize << 30, "", 0);
    for w in vocab {
        let d = edit_distance(&cj, &to_jamo(&w.to_uppercase()));
        if d < best {
            best = d;
            bw = w;
            ties = 1;
        } else if d == best {
            ties += 1;
        }
    }
    if best <= 1 && !cj.is_empty() && ties == 1 {
        bw.to_string()
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dropped_syllable_is_looked_for_among_names_of_the_plates_length() {
        let three = nearest_of_length("아네", 3);
        assert!(three.contains(&"아야네".to_string()) && three.contains(&"아카네".to_string()), "{three:?}");
        assert!(three.iter().all(|n| n.chars().count() == 3));
        assert!(nearest_of_length("아네", 2).contains(&"아루".to_string()));
        assert!(nearest_of_length("쀍뛟", 3).is_empty(), "nothing within two jamo");
    }

    #[test]
    fn resolve_name_cases() {
        let cases = [
            ("『사오리(드레스) wey", "사오리(드레스)", "exact"),
            ("' 시로코^테러       the", "시로코*테러", "exact"),
            ("사오리(트레스)", "사오리(드레스)", "corrected"),
            ("' 미카                 tet", "미카", "exact"),
            ("rs iy          whi", "", "empty"),
            ("oa", "", "empty"),
        ];
        for (raw, want, status) in cases {
            let (got, m) = resolve(raw);
            assert!(got == want && m.status == status, "{raw:?} -> {got:?}/{} ({m:?}), want {want:?}/{status}", m.status);
        }
    }

    #[test]
    fn dictionary_round_trip() {
        let shape = regex::Regex::new(r"^[가-힣]+(?: [가-힣]+)*(?:\([가-힣]+\)|\*[가-힣]+)?$").unwrap();
        for e in entries() {
            assert!(shape.is_match(&e.name), "name shape not covered: {:?}", e.name);
            let (got, m) = resolve(&e.name);
            assert!(got == e.name && m.status == "exact", "{:?} resolved to {got:?} ({})", e.name, m.status);
        }
        assert_eq!(all_names().len(), entries().len());
    }

    #[test]
    fn short_name_not_forced() {
        let (got, m) = resolve("츄");
        assert_ne!(m.status, "corrected", "short garbage corrected to {got:?}");
    }

    #[test]
    fn stray_words_dropped() {
        for raw in ["' 리오         해", "리오 해", "0 리오"] {
            let (got, m) = resolve(raw);
            assert!(got == "리오" && m.status == "exact", "{raw:?} -> {got:?}/{}", m.status);
        }
        assert_eq!(resolve("하츠네 미쿠").0, "하츠네 미쿠");
    }

    #[test]
    fn full_width_star_normalised() {
        assert_eq!(clean_name("시로코＊테러"), "시로코*테러");
        assert_eq!(clean_name("시로코^테러"), "시로코*테러");
    }

    fn np(base: &str, outfit: &str, key: &str, is_outfit: bool, distinct: bool, tail: &str) -> NameParts {
        NameParts {
            base_name: base.into(),
            outfit: outfit.into(),
            outfit_key: key.into(),
            is_outfit_variant: is_outfit,
            is_distinct_character: distinct,
            distinct_suffix: tail.into(),
        }
    }

    #[test]
    fn parse_name_cases() {
        let cases = [
            ("리오", np("리오", "", "base", false, false, "")),
            ("리오(무장)", np("리오", "무장", "armed", true, false, "")),
            ("사츠키(수영복)", np("사츠키", "수영복", "swimsuit", true, false, "")),
            ("시로코*테러", np("시로코*테러", "", "distinct", false, true, "테러")),
            ("노도카(온천)", np("노도카", "온천", "hot_spring", true, false, "")),
        ];
        for (name, want) in cases {
            assert_eq!(parse_name(name), want, "{name}");
        }
        for e in entries() {
            let p = parse_name(&e.name);
            assert!(!(p.is_outfit_variant && p.outfit_key == "other"), "outfit {:?} of {} has no key", p.outfit, e.name);
            assert_eq!(p.base_name, e.base, "{}: base vs dictionary", e.name);
        }
    }

    #[test]
    fn closest_word_tie_is_not_evidence() {
        assert_eq!(closest_word("일러", TACTIC_VOCAB), "");
        assert_eq!(closest_word("서포터", TACTIC_VOCAB), "서포터");
    }

    #[test]
    fn level_from_exp_table() {
        let mut s = Student::default();
        s.exp.required = Some(10);
        level_from_exp(&mut s);
        assert_eq!(s.level, Some(1));
        let mut known = Student { level: Some(68), ..Default::default() };
        known.exp.required = Some(10);
        level_from_exp(&mut known);
        assert_eq!(known.level, Some(68));
        let mut unknown = Student::default();
        unknown.exp.required = Some(4242);
        level_from_exp(&mut unknown);
        assert_eq!(unknown.level, None);
    }

    #[test]
    fn duplicate_names_use_role_hint() {
        let dups: Vec<String> = {
            let mut v = Vec::new();
            for e in entries() {
                if entries().iter().filter(|x| x.name == e.name).count() > 1 && !v.contains(&e.name) {
                    v.push(e.name.clone());
                }
            }
            v
        };
        for name in dups {
            let (_, amb) = pick_entry(&name, "").unwrap();
            assert!(amb, "{name} without hint must be ambiguous");
            let e0 = entries().iter().find(|e| e.name == name).unwrap();
            let (e, amb) = pick_entry(&name, &e0.tactic_role).unwrap();
            let same_role = entries().iter().filter(|e| e.name == name && e.tactic_role == e0.tactic_role).count();
            assert_eq!(amb, same_role != 1);
            if !amb {
                assert_eq!(e.tactic_role, e0.tactic_role);
            }
        }
        assert!(pick_entry("없는이름", "").is_none());
    }

    #[test]
    fn apply_fixed_and_cross_check() {
        let mut s = Student { name: "아루".into(), ..Default::default() };
        apply_fixed(&mut s, "");
        assert_eq!(s.loadout.weapon_class, "SR");
        assert_eq!(s.fixed_meta.as_ref().unwrap().terrain_base.urban, Some(4));
        s.loadout.position = "FRONT".into();
        s.terrain.urban = Some(1);
        s.rarity_stars = Some(1);
        cross_check(&mut s);
        assert!(s.warnings.contains(&"position_mismatch: pixels=FRONT reference=BACK".to_string()));
        assert!(s.warnings.contains(&"terrain_urban_mismatch: pixels=1 reference=4".to_string()));
        assert!(s.warnings.contains(&"rarity_below_base_star_grade".to_string()));
        assert!(lookup_reference("아루", &Loadout::default()).is_some());
    }

    #[test]
    fn deep_cross_check_warns() {
        let mut s = Student::default();
        s.stats.attack = Some(10);
        s.deep = Some(Deep {
            stat_detail: Some(StatDetail {
                main: Stats { attack: Some(11), ..Default::default() },
                ..Default::default()
            }),
            ..Default::default()
        });
        deep_cross_check(&mut s);
        assert_eq!(s.warnings, vec!["popup_attack_mismatch: panel=10 popup=11"]);
    }
}
