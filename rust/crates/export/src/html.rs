//! HTML pages. Every dynamic string goes through `esc` (same character set as Go's html/template).
use crate::*;
use baocr_core::model::{RosterReport, Student};

pub(crate) fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&#34;"),
            '\'' => o.push_str("&#39;"),
            '+' => o.push_str("&#43;"),
            '\0' => o.push('\u{FFFD}'),
            c => o.push(c),
        }
    }
    o
}

const STUDENT_HEAD: &str = r#"<!doctype html><html lang="ko"><meta charset="utf-8"><title>Blue Archive · "#;
const STUDENT_STYLE: &str = r#"</title><style>body{font:16px system-ui;margin:32px;background:#eef6fb;color:#17354d}main{max-width:1100px;margin:auto}section{background:white;padding:24px;border-radius:16px;box-shadow:0 4px 20px #ccd}table{border-collapse:collapse;width:100%}th,td{text-align:left;padding:12px;border-bottom:1px solid #ddd}th{width:38%}img{max-width:100%;height:auto;margin-top:20px}small{color:#555}</style><main><h1>"#;
const STUDENT_TAIL: &str = r#"</table><small>빈 값은 이미지에서 확정하지 못한 항목입니다. 원본 캡처로 검토하세요.</small><h2>증거 화면</h2><img src="detail.png" alt="학생 상세 화면"></section></main></html>"#;

pub(crate) fn student_page(s: &Student) -> String {
    let mut f: Vec<(String, String)> = vec![
        ("학생".into(), s.name.clone()),
        ("레벨".into(), cell(s.level)),
        ("성급".into(), cell(s.rarity_stars)),
        ("EX 스킬".into(), cell(s.ex_skill_level)),
        ("기본 스킬".into(), cell(s.normal_skill_level)),
        ("강화 스킬".into(), cell(s.passive_skill_level)),
        ("서브 스킬".into(), cell(s.sub_skill_level)),
        ("인연 랭크".into(), cell(s.bond_rank)),
        ("전용무기 성급".into(), cell(s.unique_weapon_stars)),
        ("전용무기 레벨".into(), cell(s.unique_weapon_level)),
        ("역할".into(), s.combat_role.clone()),
        ("핀 설정".into(), pin_cell(s.is_pinned).into()),
    ];
    for m in meta_columns(s) {
        f.push((m.label.into(), m.value));
    }
    for i in 0..4 {
        let label = if i == 3 { "고유 장비(4번)".to_string() } else { format!("장비 {}", i + 1) };
        let eq = equipment_at(s, i);
        f.push((format!("{label} 착용"), equipped_cell(eq.equipped).into()));
        f.push((format!("{label} 종류"), string_cell(&eq.kind)));
        f.push((format!("{label} 아이템"), string_cell(&eq.item_name)));
        f.push((format!("{label} 티어"), cell(eq.tier)));
        f.push((format!("{label} 레벨"), cell(eq.level)));
    }
    let name = esc(&s.name);
    let mut o = format!("{STUDENT_HEAD}{name}{STUDENT_STYLE}{name} · 학생 메타정보</h1><section><table>");
    for (k, v) in &f {
        o.push_str(&format!("<tr><th>{}</th><td>{}</td></tr>", esc(k), esc(v)));
    }
    o.push_str(STUDENT_TAIL);
    o
}

