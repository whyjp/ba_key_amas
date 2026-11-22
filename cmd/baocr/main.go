package main

import (
	"bytes"
	"context"
	"encoding/csv"
	"encoding/json"
	"errors"
	"flag"
	"fmt"
	"html/template"
	"image"
	"image/color"
	"image/draw"
	"image/png"

	"math"
	"os"
	"os/exec"
	"os/signal"
	"path/filepath"
	"regexp"
	"runtime"
	"strconv"
	"strings"
	"time"

	xdraw "golang.org/x/image/draw"
)

// Student values absent from pixels stay nil, never guessed from identity.
type Student struct {
	Name              string      `json:"name"`
	Level             *int        `json:"level"`
	RarityStars       *int        `json:"rarity_stars"`
	EXSkillLevel      *int        `json:"ex_skill_level"`
	BasicSkillLevel   *int        `json:"normal_skill_level"`
	PassiveSkillLevel *int        `json:"passive_skill_level"`
	SubSkillLevel     *int        `json:"sub_skill_level"`
	BondRank          *int        `json:"bond_rank"`
	UniqueWeaponStars *int        `json:"unique_weapon_stars"`
	UniqueWeaponLevel *int        `json:"unique_weapon_level"`
	CombatRole        string      `json:"combat_role"`
	IsPinned          *bool       `json:"is_pinned"`
	NameMatch         *NameMatch  `json:"name_match,omitempty"`
	Identity          NameParts   `json:"identity"`
	Stats             Stats       `json:"stats"`
	Exp               Exp         `json:"exp"`
	Loadout           Loadout     `json:"loadout"`
	Terrain           Terrain     `json:"terrain"`
	Badges            Badges      `json:"badges"`
	Locks             Locks       `json:"locks"`
	Reference         *Reference  `json:"reference,omitempty"`
	Deep              *Deep       `json:"deep,omitempty"`
	Fixed             *FixedMeta  `json:"fixed_meta,omitempty"`
	SkillLocked       SkillLocked `json:"skill_locked"`
	// frames are the menu-pass captures, kept as evidence next to detail.png.
	frames    map[string]image.Image
	Warnings  []string          `json:"warnings,omitempty"`
	Equipment []Equipment       `json:"equipment"`
	Source    string            `json:"source"`
	RawOCR    map[string]string `json:"raw_ocr"`
}

type record struct {
	OK           bool     `json:"ok"`
	Code         string   `json:"code"`
	Hint         string   `json:"hint"`
	InputEmitted bool     `json:"input_emitted"`
	Completed    int      `json:"completed"`
	Images       []string `json:"images"`
	Screen       struct {
		Width  int `json:"width"`
		Height int `json:"height"`
	} `json:"screen"`
	Evidence struct {
		Zoom struct {
			Region []float64 `json:"region"`
			ScaleX float64   `json:"scale_x"`
			ScaleY float64   `json:"scale_y"`
		} `json:"zoom"`
	} `json:"evidence"`
}

type HCU struct{ Binary string }

func (h HCU) call(args ...string) (record, error) {
	cmd := exec.Command(h.Binary, args...)
	b, err := cmd.CombinedOutput()
	var r record
	if e := json.Unmarshal(bytes.TrimSpace(b), &r); e != nil {
		return r, fmt.Errorf("HCU %v: %w (%s)", args, e, string(b))
	}
	if err != nil || !r.OK {
		return r, fmt.Errorf("HCU %v: %s %s (%v); input_emitted=%t", args, r.Code, r.Hint, err, r.InputEmitted)
	}
	return r, nil
}
func (h HCU) screenshot() (record, error) { return h.call("gui", "screenshot", "--records") }
func (h HCU) zoom(x0, y0, x1, y1 int) (record, error) {
	return h.call("gui", "zoom", strconv.Itoa(x0), strconv.Itoa(y0), strconv.Itoa(x1), strconv.Itoa(y1), "--records")
}
func (h HCU) click(x, y float64) error {
	r, err := h.call("gui", "click", fmt.Sprintf("%.2f", x), fmt.Sprintf("%.2f", y), "--records")
	if err != nil {
		return err
	}
	if !r.InputEmitted || r.Completed != 1 {
		return fmt.Errorf("HCU click dispatch unconfirmed: %+v", r)
	}
	return nil
}

// activate targets the game window by exact title: a bare "Blue Archive" is
// ambiguous when a browser tab such as "... - Blue Archive Wiki" is open.
func (h HCU) activate() error {
	out, err := exec.Command(h.Binary, "apps").CombinedOutput()
	if err != nil {
		return fmt.Errorf("HCU apps: %w (%s)", err, out)
	}
	ref := windowRef(string(out), "Blue Archive")
	if ref == "" {
		return errors.New("no window titled exactly \"Blue Archive\"")
	}
	_, err = h.call("activate", ref, "--records")
	return err
}

