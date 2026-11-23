//! Schedule regions and the total rank from the `Location Select` list.
//!
//! A region is one row: a `RANK n` box with the region name to its right on the same line. The total is the
//! `RANK n` box next to the `지역 랭크 총합` label. The list scrolls, so a read returns the rows visible and the
//! caller merges reads by region name.
use crate::geom::{cx, cy, h, squashed, text, w};
use crate::schema::{Region, Schedule};
use crate::Read;
use baocr_core::ocr_api::TextBox;
use regex::Regex;

fn rank_of(b: &TextBox) -> Option<u32> {
    let re = Regex::new(r"(?i)^RANK\s*(\d{1,3})$").unwrap();
    re.captures(text(b))?[1].parse().ok()
}

pub fn parse_schedule(boxes: &[TextBox]) -> Read<Schedule> {
    let mut warnings = Vec::new();
    let ranks: Vec<(usize, u32)> = boxes.iter().enumerate().filter_map(|(i, b)| Some((i, rank_of(b)?))).collect();

    let total_rank = boxes.iter().find(|b| squashed(b).contains("랭크총합")).and_then(|label| {
        ranks
            .iter()
            .map(|&(i, r)| (i, r, (cx(&boxes[i]) - cx(label)).abs() + (cy(&boxes[i]) - cy(label)).abs()))
            .filter(|&(i, _, _)| near_label(label, &boxes[i]))
            .min_by(|a, b| a.2.total_cmp(&b.2))
            .map(|(i, r, _)| (i, r))
    });
    if total_rank.is_none() {
        warnings.push("schedule: no total rank next to the 지역 랭크 총합 label".into());
    }

    let mut regions = Vec::new();
    let mut rows: Vec<&(usize, u32)> = ranks.iter().filter(|(i, _)| Some(*i) != total_rank.map(|t| t.0)).collect();
    rows.sort_by(|a, b| cy(&boxes[a.0]).total_cmp(&cy(&boxes[b.0])));
    for &&(i, rank) in &rows {
        let rb = &boxes[i];
        let name = boxes
            .iter()
            .filter(|b| b.x0 >= rb.x1 - 0.2 * w(rb) && (cy(b) - cy(rb)).abs() <= 0.5 * h(rb) && squashed(b) != "REWARD" && rank_of(b).is_none())
            .min_by(|a, b| a.x0.total_cmp(&b.x0));
        match name {
            Some(n) => regions.push(Region { name: text(n).to_string(), rank: Some(rank) }),
            None => warnings.push(format!("schedule: RANK {rank} row has no region name, skipped")),
        }
    }
    Read { value: Schedule { regions, total_rank: total_rank.map(|t| t.1) }, warnings }
}

/// The total sits right under or right of its label, within about two label heights.
fn near_label(label: &TextBox, b: &TextBox) -> bool {
    (cy(b) - cy(label)).abs() <= 2.0 * h(label) && (cx(b) - cx(label)).abs() <= 1.5 * w(label)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::boxes;

    #[test]
    fn visible_rows_and_total() {
        let r = parse_schedule(&boxes("w169-schedule"));
        let rows: Vec<(String, Option<u32>)> = r.value.regions.iter().map(|x| (x.name.clone(), x.rank)).collect();
        assert_eq!(
            rows,
            vec![
                ("살레업무관".to_string(), Some(6)),
                ("살레생활관".to_string(), Some(5)),
                ("게헨나 중앙구".to_string(), Some(5)),
                ("아비도스 본관".to_string(), Some(6)),
                ("밀레니엄 학습관".to_string(), Some(5)),
            ]
        );
        assert_eq!(r.value.total_rank, Some(56));
        assert!(r.warnings.is_empty());
    }

    #[test]
    fn a_screen_without_the_list_reads_nothing() {
        let r = parse_schedule(&boxes("w169-lobby"));
        assert!(r.value.regions.is_empty());
        assert_eq!(r.value.total_rank, None);
        assert_eq!(r.warnings.len(), 1);
    }
}
