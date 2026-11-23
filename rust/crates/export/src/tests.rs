use super::*;
use baocr_core::model::*;
use std::path::PathBuf;

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("baocr-export-{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn a_student_is_written_as_json_only() {
    let out = tmp("student");
    let mut s = Student { name: "<b>리오</b>".into(), ..Default::default() };
    s.raw_ocr.insert("test".into(), "<bad>".into());
    export_student(&s, &out).unwrap();
    assert_eq!(load_student(&out).unwrap(), s, "the JSON round-trips, markup characters included");
    let files: Vec<String> = std::fs::read_dir(&out).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert_eq!(files, vec!["student.json"], "no CSV or HTML");
}

#[test]
fn the_report_is_written_as_json_only() {
    let out = tmp("report");
    let r = RosterReport {
        schema_version: 3,
        entries: vec![RosterEntry { id: "001".into(), student: Student { name: "리오".into(), is_pinned: Some(true), ..Default::default() }, ..Default::default() }],
        ..Default::default()
    };
    save_report(&r, &out).unwrap();
    let back: RosterReport = serde_json::from_slice(&std::fs::read(out.join("students.json")).unwrap()).unwrap();
    assert_eq!(back.entries[0].student.name, "리오");
    let files: Vec<String> = std::fs::read_dir(&out).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert_eq!(files, vec!["students.json"], "no CSV or HTML");
}

#[test]
fn store_diff_and_keep() {
    let dir = tmp("store");
    let st = Store::open(&dir).unwrap();
    let now = chrono::Utc::now();
    let old = Student {
        name: "슌".into(),
        level: Some(68),
        bond_rank: Some(11),
        stats: Stats { max_hp: Some(20221), ..Default::default() },
        ..Default::default()
    };
    let (_, is_new) = st.upsert(&old, now).unwrap();
    assert!(is_new);
    let (ch, is_new) = st.upsert(&old, now).unwrap();
    assert!(!is_new && ch.is_empty());
    let cur = Student { name: "슌".into(), level: Some(68), bond_rank: Some(12), ..Default::default() };
    let (ch, is_new) = st.upsert(&cur, now).unwrap();
    assert!(!is_new);
    assert_eq!(ch, vec![Change { field: "bond_rank".into(), from: "11".into(), to: "12".into() }]);
    let all = st.all().unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].bond_rank, Some(12));
    assert_eq!(all[0].stats.max_hp, Some(20221));
    let hist = std::fs::read_to_string(dir.join("history.jsonl")).unwrap();
    assert_eq!(hist.lines().count(), 2);
    assert!(st.upsert(&Student::default(), now).is_err());
}

#[test]
fn student_key_rules() {
    let mut s = Student { name: "슌".into(), ..Default::default() };
    let k = student_key(&s);
    assert_eq!(k.len(), 13);
    assert!(k.starts_with('n'));
    s.fixed_meta = Some(FixedMeta { dictionary_id: 10, ..Default::default() });
    assert_eq!(student_key(&s), "10");
    s.fixed_meta.as_mut().unwrap().ambiguous = true;
    assert_eq!(student_key(&s), k);
    // sha1("abc") = a9993e364706...
    let a = Student { name: "abc".into(), ..Default::default() };
    assert_eq!(student_key(&a), "na9993e364706");
}

#[test]
fn index_roundtrip_and_rebuild() {
    let out = tmp("index");
    let ix = Index::new(&out);
    ix.update("002", |e| e.status = STATUS_CAPTURED.into());
    ix.update("001", |e| {
        e.status = STATUS_EXTRACTED.into();
        e.name = "리오".into();
    });
    let text = std::fs::read_to_string(out.join("index.json")).unwrap();
    assert!(text.contains("\"version\": 1"));
    let ix2 = Index::load(&out).unwrap();
    assert_eq!(ix2.sorted().iter().map(|e| e.id.as_str()).collect::<Vec<_>>(), ["001", "002"]);
    export_student(&Student { name: "리오".into(), ..Default::default() }, &out.join("details/001")).unwrap();
    let rep = rebuild_report(&out, &ix2, RosterReport { schema_version: 3, ..Default::default() });
    assert_eq!(rep.entries.len(), 2);
    assert_eq!(rep.entries[0].status, "detail_ocr");
    assert_eq!(rep.entries[1].status, "failed");
    assert_eq!(rep.unresolved, ["002"]);
    assert_eq!(rep.entries[1].detail_path, "details/002");
}

/// Real Go outputs: load, export, report, rebuild; skip when the folder is absent.
#[test]
fn real_go_output() {
    let src = PathBuf::from("D:/work-github/bluearchive-ocr/output-fast-full2");
    let Ok(ix) = Index::load(&src) else { return };
    let out = tmp("real");
    let go_report: RosterReport = match std::fs::read(src.join("students.json")) {
        Ok(b) => serde_json::from_slice(&b).expect("Go students.json must deserialize"),
        Err(_) => RosterReport::default(),
    };
    // copy student.json files so export/rebuild run against a scratch dir
    let mut names = Vec::new();
    for e in ix.sorted() {
        let Ok(s) = load_student(&src.join("details").join(&e.id)) else { continue };
        let d = out.join("details").join(&e.id);
        export_student(&s, &d).unwrap();
        assert_eq!(load_student(&d).unwrap(), s, "student.json round trip {}", e.id);
        if !s.name.is_empty() {
            names.push(s.name.clone());
        }
    }
    ix.save().ok();
    let ix_out = Index::new(&out);
    for e in ix.sorted() {
        ix_out.update(&e.id, |x| *x = e.clone());
    }
    let rep = rebuild_report(&out, &ix_out, go_report);
    assert_eq!(rep.entries.len(), ix.sorted().len());
    save_report(&rep, &out).unwrap();
    assert!(!names.is_empty());
    let back: RosterReport = serde_json::from_slice(&std::fs::read(out.join("students.json")).unwrap()).unwrap();
    assert_eq!(back.entries.len(), rep.entries.len());
}