// foreground fails unless the exact-titled game window is the active one, so a
// focus change can never turn a scan click into input for another window.
func (h HCU) foreground() error {
	out, err := exec.Command(h.Binary, "apps").CombinedOutput()
	if err != nil {
		return fmt.Errorf("HCU apps: %w (%s)", err, out)
	}
	if !windowActive(string(out), "Blue Archive") {
		return errors.New("Blue Archive is not the active window; refusing to send input")
	}
	return nil
}

// windowActive reports whether the window with exactly this title is marked active.
func windowActive(apps, title string) bool {
	for _, line := range strings.Split(apps, "\n") {
		f := strings.Fields(line)
		if len(f) > 1 && strings.HasPrefix(f[0], "w") && strings.HasPrefix(line[len(f[0]):], " \""+title+"\"") {
			return strings.HasSuffix(strings.TrimSpace(line), "active")
		}
	}
	return false
}

// windowRef returns the wN ref of the window whose title equals title exactly.
func windowRef(apps, title string) string {
	for _, line := range strings.Split(apps, "\n") {
		f := strings.Fields(line)
		if len(f) > 1 && strings.HasPrefix(f[0], "w") && strings.HasPrefix(line[len(f[0]):], " \""+title+"\"") {
			return f[0]
		}
	}
	return ""
}

func loadPNG(path string) (image.Image, error) {
	f, e := os.Open(path)
	if e != nil {
		return nil, e
	}
	defer f.Close()
	return png.Decode(f)
}
func savePNG(path string, img image.Image) error {
	f, e := os.Create(path)
	if e != nil {
		return e
	}
	defer f.Close()
	return png.Encode(f, img)
}
func clamp(v, lo, hi int) int {
	if v < lo {
		return lo
	}
	if v > hi {
		return hi
	}
	return v
}
func crop(img image.Image, r image.Rectangle) image.Image {
	b := img.Bounds()
	r = r.Intersect(b)
	dst := image.NewRGBA(image.Rect(0, 0, r.Dx(), r.Dy()))
	draw.Draw(dst, dst.Bounds(), img, r.Min, draw.Src)
	return dst
}
func resizeNearest(src image.Image, w, h int) image.Image {
	out := image.NewRGBA(image.Rect(0, 0, w, h))
	xdraw.CatmullRom.Scale(out, out.Bounds(), src, src.Bounds(), draw.Over, nil)
	return out
}
func gameCapture(h HCU, region [4]float64) (image.Image, error) {
	// A screenshot leases HCU GUI frame coordinates; zoom reads native detail.
	frame, err := h.screenshot()
	if err != nil {
		return nil, err
	}
	if frame.Screen.Width == 0 {
		return nil, errors.New("HCU screenshot has no frame")
	}
	x0, y0, x1, y1 := region[0], region[1], region[2], region[3]
	if x0 < 0 || y0 < 0 || x1 > float64(frame.Screen.Width) || y1 > float64(frame.Screen.Height) {
		return nil, errors.New("game region outside HCU frame")
	}
	result := image.NewRGBA(image.Rect(0, 0, 1280, 960))
	mid := (y0 + y1) / 2
	for i, pair := range [][2]float64{{y0, mid}, {mid, y1}} {
		z, e := h.zoom(int(math.Floor(x0))-2, int(math.Floor(pair[0]))-2, int(math.Ceil(x1))+2, int(math.Ceil(pair[1]))+2)
		if e != nil {
			return nil, e
		}
		if len(z.Images) == 0 || len(z.Evidence.Zoom.Region) != 4 {
			return nil, errors.New("HCU zoom missing image/mapping")
		}
		src, e := loadPNG(z.Images[0])
		if e != nil {
			return nil, e
		}
		zr := z.Evidence.Zoom.Region
		sx, sy := z.Evidence.Zoom.ScaleX, z.Evidence.Zoom.ScaleY
		rect := image.Rect(int(math.Round((x0-zr[0])*sx)), int(math.Round((pair[0]-zr[1])*sy)), int(math.Round((x1-zr[0])*sx)), int(math.Round((pair[1]-zr[1])*sy)))
		part := resizeNearest(crop(src, rect), 1280, 480)
		draw.Draw(result, image.Rect(0, i*480, 1280, (i+1)*480), part, image.Point{}, draw.Src)
	}
	return result, nil
}

var digits = regexp.MustCompile(`\d{1,3}`)

