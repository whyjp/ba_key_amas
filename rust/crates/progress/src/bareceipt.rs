//! Export to the backup file of BAReceipt (https://subba0987.github.io/BAReceipt/).
//!
//! The page's "파일로 저장" writes one JSON object whose values are the page's localStorage strings, each itself
//! JSON: `{basic, students, fav, raid, grand, elim, main, event, mini, group, collab, gift, journal}`. "불러오기"
//! overwrites a localStorage key only when the file has that key, so keys this export has nothing to say about
//! are left out and the page keeps what the user typed there. A key that is written replaces the page's whole
//! value for it (`basic` replaces every field of the basic info, `main` every main-story entry).
//!
//! Only what the scan can read is mapped: nickname, level, circle, student count and the students given by the
//! caller, main story per volume, and event stories by list order. The running event's stages and the
//! MomoTalk bond ranks have no field on the page and are not exported.
use crate::schema::ProgressSnapshot;
use crate::Read;
use regex::Regex;
use serde_json::{json, Map, Value};

/// One student of a student scan, as the caller summarises it.
#[derive(Debug, Clone, PartialEq)]
pub struct StudentSummary {
    pub name: String,
    pub level: Option<u32>,
    /// 1 to 5 stars; the unique-weapon stars are not part of this summary.
    pub stars: Option<u8>,
}

/// `mainStoryList` of the page: key, chapters and episodes when the whole volume is done. The page lists the
/// sixth volume (`ms_7`) before Final (`ms_6`).
const MAIN: [(&str, u32, u32); 8] = [("ms_0", 1, 10), ("ms_1", 3, 43), ("ms_2", 2, 25), ("ms_3", 4, 27), ("ms_4", 2, 24), ("ms_5", 2, 28), ("ms_7", 3, 10), ("ms_6", 4, 12)];

/// The first events of `eventStoryList` (`es_1` to `es_27`), which are in the order the game's list shows them,
/// with the episode count of each. Later entries of the page's list (`es_45_2` ...) are not in list order.
const EVENT: [u32; 27] = [13, 11, 11, 13, 9, 11, 12, 20, 9, 12, 15, 19, 16, 13, 17, 11, 19, 11, 15, 16, 9, 6, 1, 16, 13, 14, 15];

/// The backup file as a JSON object of strings, ready to write.
pub fn export(snapshot: &ProgressSnapshot, students: &[StudentSummary]) -> Read<Value> {
    let mut warnings = Vec::new();
    let mut file = Map::new();
    let mut put = |key: &str, v: Value| {
        file.insert(key.to_string(), Value::String(v.to_string()));
    };

    let basic = basic_info(snapshot, students);
    if !basic.is_empty() {
        put("basic", Value::Object(basic));
    }
    if !students.is_empty() {
        put("students", Value::Array(students.iter().map(student_entry).collect()));
    }
    let main = main_story(snapshot, &mut warnings);
    if !main.is_empty() {
        put("main", Value::Object(main));
    }
    let event = event_story(snapshot, &mut warnings);
    if !event.is_empty() {
        put("event", Value::Object(event));
    }
    Read { value: Value::Object(file), warnings }
}

/// The file's text.
pub fn to_file_text(file: &Value) -> String {
    file.to_string()
}

fn basic_info(s: &ProgressSnapshot, students: &[StudentSummary]) -> Map<String, Value> {
    let mut m = Map::new();
    if let Some(n) = &s.account.nickname {
        m.insert("in-nick".into(), json!(n));
    }
    if let Some(l) = s.account.level {
        m.insert("in-level".into(), json!(l.to_string()));
    }
    if let Some(c) = &s.account.circle {
        m.insert("in-circle".into(), json!(c));
    }
    if !students.is_empty() {
        m.insert("in-students".into(), json!(students.len().to_string()));
    }
    m
}

/// `이름 (Lv. N, M성)` is the shape the page parses back when a student is edited; skills and MomoTalk episode
/// are not read by the scan, so they stay empty.
fn student_entry(s: &StudentSummary) -> Value {
    let name = match (s.level, s.stars) {
        (Some(l), Some(st)) if (1..=5).contains(&st) => format!("{} (Lv. {l}, {st}성)", s.name),
        _ => s.name.clone(),
    };
    json!({ "name": name, "skill": "", "momo": "" })
}

fn main_story(s: &ProgressSnapshot, warnings: &mut Vec<String>) -> Map<String, Value> {
    let re = Regex::new(r"^(Final|\d+)\s*\.").unwrap();
    let mut m = Map::new();
    for v in &s.story.main {
        let slot = re.captures(v.volume.trim()).and_then(|c| match &c[1] {
            "Final" => Some(7),
            n => n.parse::<usize>().ok().filter(|n| *n <= 6),
        });
        let Some(slot) = slot else {
            warnings.push(format!("bareceipt: main story volume '{}' is not one of the page's volumes, skipped", v.volume));
            continue;
        };
        // the page's list order: 0..5, then the sixth volume (ms_7), then Final (ms_6)
        let (key, ch, ep) = MAIN[slot];
        let all_cleared = v.cleared == Some(true) || (!v.chapters.is_empty() && v.chapters.iter().all(|c| c.cleared == Some(true)));
        if all_cleared {
            m.insert(format!("{key}_ch"), json!(ch.to_string()));
            m.insert(format!("{key}_ep"), json!(ep.to_string()));
        } else if !v.chapters.is_empty() && v.chapters.iter().all(|c| c.cleared.is_some()) {
            let done = v.chapters.iter().filter(|c| c.cleared == Some(true)).count();
            if done > 0 {
                m.insert(format!("{key}_ch"), json!(done.to_string()));
            }
        } else {
            warnings.push(format!("bareceipt: main story volume '{}' progress not fully known, skipped", v.volume));
        }
    }
    m
}

