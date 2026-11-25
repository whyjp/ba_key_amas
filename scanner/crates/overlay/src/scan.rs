//! The roster scan as rows of the docked card (there is no separate banner).
use crate::panel::{PanelState, Row};
use crate::render::{BannerState, Phase};

/// Title and rows for a scan in `state`. `locked`: the player's mouse and keyboard are blocked
/// while it runs, which the card must say, together with the way out.
pub fn scan_rows(state: &BannerState, locked: bool) -> (String, Vec<Row>) {
    let title = match state.phase {
        Phase::Counting => "학생 수를 확인하는 중",
        Phase::Scanning => "학생 정보를 스캔하는 중",
        Phase::Done => "스캔을 마쳤습니다",
        Phase::Failed => "스캔을 중단했습니다",
    };
    let text = match (&state.phase, state.total) {
        (Phase::Counting, _) => format!("{}명 확인", state.done),
        (_, Some(t)) => format!("{}/{}", state.done, t),
        (_, None) => format!("{}명", state.done),
    };
    let mut rows = vec![Row::Progress { label: "진행".into(), done: state.done, total: state.total, text }];
    match state.phase {
        Phase::Counting | Phase::Scanning if locked => {
            rows.push(Row::Note("마우스와 키보드가 잠겨 있습니다. ESC를 누르면 스캔을 멈추고 직접 조작할 수 있습니다.".into()));
        }
        Phase::Counting | Phase::Scanning => rows.push(Row::Note("작업 중입니다. 완료될 때까지 게임을 조작하지 마세요.".into())),
        Phase::Failed if !state.note.is_empty() => rows.push(Row::Note(state.note.clone())),
        _ => {}
    }
    (title.to_string(), rows)
}

/// The whole card for a scan on its own (the command-line scan has no menu around it).
pub fn scan_card(state: &BannerState, locked: bool) -> PanelState {
    let (title, rows) = scan_rows(state, locked);
    PanelState { scene: "학생 스캔".into(), title, rows, ..Default::default() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_running_scan_says_how_far_it_is_and_how_to_get_control_back() {
        let (title, rows) = scan_rows(&BannerState::scanning(12, Some(106)), true);
        assert_eq!(title, "학생 정보를 스캔하는 중");
        assert_eq!(rows[0], Row::Progress { label: "진행".into(), done: 12, total: Some(106), text: "12/106".into() });
        assert!(matches!(&rows[1], Row::Note(n) if n.contains("ESC") && n.contains("직접 조작")), "{rows:?}");
        let (_, unlocked) = scan_rows(&BannerState::scanning(12, Some(106)), false);
        assert!(matches!(&unlocked[1], Row::Note(n) if !n.contains("ESC")));
    }

    #[test]
    fn counting_a_stop_and_a_finish_read_naturally() {
        assert_eq!(scan_rows(&BannerState::counting(40), false).1[0], Row::Progress { label: "진행".into(), done: 40, total: None, text: "40명 확인".into() });
        let (title, rows) = scan_rows(&BannerState::failed(30, Some(106), "ESC로 중단했습니다"), false);
        assert_eq!(title, "스캔을 중단했습니다");
        assert!(matches!(&rows[1], Row::Note(n) if n == "ESC로 중단했습니다"));
        let (title, rows) = scan_rows(&BannerState::finished(106, Some(106)), true);
        assert_eq!((title.as_str(), rows.len()), ("스캔을 마쳤습니다", 1), "no lock note once it is over");
    }
}