func num(text string, max int) *int {
	m := digits.FindString(text)
	if m == "" {
		return nil
	}
	n, e := strconv.Atoi(m)
	if e != nil || n < 1 || n > max {
		return nil
	}
	return &n
}
func clean(t string) string {
	return strings.TrimSpace(strings.ReplaceAll(strings.ReplaceAll(t, "\r", ""), "\n", " "))
}
func recognizePNG(tesseract, tessdata string, scaled image.Image, lang string, psm int, extra ...string) (string, error) {
	var data bytes.Buffer
	if e := png.Encode(&data, scaled); e != nil {
		return "", e
	}
	args := []string{"stdin", "stdout", "-l", lang, "--oem", "1", "--psm", strconv.Itoa(psm), "--tessdata-dir", tessdata}
	cmd := exec.Command(tesseract, append(args, extra...)...)
	cmd.Stdin = &data
	var stdout, stderr bytes.Buffer
	cmd.Stdout = &stdout
	cmd.Stderr = &stderr
	if e := cmd.Run(); e != nil {
		return "", fmt.Errorf("tesseract: %w: %s", e, stderr.String())
	}
	return clean(stdout.String()), nil
}
func ocr(tesseract, tessdata string, img image.Image, rect image.Rectangle, lang string, psm int) (string, error) {
	return ocrExtra(tesseract, tessdata, img, rect, lang, psm)
}

// ocrWhitelist restricts Tesseract to a closed alphabet (for closed vocabularies).
func ocrWhitelist(tesseract, tessdata string, img image.Image, rect image.Rectangle, lang string, psm int, chars string) (string, error) {
	return ocrExtra(tesseract, tessdata, img, rect, lang, psm, "-c", "tessedit_char_whitelist="+chars)
}

func ocrExtra(tesseract, tessdata string, img image.Image, rect image.Rectangle, lang string, psm int, extra ...string) (string, error) {
	area := crop(img, rect)
	if area.Bounds().Dx() == 0 || area.Bounds().Dy() == 0 {
		return "", nil
	}
	return recognizePNG(tesseract, tessdata, resizeNearest(area, area.Bounds().Dx()*6, area.Bounds().Dy()*6), lang, psm, extra...)
}
func value(text string, max int) *int { return num(strings.ReplaceAll(text, "O", "0"), max) }
func gold(c color.Color) bool {
	r, g, b, _ := c.RGBA()
	return r>>8 > 140 && g>>8 > 100 && b>>8 < 140 && r > g
}
func cyan(c color.Color) bool {
	r, g, b, _ := c.RGBA()
	return g>>8 > 125 && b>>8 > 140 && r>>8 < 180 && b > r+20*257
}
func pinState(img image.Image) *bool {
	// The yellow SELECT tag is fixed above the name on pinned profiles.
	// Non-pinned portraits can have a few yellow pixels, hence the gap.
	hits := 0
	for y := 762; y < 785; y++ {
		for x := 45; x < 120; x++ {
			r, g, b, _ := img.At(x, y).RGBA()
			if r>>8 > 180 && g>>8 > 120 && b>>8 < 140 && r > g+15*257 {
				hits++
			}
		}
	}
	if hits >= 450 {
		v := true
		return &v
	}
	if hits <= 150 {
		v := false
		return &v
	}
	return nil
}
func stars(img image.Image) *int {
	count := 0
	for _, x := range []int{270, 285, 300, 315, 330} {
		hit := 0
		for xx := x - 5; xx <= x+5; xx++ {
			for y := 806; y < 831; y++ {
				if gold(img.At(xx, y)) {
					hit++
				}
			}
		}
		if hit >= 24 {
			count++
		}
	}
	if count == 0 {
		return nil
	}
	return &count
}
func weaponStars(img image.Image) *int {
	// Bottom-right icons are spaced 16px. Adjacent cyan stars touch, so
	// connected-component counting incorrectly merges two stars into one.
	count := 0
	for _, x := range []int{1029, 1045, 1061, 1077} {
		hit := 0
		for xx := x - 5; xx <= x+5; xx++ {
			for y := 627; y < 650; y++ {
				if cyan(img.At(xx, y)) {
					hit++
				}
			}
		}
		if hit >= 24 {
			count++
		}
	}
	if count == 0 {
		return nil
	}
	return &count
}

// extract reads the variable stage only (see fixed.go).
func extract(img image.Image, tesseract, tessdata, source string) (Student, error) {
	return extractWith(img, tesseract, tessdata, source, Options{})
}

