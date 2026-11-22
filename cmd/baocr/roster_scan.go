package main

import (
	"crypto/sha256"
	"encoding/csv"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"html/template"
	"image"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"time"
)

type RosterEntry struct {
	ID         string  `json:"id"`
	Page       int     `json:"page"`
	Column     int     `json:"column"`
	Status     string  `json:"status"`
	Student    Student `json:"student"`
	DetailPath string  `json:"detail_path,omitempty"`
}
type RosterReport struct {
	SchemaVersion int           `json:"schema_version"`
	Complete      bool          `json:"complete"`
	StopReason    string        `json:"stop_reason"`
	PagesScanned  int           `json:"pages_scanned"`
	Entries       []RosterEntry `json:"entries"`
	Unresolved    []string      `json:"unresolved"`
	Account       *Account      `json:"account,omitempty"`
	Capture       *Capture      `json:"capture,omitempty"`
}

func (h HCU) scroll(x, y float64, delta int) error {
	payload := fmt.Sprintf(`{"actions":[{"type":"scroll","x":%.2f,"y":%.2f,"scroll_y":%d}]}`, x, y, delta)
	r, err := h.call("gui", "--json", payload, "--records")
	if err != nil {
		return err
	}
	if !r.InputEmitted || r.Completed != 1 {
		return fmt.Errorf("HCU scroll dispatch unconfirmed: %+v", r)
	}
	return nil
}
func parseFrameRegion(text string) ([4]float64, error) {
	var region [4]float64
	parts := strings.Split(text, ",")
	if len(parts) != 4 {
		return region, errors.New("--region must have four comma-separated frame coordinates")
	}
	for i, p := range parts {
		v, err := strconv.ParseFloat(p, 64)
		if err != nil {
			return region, err
		}
		region[i] = v
	}
	if region[0] >= region[2] || region[1] >= region[3] {
		return region, errors.New("--region must have positive width and height")
	}
	return region, nil
}
func toFrame(region [4]float64, x, y int) (float64, float64) {
	return region[0] + float64(x)*(region[2]-region[0])/1280, region[1] + float64(y)*(region[3]-region[1])/960
}
func rosterVisible(img image.Image, tesseract, tessdata string) (bool, error) {
	// The selected All tab, six-card grid and narrow scrollbar must all be
	// present. Korean title OCR alone varied between live frames.
	if detailVisible(img) { // a profile page can never be the roster
		return false, nil
	}
	thumb := scrollThumb(img)
	if thumb.Top < 179 || thumb.Bottom-thumb.Top < 45 || thumb.Bottom-thumb.Top > 90 || thumb.Bottom > 945 || len(rosterCards(img)) < 6 {
		return false, nil
	}
	r, g, b, _ := img.At(395, 110).RGBA()
	return (g>>8) > (r>>8)+24 && (b>>8) > (g>>8)+20, nil
}
func cardID(card Card) string {
	hash := sha256.Sum256(card.Signature[:])
	return hex.EncodeToString(hash[:8])
}
func saveReport(report RosterReport, out string) error {
	if err := os.MkdirAll(out, 0755); err != nil {
		return err
	}
	data, err := json.MarshalIndent(report, "", "  ")
	if err != nil {
		return err
	}
	if err = os.WriteFile(filepath.Join(out, "students.json"), data, 0644); err != nil {
		return err
	}
	f, err := os.Create(filepath.Join(out, "students.csv"))
	if err != nil {
		return err
	}
	w := csv.NewWriter(f)
	w.UseCRLF = true
	var header []string
	if len(report.Entries) > 0 {
		header, _ = studentCSV(report.Entries[0].Student)
	} else {
		header, _ = studentCSV(Student{})
	}
	header = append([]string{"id", "page", "status", "detail_path"}, header...)
	if err = w.Write(header); err != nil {
		_ = f.Close()
		return err
	}
	for _, entry := range report.Entries {
		_, row := studentCSV(entry.Student)
		row = append([]string{entry.ID, fmt.Sprint(entry.Page), entry.Status, entry.DetailPath}, row...)
		if err = w.Write(row); err != nil {
			_ = f.Close()
			return err
		}
	}
	w.Flush()
	err = w.Error()
	if closeErr := f.Close(); err == nil {
		err = closeErr
	}
	if err != nil {
		return err
	}
	const page = `<!doctype html><html lang="ko"><meta charset="utf-8"><title>학생 OCR 수집</title>
<style>body{font:14px system-ui;margin:24px;background:#eef6fb;color:#17354d}table{border-collapse:collapse;background:#fff;width:100%}td,th{border-bottom:1px solid #ccd;padding:8px;text-align:left;white-space:nowrap}th{position:sticky;top:0;background:#d8effb}.wrap{overflow:auto;max-height:80vh}a{color:#075987}input,select{padding:7px;margin:6px}tr[hidden]{display:none}</style>
<h1>보유 학생 OCR 수집</h1><p>수집 {{len .Entries}}명 · 전체 순회: {{if .Complete}}완료{{else}}미확인{{end}} · 종료 사유: {{.StopReason}}</p>
<div><input id="q" aria-label="학생 또는 아이템 검색" placeholder="학생·아이템 검색">
<select id="role" aria-label="역할 필터"><option value="">전체 역할</option><option>STRIKER</option><option>SPECIAL</option><option value="unknown">미확인</option></select>
<select id="pin" aria-label="핀 필터"><option value="">전체 핀</option><option value="true">고정</option><option value="false">미고정</option><option value="unknown">미확인</option></select>
<select id="outfit" aria-label="복장 필터"><option value="">전체 복장</option></select>
<select id="tactic" aria-label="포지션 필터"><option value="">전체 포지션</option><option>딜러</option><option>탱커</option><option>힐러</option><option>서포터</option></select>
<select id="attack" aria-label="공격 타입 필터"><option value="">전체 공격 타입</option><option>폭발</option><option>관통</option><option>신비</option><option>진동</option></select>
<select id="armor" aria-label="방어 타입 필터"><option value="">전체 방어 타입</option><option>경장갑</option><option>중장갑</option><option>특수장갑</option><option>탄력장갑</option><option>복합장갑</option></select>
<label><input type="checkbox" id="group"> 원본 이름별로 묶기</label></div>
<p id="summary"></p><div class="wrap"><table><thead><tr><th>상태</th><th>학생</th><th>원본 이름</th><th>복장</th><th>역할</th><th>핀</th><th>포지션</th><th>공격</th><th>방어</th><th>체력</th><th>공격력</th><th>방어력</th><th>치유력</th><th>레벨</th><th>성급</th><th>EX</th><th>기본</th><th>강화</th><th>서브</th><th>인연</th><th>전용무기</th><th>장비 1</th><th>장비 2</th><th>장비 3</th><th>고유 장비</th></tr></thead><tbody id="rows">{{range .Entries}}<tr data-role="{{.Student.CombatRole}}" data-pin="{{pinKey .Student.IsPinned}}" data-base="{{.Student.Identity.Base}}" data-outfit="{{if .Student.Identity.Outfit}}{{.Student.Identity.Outfit}}{{else}}기본{{end}}" data-tactic="{{.Student.Loadout.TacticRole}}" data-attack="{{.Student.Loadout.AttackType}}" data-armor="{{.Student.Loadout.ArmorType}}"><td>{{.Status}}</td><td><a href="{{.DetailPath}}/student.html">{{.Student.Name}}</a></td><td>{{.Student.Identity.Base}}</td><td>{{if .Student.Identity.Outfit}}{{.Student.Identity.Outfit}}{{else}}기본{{end}}</td><td>{{.Student.CombatRole}}</td><td>{{pin .Student.IsPinned}}</td><td>{{.Student.Loadout.TacticRole}}</td><td>{{.Student.Loadout.AttackType}}</td><td>{{.Student.Loadout.ArmorType}}</td><td>{{cell .Student.Stats.MaxHP}}</td><td>{{cell .Student.Stats.Attack}}</td><td>{{cell .Student.Stats.Defense}}</td><td>{{cell .Student.Stats.Healing}}</td><td>{{cell .Student.Level}}</td><td>{{cell .Student.RarityStars}}</td><td>{{cell .Student.EXSkillLevel}}</td><td>{{cell .Student.BasicSkillLevel}}</td><td>{{cell .Student.PassiveSkillLevel}}</td><td>{{cell .Student.SubSkillLevel}}</td><td>{{cell .Student.BondRank}}</td><td>{{cell .Student.UniqueWeaponLevel}}</td>{{range .Student.Equipment}}<td>{{equipped .Equipped}} {{str .ItemName}} T{{cell .Tier}} Lv.{{cell .Level}}</td>{{end}}</tr>{{end}}</tbody></table></div>
<p>빈 값은 화면에서 확정하지 못한 항목입니다. 각 학생 상세 페이지에서 캡처를 확인하세요.</p>
<script>const body=document.querySelector('#rows'),rows=[...body.querySelectorAll('tr')],orig=rows.slice(),$=id=>document.querySelector('#'+id),summary=$('summary');
for(const o of [...new Set(rows.map(r=>r.dataset.outfit).filter(Boolean))].sort())$('outfit').add(new Option(o,o));
function update(){const counts={STRIKER:0,SPECIAL:0,unknown:0};let n=0;const q=$('q').value.trim().toLocaleLowerCase();
const order=$('group').checked?[...orig].sort((a,b)=>a.dataset.base.localeCompare(b.dataset.base,'ko')):orig;order.forEach(r=>body.appendChild(r));
for(const row of rows){const d=row.dataset,r=d.role||'unknown';counts[r]=(counts[r]||0)+1;
const ok=(!$('role').value||$('role').value===r)&&(!$('pin').value||$('pin').value===d.pin)&&(!$('outfit').value||$('outfit').value===d.outfit)&&(!$('tactic').value||$('tactic').value===d.tactic)&&(!$('attack').value||$('attack').value===d.attack)&&(!$('armor').value||$('armor').value===d.armor)&&row.textContent.toLocaleLowerCase().includes(q);
row.hidden=!ok;if(ok)n++}
summary.textContent='표시 '+n+'/'+rows.length+' · STRIKER '+counts.STRIKER+' · SPECIAL '+counts.SPECIAL+' · 미확인 '+counts.unknown}
for(const id of ['q','role','pin','outfit','tactic','attack','armor','group'])$(id).addEventListener('input',update);update()</script>`
	t := template.Must(template.New("roster").Funcs(template.FuncMap{"cell": cell, "equipped": equippedCell, "str": stringCell, "pinKey": pinKey, "pin": pinCell}).Parse(page))
	f, err = os.Create(filepath.Join(out, "students.html"))
	if err != nil {
		return err
	}
	defer f.Close()
	return t.Execute(f, report)
}
func scanRoster(h HCU, tesseract, tessdata string, region [4]float64, out string, maxPages, maxStudents int, rosterOnly bool) (RosterReport, error) {
	report := RosterReport{SchemaVersion: 2, Entries: []RosterEntry{}, Unresolved: []string{}, StopReason: "running"}
	if err := os.MkdirAll(out, 0755); err != nil {
		return report, err
	}
	if err := h.activate(); err != nil {
		return report, err
	}
	img, err := gameCapture(h, region)
	if err != nil {
		return report, err
	}
	valid, err := rosterVisible(img, tesseract, tessdata)
	if err != nil {
		return report, err
	}
	if !valid {
		return report, errors.New("student roster All view not visible; no input sent")
	}
	if thumb := scrollThumb(img); thumb.Top < 179 || thumb.Top > 190 {
		return report, fmt.Errorf("roster must start at top, scrollbar=%+v; no input sent", thumb)
	}
	var seen [][768]uint8
	for page := 1; page <= maxPages; page++ {
		valid, err = rosterVisible(img, tesseract, tessdata)
		if err != nil {
			return report, err
		}
		if !valid {
			report.StopReason = "roster disappeared"
			_ = saveReport(report, out)
			return report, errors.New(report.StopReason)
		}
		cards := rosterCards(img)
		thumb := scrollThumb(img)
		report.PagesScanned = page
		if err = savePNG(filepath.Join(out, fmt.Sprintf("roster-%03d.png", page)), img); err != nil {
			return report, err
		}
		fmt.Printf("page %d: cards=%d thumb=%+v collected=%d\n", page, len(cards), thumb, len(report.Entries))
		for _, card := range cards {
			known := false
			for _, s := range seen {
				if signatureDistance(s, card.Signature) < 2 {
					known = true
					break
				}
			}
			if known {
				continue
			}
			// Save the card fingerprint before the click; same card on the next page
			// must not be collected twice even if its name OCR varies.
			seen = append(seen, card.Signature)
			id := cardID(card)
			entry := RosterEntry{ID: id, Page: page, Column: card.Column, Status: "pending", DetailPath: fmt.Sprintf("details/%03d-%s", len(report.Entries)+1, id)}
			report.Entries = append(report.Entries, entry)
			idx := len(report.Entries) - 1
			if rosterOnly {
				name, e := ocr(tesseract, tessdata, img, image.Rect(card.X-83, card.Y+100, card.X+83, card.Y+155), "kor+eng", 7)
				if e != nil {
					return report, e
				}
				report.Entries[idx].Student = Student{Name: clean(name), Source: fmt.Sprintf("roster page %d", page)}
				report.Entries[idx].Status = "roster_only"
			} else {
				x, y := toFrame(region, card.X, card.Y)
				if err = h.click(x, y); err != nil {
					report.StopReason = "click failed"
					_ = saveReport(report, out)
					return report, err
				}
				time.Sleep(500 * time.Millisecond)
				detail, e := gameCapture(h, region)
				if e != nil {
					report.StopReason = "detail capture failed"
					_ = saveReport(report, out)
					return report, e
				}
				stillRoster, e := rosterVisible(detail, tesseract, tessdata)
				if e != nil {
					return report, e
				}
				if stillRoster {
					report.StopReason = "card click did not open detail"
					_ = saveReport(report, out)
					return report, errors.New(report.StopReason)
				}
				student, e := extract(detail, tesseract, tessdata, fmt.Sprintf("roster page %d column %d", page, card.Column+1))
				if e != nil {
					return report, e
				}
				report.Entries[idx].Student = student
				if student.Name == "" {
					report.Entries[idx].Status = "unresolved_name"
					report.Unresolved = append(report.Unresolved, id)
				} else {
					report.Entries[idx].Status = "detail_ocr"
				}
				if err = export(student, detail, filepath.Join(out, entry.DetailPath)); err != nil {
					return report, err
				}
				bx, by := toFrame(region, 50, 40)
				if err = h.click(bx, by); err != nil {
					report.StopReason = "back click failed"
					_ = saveReport(report, out)
					return report, err
				}
				time.Sleep(400 * time.Millisecond)
				back, e := gameCapture(h, region)
				if e != nil {
					return report, e
				}
				valid, e = rosterVisible(back, tesseract, tessdata)
				if e != nil {
					return report, e
				}
				if !valid {
					report.StopReason = "roster did not return"
					_ = saveReport(report, out)
					return report, errors.New(report.StopReason)
				}
				if !sameCards(rosterCards(img), rosterCards(back)) {
					report.StopReason = "roster position changed while inspecting detail"
					_ = saveReport(report, out)
					return report, errors.New(report.StopReason)
				}
			}
			if err = saveReport(report, out); err != nil {
				return report, err
			}
			fmt.Printf("  %d %s %s\n", len(report.Entries), report.Entries[idx].Student.Name, report.Entries[idx].Status)
			if len(report.Entries) >= maxStudents {
				report.StopReason = "max_students"
				_ = saveReport(report, out)
				return report, nil
			}
		}
		if thumb.Bottom >= 940 {
			report.Complete = len(report.Unresolved) == 0
			report.StopReason = "scrollbar_end"
			_ = saveReport(report, out)
			return report, nil
		}
		if page == maxPages {
			report.StopReason = "max_pages"
			_ = saveReport(report, out)
			return report, nil
		}
		sx, sy := toFrame(region, 1150, 750)
		if err = h.scroll(sx, sy, 300); err != nil {
			report.StopReason = "scroll failed"
			_ = saveReport(report, out)
			return report, err
		}
		time.Sleep(450 * time.Millisecond)
		next, e := gameCapture(h, region)
		if e != nil {
			return report, e
		}
		nextCards := rosterCards(next)
		if sameCards(cards, nextCards) {
			nextThumb := scrollThumb(next)
			if nextThumb.Bottom >= 940 {
				report.Complete = len(report.Unresolved) == 0
				report.StopReason = "scrollbar_end"
				_ = saveReport(report, out)
				return report, nil
			}
			report.StopReason = "scroll did not move"
			_ = saveReport(report, out)
			return report, errors.New(report.StopReason)
		}
		img = next
	}
	return report, nil
}

// pinKey is the filter token for the report's pin dropdown.
func pinKey(v *bool) string {
	if v == nil {
		return "unknown"
	}
	if *v {
		return "true"
	}
	return "false"
}