fn event_story(s: &ProgressSnapshot, warnings: &mut Vec<String>) -> Map<String, Value> {
    let mut m = Map::new();
    for e in s.events.past.iter().filter(|e| e.cleared == Some(true)) {
        match EVENT.get(e.index as usize) {
            Some(ep) => {
                m.insert(format!("es_{}", e.index + 1), json!(ep.to_string()));
            }
            None => warnings.push(format!("bareceipt: past event #{} has no entry in the page's ordered list, skipped", e.index)),
        }
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{Chapter, PastEvent, StoryVolume};

    fn snapshot() -> ProgressSnapshot {
        let mut s = ProgressSnapshot::new("2026-11-24T12:00:00+09:00", "16:9");
        s.account.nickname = Some("소다팝".into());
        s.account.level = Some(69);
        s.story.main.push(StoryVolume { volume: "Final.그리고 모든 기적이 시작되는 곳".into(), chapters: vec![], cleared: Some(true) });
        s.story.main.push(StoryVolume {
            volume: "3.에덴조약 편".into(),
            chapters: vec![
                Chapter { chapter: "1장".into(), title: None, cleared: Some(true) },
                Chapter { chapter: "2장".into(), title: None, cleared: Some(false) },
            ],
            cleared: None,
        });
        s.events.past = vec![
            PastEvent { index: 0, cleared: Some(true), scanned_while_open: false },
            PastEvent { index: 1, cleared: Some(false), scanned_while_open: false },
            PastEvent { index: 7, cleared: Some(true), scanned_while_open: false },
            PastEvent { index: 40, cleared: Some(true), scanned_while_open: false },
        ];
        s
    }

    fn inner(file: &Value, key: &str) -> Value {
        serde_json::from_str(file[key].as_str().unwrap_or_else(|| panic!("{key} is not a string"))).unwrap()
    }

    #[test]
    fn file_is_json_with_string_encoded_values() {
        let students = [StudentSummary { name: "노노미".into(), level: Some(90), stars: Some(5) }];
        let r = export(&snapshot(), &students);
        let text = to_file_text(&r.value);
        let back: Value = serde_json::from_str(&text).expect("valid JSON");
        for (k, v) in back.as_object().unwrap() {
            assert!(v.is_string(), "{k} must be a JSON-encoded string");
            serde_json::from_str::<Value>(v.as_str().unwrap()).unwrap_or_else(|e| panic!("{k}: {e}"));
        }
        assert_eq!(back, r.value);
    }

    #[test]
    fn basic_students_main_and_event_are_mapped() {
        let students = [
            StudentSummary { name: "노노미".into(), level: Some(90), stars: Some(5) },
            StudentSummary { name: "세리카".into(), level: None, stars: None },
        ];
        let r = export(&snapshot(), &students);
        let f = &r.value;
        assert_eq!(inner(f, "basic"), json!({"in-nick": "소다팝", "in-level": "69", "in-students": "2"}));
        assert_eq!(
            inner(f, "students"),
            json!([{"name": "노노미 (Lv. 90, 5성)", "skill": "", "momo": ""}, {"name": "세리카", "skill": "", "momo": ""}])
        );
        // Final is ms_6 with 4 chapters and 12 episodes; volume 3 has only its first chapter done
        assert_eq!(inner(f, "main"), json!({"ms_6_ch": "4", "ms_6_ep": "12", "ms_3_ch": "1"}));
        // list positions 0 and 7 are done: es_1 (13 episodes) and es_8 (20 episodes)
        assert_eq!(inner(f, "event"), json!({"es_1": "13", "es_8": "20"}));
        assert!(r.warnings.iter().any(|w| w.contains("#40")));
    }

    #[test]
    fn keys_without_data_are_left_out() {
        let r = export(&ProgressSnapshot::new("t", "4:3"), &[]);
        assert_eq!(r.value, json!({}));
        let only_nick = {
            let mut s = ProgressSnapshot::new("t", "4:3");
            s.account.nickname = Some("a".into());
            export(&s, &[])
        };
        let keys: Vec<&String> = only_nick.value.as_object().unwrap().keys().collect();
        assert_eq!(keys, vec!["basic"]);
    }

    #[test]
    fn the_sixth_volume_uses_the_pages_ms_7_key() {
        let mut s = ProgressSnapshot::new("t", "16:9");
        s.story.main.push(StoryVolume { volume: "6.흘러간 시간들의 오라토리오 편".into(), chapters: vec![], cleared: Some(true) });
        let r = export(&s, &[]);
        assert_eq!(inner(&r.value, "main"), json!({"ms_7_ch": "3", "ms_7_ep": "10"}));
    }
}