func extractWith(img image.Image, tesseract, tessdata, source string, opts Options) (Student, error) {
	s := Student{RawOCR: map[string]string{}, Source: source}
	regions := []struct {
		key  string
		r    image.Rectangle
		lang string
		max  int
		dest **int
	}{
		{"level", image.Rect(30, 827, 104, 865), "eng", 150, &s.Level},
		{"ex", image.Rect(694, 522, 767, 547), "eng", 20, &s.EXSkillLevel},
		{"basic", image.Rect(800, 520, 883, 548), "eng", 20, &s.BasicSkillLevel},
		{"passive", image.Rect(908, 522, 987, 547), "eng", 20, &s.PassiveSkillLevel},
		{"sub", image.Rect(1001, 516, 1115, 551), "eng", 20, &s.SubSkillLevel},
		{"weapon_level", image.Rect(766, 563, 850, 608), "eng", 100, &s.UniqueWeaponLevel},
	}
	name, e := ocr(tesseract, tessdata, img, image.Rect(64, 789, 300, 835), "kor+eng", 7)
	if e != nil {
		return s, e
	}
	s.RawOCR["name"] = name
	s.Name, s.NameMatch = readName(img, tesseract, tessdata, name, s.RawOCR)
	// All regions are English digits; read them concurrently.
	rects := make([]image.Rectangle, len(regions))
	for i, r := range regions {
		rects[i] = r.r
	}
	texts, e := ocrAll(img, tesseract, tessdata, "eng", 7, rects, "-c", "tessedit_char_whitelist=Lv.0123456789")
	if e != nil {
		return s, e
	}
	for i, r := range regions {
		s.RawOCR[r.key] = texts[i]
		*r.dest = value(texts[i], r.max)
	}
	// A locked skill shows "★N Lv.1" over a dark card: it has no level to read.
	s.SkillLocked = readSkillLocks(img)
	for _, l := range []struct {
		locked bool
		dest   **int
	}{{s.SkillLocked.EX, &s.EXSkillLevel}, {s.SkillLocked.Basic, &s.BasicSkillLevel}, {s.SkillLocked.Passive, &s.PassiveSkillLevel}, {s.SkillLocked.Sub, &s.SubSkillLevel}} {
		if l.locked {
			*l.dest = nil
		}
	}
	// Thin italic "1" often defeats the first read ("Lv.1" -> "v"): retry the
	// skill levels that came back empty with the bright-ink fallback.
	for _, r := range regions {
		if (r.key == "passive" && s.SkillLocked.Passive) || (r.key == "sub" && s.SkillLocked.Sub) || (r.key == "ex" && s.SkillLocked.EX) || (r.key == "basic" && s.SkillLocked.Basic) {
			continue
		}
		if *r.dest != nil || !strings.HasPrefix(r.key, "ex") && r.key != "basic" && r.key != "passive" && r.key != "sub" {
			continue
		}
		mid := (r.r.Min.X + r.r.Max.X) / 2
		inner := image.Rect(mid-35, r.r.Min.Y+4, mid+35, r.r.Max.Y-3)
		v, e := ocrBright(tesseract, tessdata, img, inner, 170, 1, "Lv.0123456789")
		if e != nil {
			return s, e
		}
		s.RawOCR[r.key+"_retry"] = v
		*r.dest = value(v, r.max)
	}
	bond, e := readHeartNumber(img, tesseract, tessdata)
	if e != nil {
		return s, e
	}
	s.RawOCR["bond"] = bond
	s.BondRank = value(bond, 100)
	if s.BondRank == nil && strings.TrimSpace(bond) != "" {
		// Digits were read but fall outside 1..100 (e.g. a 4-digit read of a
		// 3-digit heart): keep it null and say so instead of guessing.
		s.Warnings = append(s.Warnings, "bond_rank_implausible: raw="+strings.TrimSpace(bond))
	}
	if s.UniqueWeaponLevel == nil {
		v, e := ocr(tesseract, tessdata, img, image.Rect(775, 575, 829, 602), "eng", 13)
		if e != nil {
			return s, e
		}
		s.RawOCR["weapon_level_retry"] = v
		// A label-free duplicated digit (for example R77) is ambiguous, not
		// evidence for weapon level 77. Prefer null over a fabricated value.
		d := digits.FindString(v)
		if !(len(d) == 2 && d[0] == d[1] && !strings.Contains(strings.ToLower(v), "lv")) {
			s.UniqueWeaponLevel = value(v, 100)
		}
	}
	role, e := ocr(tesseract, tessdata, img, image.Rect(332, 803, 422, 832), "eng", 7)
	if e != nil {
		return s, e
	}
	if strings.Contains(strings.ToUpper(role), "STRIKER") {
		s.CombatRole = "STRIKER"
	} else if strings.Contains(strings.ToUpper(role), "SPECIAL") {
		s.CombatRole = "SPECIAL"
	}
	if s.Stats, e = readStats(img, tesseract, tessdata, s.RawOCR); e != nil {
		return s, e
	}
	if s.Exp, e = readExp(img, tesseract, tessdata, s.RawOCR); e != nil {
		return s, e
	}
	levelFromExp(&s)
	s.Badges, s.Locks = readBadges(img), readLocks(img)
	s.Identity = parseName(s.Name)
	if opts.VerifyFixed {
		// Audit mode: read the fixed groups from pixels and compare with the dictionary.
		if s.Loadout, e = readLoadout(img, tesseract, tessdata, s.RawOCR); e != nil {
			return s, e
		}
		s.Terrain = readTerrain(img)
		s.Reference = defaultResolver.lookupReference(s.Name, s.Loadout)
		defaultResolver.crossCheck(&s)
		applyFixed(&s, img)
		if s.Fixed != nil {
			s.Fixed.Source = "pixels+dictionary"
		}
	} else {
		applyFixed(&s, img)
		s.Reference = defaultResolver.lookupReference(s.Name, s.Loadout)
	}
	// Dictionary role is a cross-check only; it never fills CombatRole.
	if want := defaultResolver.RoleOf(s.Name); want != "" && s.CombatRole != "" && want != s.CombatRole {
		s.Warnings = append(s.Warnings, "role_mismatch: pixels="+s.CombatRole+" dictionary="+want)
	}
	s.RarityStars = stars(img)
	s.IsPinned = pinState(img)
	s.UniqueWeaponStars = weaponStars(img)
	var equipmentOCR map[string]string
	s.Equipment, equipmentOCR, e = detectEquipment(img, tesseract, tessdata)
	if e != nil {
		return s, e
	}
	for k, v := range equipmentOCR {
		s.RawOCR[k] = v
	}
	return s, nil
}
func cell(n *int) string {
	if n == nil {
		return ""
	}
	return strconv.Itoa(*n)
}
func stringCell(s *string) string {
	if s == nil {
		return ""
	}
	return *s
}
func equippedCell(v *bool) string {
	if v == nil {
		return "미확인"
	}
	if *v {
		return "착용"
	}
	return "미착용"
}
func pinCell(v *bool) string {
	if v == nil {
		return "미확인"
	}
	if *v {
		return "고정"
	}
	return "미고정"
}
func equipmentAt(s Student, i int) Equipment {
	if i < len(s.Equipment) {
		return s.Equipment[i]
	}
	return Equipment{Slot: i + 1}
}
func studentCSV(s Student) ([]string, []string) {
	header := []string{"name", "level", "rarity_stars", "ex_skill_level", "normal_skill_level", "passive_skill_level", "sub_skill_level", "bond_rank", "unique_weapon_stars", "unique_weapon_level", "combat_role", "is_pinned"}
	pin := ""
	if s.IsPinned != nil {
		pin = strconv.FormatBool(*s.IsPinned)
	}
	row := []string{s.Name, cell(s.Level), cell(s.RarityStars), cell(s.EXSkillLevel), cell(s.BasicSkillLevel), cell(s.PassiveSkillLevel), cell(s.SubSkillLevel), cell(s.BondRank), cell(s.UniqueWeaponStars), cell(s.UniqueWeaponLevel), s.CombatRole, pin}
	for _, m := range metaColumns(s) {
		header, row = append(header, m.Key), append(row, m.Value)
	}
	for i := 0; i < 4; i++ {
		prefix := fmt.Sprintf("equipment_%d_", i+1)
		header = append(header, prefix+"equipped", prefix+"type", prefix+"item_name", prefix+"tier", prefix+"level")
		eq := equipmentAt(s, i)
		state := ""
		if eq.Equipped != nil {
			state = strconv.FormatBool(*eq.Equipped)
		}
		row = append(row, state, stringCell(eq.Type), stringCell(eq.ItemName), cell(eq.Tier), cell(eq.Level))
	}
	return header, row
}

