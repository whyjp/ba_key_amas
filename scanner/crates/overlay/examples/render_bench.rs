//! Times what the overlay draws: the card at the widths it takes, the marks over a full grid, a tooltip and the folded tab.
//!     cargo run -p baocr-overlay --release --example render_bench
use baocr_overlay::{load_fonts, render_marks, render_marks_clipped, render_panel, render_tooltip, Frame, Grade, Hue, Icon, Mark, PanelState, Row, TypeTag};
use std::time::Instant;

fn button(id: &str, label: &str, value: &str, on: bool) -> Row {
    Row::Button { id: id.into(), label: label.into(), value: value.into(), on }
}

fn time(runs: usize, mut f: impl FnMut()) -> f64 {
    f();
    let mut v: Vec<f64> = (0..runs).map(|_| {
        let t = Instant::now();
        f();
        t.elapsed().as_secs_f64() * 1000.0
    }).collect();
    v.sort_by(|a, b| a.total_cmp(b));
    v[v.len() / 2]
}

fn main() {
    let fonts = load_fonts().expect("fonts");
    let open = PanelState {
        scene: "게임 연결됨".into(),
        rows: vec![
            Row::Section { id: "section:student".into(), title: "데이터 수집 · 업로드".into(), open: true, summary: String::new() },
            Row::Heading("1  학생 정보".into()),
            Row::Hint { label: "갱신".into(), value: "30분 경과 · 106명".into(), tip: String::new() },
            button("scan:deep", "스캔 · 상세 포함", "켜짐", true),
            button("scan:start", "학생 정보 스캔", "시작", false),
            Row::Heading("2  진행 상태".into()),
            button("scan:progress", "진행 상태 스캔", "시작", false),
            Row::Heading("3  업로드".into()),
            button("upload", "스캔 결과 업로드", "전송", false),
            Row::Info { label: "결과".into(), value: "업로드 완료 · 공개 (106 명, 점수 12345)".into() },
            Row::Note("https://key-amas.cxx.co.kr/#/u/abcd1234".into()),
        ],
        close: Some(("quit".into(), "x".into())),
        pin: Some(true),
        settings: Some(("settings".into(), "x".into())),
        ..Default::default()
    };
    let small = PanelState { scene: "게임 연결됨".into(), rows: vec![open.rows[0].clone()], ..Default::default() };
    for width in [300u32, 360, 480, 640] {
        println!("render_panel {:>3} rows at {width:>3} px: {:6.2} ms", open.rows.len(), time(9, || { std::hint::black_box(render_panel(width, &open, &fonts)); }));
    }
    println!("render_panel   1 row  at 360 px: {:6.2} ms", time(9, || { std::hint::black_box(render_panel(360, &small, &fonts)); }));
    println!("render_tooltip: {:6.2} ms", time(9, || { std::hint::black_box(render_tooltip(1.0, "마지막 스캔\n2026-11-25 14:00", &fonts)); }));
    let marks: Vec<Mark> = (0..24)
        .map(|i| {
            let icons = vec![Icon::Fit(Grade::Excellent), Icon::Attack(TypeTag { hue: Hue::Red, glyph: '폭' }), Icon::Armor(TypeTag { hue: Hue::Yellow, glyph: '중' })];
            Mark::column_from_top(660 + 110 * (i % 6), 360 + 104 * (i / 6), icons).framed(Frame { left: 600 + 110 * (i % 6), top: 322 + 104 * (i / 6), right: 710 + 110 * (i % 6), bottom: 420 + 104 * (i / 6), grade: Grade::Excellent })
        })
        .collect();
    let frames_only: Vec<Mark> = marks.iter().map(|m| { let mut m = Mark::column_from_top(m.cx, m.top(), vec![]); m.frame = marks[0].frame; m }).collect();
    let icons_only: Vec<Mark> = marks.iter().map(|m| Mark::column_from_top(m.cx, m.top(), m.icons.clone())).collect();
    let frames_only: Vec<Mark> = marks.iter().map(|m| Mark { icons: vec![], ..m.clone() }).collect::<Vec<_>>().into_iter().chain(frames_only.into_iter().take(0)).collect();
    println!("  only the frames of the 24: {:6.2} ms, only their icons: {:6.2} ms (client 1601x1201, clipped)", time(9, || { std::hint::black_box(render_marks_clipped(1601, 1201, &frames_only, &fonts)); }), time(9, || { std::hint::black_box(render_marks_clipped(1601, 1201, &icons_only, &fonts)); }));
    for (w, h) in [(1280u32, 960u32), (1601, 1201), (2560, 1920)] {
        let (part, at) = render_marks_clipped(w, h, &marks, &fonts);
        println!(
            "render_marks 24 framed marks, client {w}x{h}: whole client {:6.2} ms, only where drawn {:6.2} ms ({}x{} at {:?}, {:.0}% of the area)",
            time(9, || { std::hint::black_box(render_marks(w, h, &marks, &fonts)); }),
            time(9, || { std::hint::black_box(render_marks_clipped(w, h, &marks, &fonts)); }),
            part.width(), part.height(), at, 100.0 * (part.width() * part.height()) as f64 / (w * h) as f64
        );
    }
}