const REPORT_HEAD: &str = r#"<!doctype html><html lang="ko"><meta charset="utf-8"><title>학생 OCR 수집</title>
<style>body{font:14px system-ui;margin:24px;background:#eef6fb;color:#17354d}table{border-collapse:collapse;background:#fff;width:100%}td,th{border-bottom:1px solid #ccd;padding:8px;text-align:left;white-space:nowrap}th{position:sticky;top:0;background:#d8effb}.wrap{overflow:auto;max-height:80vh}a{color:#075987}input,select{padding:7px;margin:6px}tr[hidden]{display:none}</style>
<h1>보유 학생 OCR 수집</h1>"#;

const REPORT_FILTERS: &str = r#"
<div><input id="q" aria-label="학생 또는 아이템 검색" placeholder="학생·아이템 검색">
<select id="role" aria-label="역할 필터"><option value="">전체 역할</option><option>STRIKER</option><option>SPECIAL</option><option value="unknown">미확인</option></select>
<select id="pin" aria-label="핀 필터"><option value="">전체 핀</option><option value="true">고정</option><option value="false">미고정</option><option value="unknown">미확인</option></select>
<select id="outfit" aria-label="복장 필터"><option value="">전체 복장</option></select>
<select id="tactic" aria-label="포지션 필터"><option value="">전체 포지션</option><option>딜러</option><option>탱커</option><option>힐러</option><option>서포터</option></select>
<select id="attack" aria-label="공격 타입 필터"><option value="">전체 공격 타입</option><option>폭발</option><option>관통</option><option>신비</option><option>진동</option></select>
<select id="armor" aria-label="방어 타입 필터"><option value="">전체 방어 타입</option><option>경장갑</option><option>중장갑</option><option>특수장갑</option><option>탄력장갑</option><option>복합장갑</option></select>
<label><input type="checkbox" id="group"> 원본 이름별로 묶기</label></div>
<p id="summary"></p><div class="wrap"><table><thead><tr><th>상태</th><th>학생</th><th>원본 이름</th><th>복장</th><th>역할</th><th>핀</th><th>포지션</th><th>공격</th><th>방어</th><th>체력</th><th>공격력</th><th>방어력</th><th>치유력</th><th>레벨</th><th>성급</th><th>EX</th><th>기본</th><th>강화</th><th>서브</th><th>인연</th><th>전용무기</th><th>장비 1</th><th>장비 2</th><th>장비 3</th><th>고유 장비</th></tr></thead><tbody id="rows">"#;

const REPORT_TAIL: &str = r#"</tbody></table></div>
<p>빈 값은 화면에서 확정하지 못한 항목입니다. 각 학생 상세 페이지에서 캡처를 확인하세요.</p>
<script>const body=document.querySelector('#rows'),rows=[...body.querySelectorAll('tr')],orig=rows.slice(),$=id=>document.querySelector('#'+id),summary=$('summary');
for(const o of [...new Set(rows.map(r=>r.dataset.outfit).filter(Boolean))].sort())$('outfit').add(new Option(o,o));
function update(){const counts={STRIKER:0,SPECIAL:0,unknown:0};let n=0;const q=$('q').value.trim().toLocaleLowerCase();
const order=$('group').checked?[...orig].sort((a,b)=>a.dataset.base.localeCompare(b.dataset.base,'ko')):orig;order.forEach(r=>body.appendChild(r));
for(const row of rows){const d=row.dataset,r=d.role||'unknown';counts[r]=(counts[r]||0)+1;
const ok=(!$('role').value||$('role').value===r)&&(!$('pin').value||$('pin').value===d.pin)&&(!$('outfit').value||$('outfit').value===d.outfit)&&(!$('tactic').value||$('tactic').value===d.tactic)&&(!$('attack').value||$('attack').value===d.attack)&&(!$('armor').value||$('armor').value===d.armor)&&row.textContent.toLocaleLowerCase().includes(q);
row.hidden=!ok;if(ok)n++}
summary.textContent='표시 '+n+'/'+rows.length+' · STRIKER '+counts.STRIKER+' · SPECIAL '+counts.SPECIAL+' · 미확인 '+counts.unknown}
for(const id of ['q','role','pin','outfit','tactic','attack','armor','group'])$(id).addEventListener('input',update);update()</script>"#;

pub(crate) fn report_page(r: &RosterReport) -> String {
    let mut o = String::from(REPORT_HEAD);
    o.push_str(&format!(
        "<p>수집 {}명 · 전체 순회: {} · 종료 사유: {}</p>",
        r.entries.len(),
        if r.complete { "완료" } else { "미확인" },
        esc(&r.stop_reason)
    ));
    o.push_str(REPORT_FILTERS);
    for e in &r.entries {
        let s = &e.student;
        let outfit = if s.identity.outfit.is_empty() { "기본" } else { s.identity.outfit.as_str() };
        o.push_str(&format!(
            r#"<tr data-role="{}" data-pin="{}" data-base="{}" data-outfit="{}" data-tactic="{}" data-attack="{}" data-armor="{}"><td>{}</td><td><a href="{}/student.html">{}</a></td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td>"#,
            esc(&s.combat_role),
            pin_key(s.is_pinned),
            esc(&s.identity.base_name),
            esc(outfit),
            esc(&s.loadout.tactic_role),
            esc(&s.loadout.attack_type),
            esc(&s.loadout.armor_type),
            esc(&e.status),
            esc(&e.detail_path),
            esc(&s.name),
            esc(&s.identity.base_name),
            esc(outfit),
            esc(&s.combat_role),
            pin_cell(s.is_pinned),
            esc(&s.loadout.tactic_role),
            esc(&s.loadout.attack_type),
            esc(&s.loadout.armor_type),
        ));
        for n in [
            s.stats.max_hp,
            s.stats.attack,
            s.stats.defense,
            s.stats.healing,
            s.level,
            s.rarity_stars,
            s.ex_skill_level,
            s.normal_skill_level,
            s.passive_skill_level,
            s.sub_skill_level,
            s.bond_rank,
            s.unique_weapon_level,
        ] {
            o.push_str(&format!("<td>{}</td>", cell(n)));
        }
        for q in &s.equipment {
            o.push_str(&format!(
                "<td>{} {} T{} Lv.{}</td>",
                equipped_cell(q.equipped),
                esc(&string_cell(&q.item_name)),
                cell(q.tier),
                cell(q.level)
            ));
        }
        o.push_str("</tr>");
    }
    o.push_str(REPORT_TAIL);
    o
}