// exportStudent writes student.json/csv/html; the evidence frames already exist.
func exportStudent(s Student, out string) error { return export(s, nil, out) }

func export(s Student, img image.Image, out string) error {
	if e := os.MkdirAll(out, 0755); e != nil {
		return e
	}
	if img != nil { // nil when the frames are already on disk (async pipeline)
		if e := savePNG(filepath.Join(out, "detail.png"), img); e != nil {
			return e
		}
		for name, frame := range s.frames {
			if e := savePNG(filepath.Join(out, name+".png"), frame); e != nil {
				return e
			}
		}
	}
	data, e := json.MarshalIndent(s, "", "  ")
	if e != nil {
		return e
	}
	if e = os.WriteFile(filepath.Join(out, "student.json"), data, 0644); e != nil {
		return e
	}
	f, e := os.Create(filepath.Join(out, "student.csv"))
	if e != nil {
		return e
	}
	defer f.Close()
	w := csv.NewWriter(f)
	w.UseCRLF = true
	header, row := studentCSV(s)
	if e := w.Write(header); e != nil {
		return e
	}
	if e := w.Write(row); e != nil {
		return e
	}
	w.Flush()
	if e := w.Error(); e != nil {
		return e
	}
	const page = `<!doctype html><html lang="ko"><meta charset="utf-8"><title>Blue Archive · {{.Name}}</title><style>body{font:16px system-ui;margin:32px;background:#eef6fb;color:#17354d}main{max-width:1100px;margin:auto}section{background:white;padding:24px;border-radius:16px;box-shadow:0 4px 20px #ccd}table{border-collapse:collapse;width:100%}th,td{text-align:left;padding:12px;border-bottom:1px solid #ddd}th{width:38%}img{max-width:100%;height:auto;margin-top:20px}small{color:#555}</style><main><h1>{{.Name}} · 학생 메타정보</h1><section><table>{{range .Fields}}<tr><th>{{index . 0}}</th><td>{{index . 1}}</td></tr>{{end}}</table><small>빈 값은 이미지에서 확정하지 못한 항목입니다. 원본 캡처로 검토하세요.</small><h2>증거 화면</h2><img src="detail.png" alt="학생 상세 화면"></section></main></html>`
	fields := [][2]string{{"학생", s.Name}, {"레벨", cell(s.Level)}, {"성급", cell(s.RarityStars)}, {"EX 스킬", cell(s.EXSkillLevel)}, {"기본 스킬", cell(s.BasicSkillLevel)}, {"강화 스킬", cell(s.PassiveSkillLevel)}, {"서브 스킬", cell(s.SubSkillLevel)}, {"인연 랭크", cell(s.BondRank)}, {"전용무기 성급", cell(s.UniqueWeaponStars)}, {"전용무기 레벨", cell(s.UniqueWeaponLevel)}, {"역할", s.CombatRole}, {"핀 설정", pinCell(s.IsPinned)}}
	for _, m := range metaColumns(s) {
		fields = append(fields, [2]string{m.Label, m.Value})
	}
	for i := 0; i < 4; i++ {
		label := fmt.Sprintf("장비 %d", i+1)
		if i == 3 {
			label = "고유 장비(4번)"
		}
		eq := equipmentAt(s, i)
		fields = append(fields, [2]string{label + " 착용", equippedCell(eq.Equipped)}, [2]string{label + " 종류", stringCell(eq.Type)}, [2]string{label + " 아이템", stringCell(eq.ItemName)}, [2]string{label + " 티어", cell(eq.Tier)}, [2]string{label + " 레벨", cell(eq.Level)})
	}
	t := template.Must(template.New("page").Parse(page))
	htmlFile, e := os.Create(filepath.Join(out, "student.html"))
	if e != nil {
		return e
	}
	defer htmlFile.Close()
	return t.Execute(htmlFile, struct {
		Name   string
		Fields [][2]string
	}{s.Name, fields})
}
func findBin(dir, env, rel, unix string) string {
	if v := os.Getenv(env); v != "" {
		return v
	}
	p := filepath.Join(dir, rel)
	if _, e := os.Stat(p); e == nil {
		return p
	}
	if v, e := exec.LookPath(unix); e == nil {
		return v
	}
	return p
}
func mainRun() error {
	live := flag.Bool("live-one", false, "HCU: click first visible student, capture one detail page")
	watch := flag.Bool("watch", false, "background updater: passively watch the game window (screenshots only, no input) and store changed student values in --out")
	interval := flag.Duration("interval", time.Second, "--watch frame interval")
	scan := flag.Bool("scan-live", false, "HCU: scan roster cards while scrolling")
	arrows := flag.Bool("scan-arrows", false, "HCU: scan sequential student profiles using the right arrow")
	verifyFixed := flag.Bool("verify-fixed", false, "also read fixed meta (role/type/terrain) from pixels and warn when it disagrees with the dictionary; for audits after a patch")
	workers := flag.Int("workers", 3, "concurrent extraction workers (OCR runs asynchronously while the scan continues)")
	compareDir := flag.String("compare-ocr", "", "compare Tesseract values saved in this output dir with the PP-OCR sidecar on the same saved frames (no game input)")
	sidecarURL := flag.String("ocr-sidecar", "http://127.0.0.1:47321", "PP-OCR sidecar base URL (see tools/ocr-sidecar)")
	reextractDir := flag.String("reextract", "", "re-run extraction from the saved frames in this output dir (no game input); retries failed students, or --ids")
	idsFlag := flag.String("ids", "", "with --reextract: comma-separated student ids (e.g. 003,007)")
	fast := flag.Bool("fast", false, "scan-arrows: skip the menu pass (상세 popup, 레벨 업, 신비 해방 tabs)")
	fromFirst := flag.Bool("from-first", false, "scan-arrows starts at the first student in the selected roster sort")
	preview := flag.Bool("roster-preview", false, "HCU: capture and detect roster cards without input")
	current := flag.Bool("detail-current", false, "HCU: extract the current student detail without input")
	maxPages := flag.Int("max-pages", 60, "scan-live page safety cap")
	maxStudents := flag.Int("max-students", 300, "scan-live student safety cap")
	rosterOnly := flag.Bool("roster-only", false, "scan-live: collect roster cards without opening details")
	grab := flag.String("grab", "", "HCU: save the current game region to this PNG (no input, no OCR)")
	imagePath := flag.String("image", "", "parse an existing 1280x960 detail screenshot, no desktop input")
	out := flag.String("out", "output-go", "CSV/JSON/HTML output directory")
	regionText := flag.String("region", "230.36,91.73,685.39,433.07", "HCU frame rectangle of Blue Archive client")
	flag.Parse()
	modes := 0
	if *live {
		modes++
	}
	if *scan {
		modes++
	}
	if *arrows {
		modes++
	}
	if *preview {
		modes++
	}
	if *current {
		modes++
	}
	if *imagePath != "" {
		modes++
	}
	if *grab != "" {
		modes++
	}
	if *watch {
		modes++
	}
	if *reextractDir != "" {
		modes++
	}
	if *compareDir != "" {
		modes++
	}
	if modes != 1 {
		return errors.New("choose exactly one of --live-one, --scan-live, --scan-arrows, --roster-preview, --detail-current, --grab, --watch, --reextract, --compare-ocr or --image")
	}
	if *fromFirst && !*arrows {
		return errors.New("--from-first requires --scan-arrows")
	}
	if *maxPages < 1 || *maxStudents < 1 {
		return errors.New("--max-pages and --max-students must be positive")
	}
	if *rosterOnly && !*scan {
		return errors.New("--roster-only requires --scan-live")
	}
	exe, e := os.Executable()
	if e != nil {
		return e
	}
	base := filepath.Dir(exe)
	hcuName := "computer-use"
	ocrName := "tesseract"
	if runtime.GOOS == "windows" {
		hcuName += ".exe"
		ocrName += ".exe"
	}
	h := HCU{Binary: findBin(base, "HCU_BIN", filepath.Join("vendor", "hcu", hcuName), hcuName)}
	tess := findBin(base, "TESSERACT_BIN", filepath.Join("vendor", "ocr", "Library", "bin", ocrName), ocrName)
	tessdata := filepath.Join(base, "vendor", "ocr", "tessdata")
	if v := os.Getenv("TESSDATA_DIR"); v != "" {
		tessdata = v
	}
	if *compareDir != "" {
		sum, err := compareEngines(*compareDir, newSidecarRecognizer(*sidecarURL))
		if err != nil {
			return err
		}
		fmt.Print(sum)
		return nil
	}
	if *reextractDir != "" {
		// Offline: no HCU, no clicks. Menu frames present in the index are re-parsed too.
		ix, err := loadIndex(*reextractDir)
		if err != nil {
			return fmt.Errorf("no index in %s: %w", *reextractDir, err)
		}
		deep := false
		for _, en := range ix.sorted() {
			for _, f := range en.Frames {
				deep = deep || f == frameMystic
			}
		}
		var ids []string
		if *idsFlag != "" {
			ids = strings.Split(*idsFlag, ",")
		}
		rep, err := reextract(*reextractDir, ids, tess, tessdata, Options{VerifyFixed: *verifyFixed}, deep, *workers)
		if err != nil {
			return err
		}
		fmt.Printf("reextract: entries=%d unresolved=%d\n", len(rep.Entries), len(rep.Unresolved))
		return nil
	}
	if *scan || *arrows || *preview || *current || *grab != "" || *watch {
		region, err := parseFrameRegion(*regionText)
		if err != nil {
			return err
		}
		if err = checkAspect(region); err != nil {
			return err
		}
		if *watch {
			st, err := OpenStore(*out)
			if err != nil {
				return err
			}
			ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt)
			defer stop()
			w := &Watcher{Src: hcuSource{h, region}, Store: st, Tess: tess, TessData: tessdata, Opts: Options{VerifyFixed: *verifyFixed}, Interval: *interval,
				OnUpdate: func(u Update) { fmt.Println("update:", u) },
				OnError:  func(e error) { fmt.Fprintln(os.Stderr, "watch:", e) }}
			fmt.Println("watching (screenshots only, no input); Ctrl-C to stop; store:", *out)
			w.Run(ctx)
			return nil
		}
		if *grab != "" {
			if err = h.activate(); err != nil {
				return err
			}
			img, err := gameCapture(h, region)
			if err != nil {
				return err
			}
			return savePNG(*grab, img)
		}
		if *preview {
			if err = h.activate(); err != nil {
				return err
			}
			img, err := gameCapture(h, region)
			if err != nil {
				return err
			}
			ok, err := rosterVisible(img, tess, tessdata)
			if err != nil {
				return err
			}
			if !ok {
				return errors.New("roster not visible")
			}
			if err = os.MkdirAll(*out, 0755); err != nil {
				return err
			}
			if err = savePNG(filepath.Join(*out, "roster-preview.png"), img); err != nil {
				return err
			}
			b, _ := json.MarshalIndent(struct {
				Cards []Card `json:"cards"`
				Thumb Thumb  `json:"thumb"`
			}{rosterCards(img), scrollThumb(img)}, "", "  ")
			fmt.Println(string(b))
			return nil
		}
		if *current {
			if err = h.activate(); err != nil {
				return err
			}
			img, err := gameCapture(h, region)
			if err != nil {
				return err
			}
			if ok, _ := rosterVisible(img, tess, tessdata); ok {
				return errors.New("current screen is roster, not detail")
			}
			student, err := extractWith(img, tess, tessdata, "HCU current detail", Options{VerifyFixed: *verifyFixed})
			if err != nil {
				return err
			}
			if err = export(student, img, *out); err != nil {
				return err
			}
			b, _ := json.MarshalIndent(student, "", "  ")
			fmt.Println(string(b))
			return nil
		}
		if *arrows {
			report, err := scanArrows(h, tess, tessdata, region, *out, *maxStudents, *fromFirst, !*fast, Options{VerifyFixed: *verifyFixed}, *workers)
			fmt.Printf("arrows: entries=%d complete=%t reason=%s unresolved=%d\n", len(report.Entries), report.Complete, report.StopReason, len(report.Unresolved))
			return err
		}
		report, err := scanRoster(h, tess, tessdata, region, *out, *maxPages, *maxStudents, *rosterOnly)
		fmt.Printf("scan: entries=%d pages=%d complete=%t reason=%s unresolved=%d\n", len(report.Entries), report.PagesScanned, report.Complete, report.StopReason, len(report.Unresolved))
		return err
	}
	var img image.Image
	if *imagePath != "" {
		img, e = loadPNG(*imagePath)
		if e != nil {
			return e
		}
	} else {
		var region [4]float64
		parts := strings.Split(*regionText, ",")
		if len(parts) != 4 {
			return errors.New("--region must have four comma-separated frame coordinates")
		}
		for i, p := range parts {
			region[i], e = strconv.ParseFloat(p, 64)
			if e != nil {
				return e
			}
		}
		if e = h.activate(); e != nil {
			return e
		}
		if _, e = h.screenshot(); e != nil {
			return e
		}
		x := region[0] + 140*(region[2]-region[0])/1280
		y := region[1] + 290*(region[3]-region[1])/960
		if e = h.click(x, y); e != nil {
			return e
		}
		time.Sleep(700 * time.Millisecond)
		img, e = gameCapture(h, region)
		if e != nil {
			return e
		}
	}
	if img.Bounds().Dx() != 1280 || img.Bounds().Dy() != 960 {
		return errors.New("detail image must be 1280x960")
	}
	s, e := extractWith(img, tess, tessdata, func() string {
		if *live {
			return "HCU live detail"
		}
		return *imagePath
	}(), Options{VerifyFixed: *verifyFixed})
	if e != nil {
		return e
	}
	if s.Name == "" {
		return errors.New("detail OCR did not find a student name; no export")
	}
	if e = export(s, img, *out); e != nil {
		return e
	}
	b, _ := json.MarshalIndent(s, "", "  ")
	fmt.Println(string(b))
	return nil
}
func main() {
	if e := mainRun(); e != nil {
		fmt.Fprintln(os.Stderr, "ERROR:", e)
		os.Exit(1)
	}
}

