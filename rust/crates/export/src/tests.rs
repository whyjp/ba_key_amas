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
fn export_escapes_ocr() {
    let out = tmp("escape");
    let mut s = Student { name: "<script>alert(1)</script>".into(), ..Default::default() };
    s.raw_ocr.insert("test".into(), "<bad>".into());
    export_student(&s, &out).unwrap();
    let h = std::fs::read_to_string(out.join("student.html")).unwrap();
    assert!(!h.contains("<script>alert(1)</script>"));
    assert!(h.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
    assert!(out.join("student.json").exists() && out.join("student.csv").exists());
}

#[test]
fn save_report_renders_html() {
    let out = tmp("report");
    let r = RosterReport {
        schema_version: 3,
        entries: vec![RosterEntry {
            id: "001".into(),
            student: Student { name: "리오".into(), is_pinned: Some(true), ..Default::default() },
            ..Default::default()
        }],
        ..Default::default()
    };
    save_report(&r, &out).unwrap();
    let h = std::fs::read_to_string(out.join("students.html")).unwrap();
    assert!(h.contains("리오") && h.contains(r#"data-pin="true""#) && h.contains("고정"));
    assert!(h.contains(r#"id="armor""#) && h.contains("update()</script>"));
    let csv = std::fs::read_to_string(out.join("students.csv")).unwrap();
    assert!(csv.starts_with("id,page,status,detail_path,name,"));
    assert!(csv.ends_with("\r\n") && !csv.starts_with('\u{feff}'));
}

#[test]
fn csv_header_and_row_align() {
    let (h, r) = student_csv(&Student::default());
    assert_eq!(h.len(), r.len());
    assert_eq!(h.len(), 12 + meta_columns(&Student::default()).len() + 20);
    assert_eq!(&h[..3], ["name", "level", "rarity_stars"]);
    assert_eq!(h.last().unwrap(), "equipment_4_level");
    // Go quoting: comma, quote, newline, leading space.
    let rec = csv_record(&["a,b".into(), "q\"x".into(), " lead".into(), "ok".into(), "".into()]);
    assert_eq!(rec, "\"a,b\",\"q\"\"x\",\" lead\",ok,\r\n");
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
        let (h, r) = student_csv(&s);
        assert_eq!(h.len(), r.len());
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
    let html = std::fs::read_to_string(out.join("students.html")).unwrap();
    assert!(!names.is_empty());
    for n in &names {
        assert!(html.contains(&crate::html::esc(n)), "missing {n}");
    }
    let back: RosterReport = serde_json::from_slice(&std::fs::read(out.join("students.json")).unwrap()).unwrap();
    assert_eq!(back.entries.len(), rep.entries.len());
}

/// The Go students.csv header must match ours exactly (skip when absent).
#[test]
fn csv_header_matches_go() {
    let Ok(go) = std::fs::read_to_string("D:/work-github/bluearchive-ocr/output-fast-full2/students.csv") else { return };
    let go_header = go.split("\r\n").next().unwrap();
    let mut h = vec!["id".to_string(), "page".into(), "status".into(), "detail_path".into()];
    h.extend(student_csv(&Student::default()).0);
    assert_eq!(csv_record(&h).trim_end(), go_header);
}
