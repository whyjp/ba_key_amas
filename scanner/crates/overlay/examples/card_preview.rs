//! Renders the main card's three looks to PNG files: `cargo run -p baocr-overlay --example card_preview -- <dir>`.
use baocr_overlay::{load_fonts, render_panel, PanelState, Row, PANEL_BASE_W};

fn button(id: &str, label: &str, value: &str, on: bool) -> Row {
    Row::Button { id: id.into(), label: label.into(), value: value.into(), on }
}

fn card(rows: Vec<Row>) -> PanelState {
    PanelState { scene: "게임 연결됨".into(), rows, close: Some(("quit".into(), "애드온 종료".into())), pin: Some(true), settings: Some(("settings".into(), "표시 설정".into())), ..Default::default() }
}

fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).expect("an output folder"));
    let fonts = load_fonts().expect("fonts");
    let section = |open: bool| Row::Section { id: "section:student".into(), title: "데이터 수집 · 업로드".into(), open, summary: if open { String::new() } else { "30분 경과 · 106명".into() } };
    let open = card(vec![
        section(true),
        Row::Heading("1  학생 정보".into()),
        Row::Hint { label: "갱신".into(), value: "30분 경과 · 106명".into(), tip: String::new() },
        button("scan:deep", "스캔 · 상세 포함", "켜짐", true),
        button("scan:start", "학생 정보 스캔", "시작", false),
        Row::Heading("2  진행 상태".into()),
        button("scan:progress", "진행 상태 스캔", "시작", false),
        Row::Heading("3  업로드".into()),
        button("code:change", "계정", "저장됨 · 바꾸기", false),
        button("upload", "스캔 결과 업로드", "전송", false),
        Row::Info { label: "결과".into(), value: "업로드 완료 · 공개 (106 명, 점수 12345)".into() },
        Row::Note("https://key-amas.cxx.co.kr/#/u/abcd1234".into()),
        Row::Heading("4  내 페이지".into()),
        button("upload:open", "내 페이지", "열기", false),
        button("upload:copy", "주소", "복사", false),
    ]);
    let folded = card(vec![section(false)]);
    let settings = card(vec![
        Row::Heading("표시 설정".into()),
        button("ingame", "인게임 표시", "켜짐", true),
        button("place:mission_info", "임무 카드", "게임 밖 · 100%", false),
        button("place:portrait_icons", "초상화 아이콘", "켜짐", true),
    ]);
    for (name, state) in [("open", open), ("folded", folded), ("settings", settings)] {
        render_panel(PANEL_BASE_W as u32, &state, &fonts).save(dir.join(format!("card-{name}.png"))).unwrap();
    }
}