// readName resolves the primary OCR text; when that is not an exact dictionary
// hit it re-reads narrower strips (the heart/bond icon and the long strip both
// hurt short names such as a single syllable) and keeps the closest result.
func readName(img image.Image, tesseract, tessdata, primary string, rawOCR map[string]string) (string, *NameMatch) {
	best, bm := defaultResolver.Resolve(primary)
	better := func(m NameMatch, name string) bool {
		if name == "" || m.Status == "empty" {
			return false
		}
		if bm.Status == "empty" || best == "" {
			return true
		}
		return m.Distance < bm.Distance || (m.Distance == bm.Distance && m.Margin > bm.Margin)
	}
	if bm.Status != "exact" {
		retries := []struct {
			key    string
			r      image.Rectangle
			ink    bool
			narrow bool
		}{
			// Same width, white glyphs isolated: cleans up the busy portrait behind the name.
			{"name_ink", image.Rect(64, 789, 300, 835), true, false},
			// Narrow strips rescue short names but can cut a long one off.
			{"name_retry1", image.Rect(64, 789, 130, 835), false, true},
			{"name_retry2", image.Rect(70, 789, 200, 835), false, true},
		}
		for _, rt := range retries {
			var v string
			var err error
			if rt.ink {
				v, err = ocrInk(tesseract, tessdata, img, rt.r, "kor", 7, whiteInk)
			} else {
				v, err = ocr(tesseract, tessdata, img, rt.r, "kor", 7)
			}
			if err != nil {
				continue
			}
			rawOCR[rt.key] = v
			n, m := defaultResolver.Resolve(v)
			// A narrow strip whose right edge still touches glyphs has cut the name
			// off ("호시노(무장)" read as "호시노"); a genuinely short name leaves a gap.
			if rt.narrow && textTouchesRightEdge(img, rt.r) {
				continue
			}
			if better(m, n) {
				best, bm = n, m
			}
		}
	}
	return best, &bm
}
