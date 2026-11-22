package main

import (
	"embed"
	"image"
	"image/color"
	"image/png"
	"regexp"
	"strconv"
	"strings"
)

// ---- schema -------------------------------------------------------------

// Stats are the four 능력치 numbers on the basic-info panel.
type Stats struct {
	MaxHP   *int `json:"max_hp"`  // 최대체력
	Attack  *int `json:"attack"`  // 공격력
	Defense *int `json:"defense"` // 방어력
	Healing *int `json:"healing"` // 치유력
}

type Exp struct {
	Current  *int `json:"current"`
	Required *int `json:"required"`
}

// Loadout is the combat profile strip at the bottom-left of the profile.
type Loadout struct {
	TacticRole  string `json:"tactic_role"`  // 딜러/탱커/힐러/서포터/T.S
	Position    string `json:"position"`     // FRONT/MIDDLE/BACK
	AttackType  string `json:"attack_type"`  // 폭발/관통/신비/진동
	ArmorType   string `json:"armor_type"`   // 경장갑/중장갑/특수장갑/탄력장갑/복합장갑
	WeaponClass string `json:"weapon_class"` // SR/AR/SMG/...
}

// Terrain grades: 4=S (best) .. 0=D. nil when the face is not recognised.
type Terrain struct {
	Urban   *int `json:"urban"`
	Outdoor *int `json:"outdoor"`
	Indoor  *int `json:"indoor"`
}

// Badges are the orange notification dots ("something can be upgraded").
type Badges struct {
	BasicInfoTab     *bool `json:"basic_info_tab"`
	LevelUpTab       *bool `json:"level_up_tab"`
	MysticReleaseTab *bool `json:"mystic_release_tab"`
	SkillGrowth      *bool `json:"skill_growth"`
	WeaponGrowth     *bool `json:"weapon_growth"`
	EquipmentGrowth  *bool `json:"equipment_growth"`
}

// Locks: true = the lock glyph is visible. nil = not locked as far as pixels
// prove (an unlocked sample has not been seen, so it is not reported as false).
type Locks struct {
	AbilityUnlock *bool `json:"ability_unlock_locked"` // 능력개방
	SimsangBloom  *bool `json:"simsang_bloom_locked"`  // 심상개화
}

// NameParts splits a displayed name for search and grouping.
//   - 리오(무장)   -> base 리오, outfit 무장 (same student, other costume)
//   - 시로코*테러  -> base 시로코*테러 (a different character, not an outfit)
type NameParts struct {
	Base         string `json:"base_name"`
	Outfit       string `json:"outfit"`
	OutfitKey    string `json:"outfit_key"`
	IsOutfit     bool   `json:"is_outfit_variant"`
	IsDistinct   bool   `json:"is_distinct_character"`
	DistinctTail string `json:"distinct_suffix,omitempty"`
}

// Reference is community data (SchaleDB) attached after the name is resolved.
// It is *never* merged into pixel-read fields; mismatches become warnings.
type Reference struct {
	Source      string `json:"source"`
	ID          int    `json:"id"`
	School      string `json:"school"`
	Club        string `json:"club"`
	SchoolYear  string `json:"school_year,omitempty"`
	Age         string `json:"age,omitempty"`
	Birthday    string `json:"birthday,omitempty"`
	Height      string `json:"height,omitempty"`
	FullName    string `json:"full_name,omitempty"`
	Hobby       string `json:"hobby,omitempty"`
	Designer    string `json:"designer,omitempty"`
	Illustrator string `json:"illustrator,omitempty"`
	Voice       string `json:"voice,omitempty"`
	StarGrade   int    `json:"base_star_grade"`
	WeaponType  string `json:"weapon_type"`
	Ambiguous   bool   `json:"ambiguous_name,omitempty"`
}

// outfitKeys maps the Korean costume label to a stable key for grouping.
var outfitKeys = map[string]string{
	"수영복": "swimsuit", "드레스": "dress", "무장": "armed", "메이드": "maid", "체육복": "gym",
	"바니걸": "bunny", "응원단": "cheer", "파자마": "pajama", "새해": "new_year", "사복": "casual",
	"온천": "hot_spring", "크리스마스": "christmas", "라이딩": "riding", "어린이": "kid", "교복": "uniform",
	"치파오": "qipao", "아이돌": "idol", "아르바이트": "part_time", "매지컬": "magical", "레이싱": "racing",
	"가이드": "guide", "밴드": "band", "캠핑": "camping",
}

func parseName(name string) NameParts {
	// OutfitKey is never empty so "base costume" is a filterable group too.
	p := NameParts{Base: name, OutfitKey: "base"}
	if i := strings.Index(name, "("); i > 0 && strings.HasSuffix(name, ")") {
		p.Base, p.Outfit, p.IsOutfit = name[:i], name[i+1:len(name)-1], true
		p.OutfitKey = outfitKeys[p.Outfit]
		if p.OutfitKey == "" {
			p.OutfitKey = "other"
		}
	} else if i := strings.Index(name, "*"); i > 0 {
		p.IsDistinct, p.DistinctTail, p.OutfitKey = true, name[i+1:], "distinct"
	}
	return p
}

// ---- pixel helpers --------------------------------------------------------

func rgba(img image.Image, x, y int) color.RGBA {
	return color.RGBAModel.Convert(img.At(x, y)).(color.RGBA)
}

func boolp(v bool) *bool { return &v }
func intp(v int) *int    { return &v }

// orangeDot reports whether a notification dot is drawn inside r.
func orangeDot(img image.Image, r image.Rectangle) bool {
	n := 0
	for y := r.Min.Y; y < r.Max.Y; y++ {
		for x := r.Min.X; x < r.Max.X; x++ {
			c := rgba(img, x, y)
			if c.R > 225 && c.G > 140 && c.G < 215 && c.B < 90 {
				n++
			}
		}
	}
	return n >= 20
}

func dotAt(cx, cy int) image.Rectangle { return image.Rect(cx-10, cy-10, cx+10, cy+10) }

func readBadges(img image.Image) Badges {
	return Badges{
		BasicInfoTab:     boolp(orangeDot(img, dotAt(829, 238))),
		LevelUpTab:       boolp(orangeDot(img, dotAt(1015, 238))),
		MysticReleaseTab: boolp(orangeDot(img, dotAt(1195, 238))),
		SkillGrowth:      boolp(orangeDot(img, dotAt(1196, 441))),
		WeaponGrowth:     boolp(orangeDot(img, dotAt(1196, 573))),
		EquipmentGrowth:  boolp(orangeDot(img, dotAt(1196, 678))),
	}
}

// fraction of pixels in r satisfying f.
func fraction(img image.Image, r image.Rectangle, f func(color.RGBA) bool) float64 {
	n, t := 0, 0
	for y := r.Min.Y; y < r.Max.Y; y++ {
		for x := r.Min.X; x < r.Max.X; x++ {
			t++
			if f(rgba(img, x, y)) {
				n++
			}
		}
	}
	return float64(n) / float64(t)
}

func readLocks(img image.Image) Locks {
	var l Locks
	// 능력개방: dark-teal button carrying a white lock glyph.
	teal := fraction(img, image.Rect(1112, 345, 1196, 416), func(c color.RGBA) bool {
		return c.R < 80 && c.G > 50 && c.G < 120 && c.B > 60 && c.B < 135 && c.B > c.R+15
	})
	white := fraction(img, image.Rect(1135, 365, 1175, 400), func(c color.RGBA) bool { return c.R > 220 && c.G > 220 && c.B > 220 })
	if teal > 0.35 && white > 0.04 {
		l.AbilityUnlock = boolp(true)
	}
	// 심상개화: grey disc carrying a white lock glyph.
	grey := fraction(img, image.Rect(588, 292, 640, 340), func(c color.RGBA) bool {
		d := int(c.R) - int(c.B)
		return c.R > 40 && c.R < 130 && d > -14 && d < 14
	})
	w2 := fraction(img, image.Rect(606, 308, 626, 332), func(c color.RGBA) bool { return c.R > 215 && c.G > 215 && c.B > 215 })
	if grey > 0.25 && w2 > 0.04 {
		l.SimsangBloom = boolp(true)
	}
	return l
}

// ---- badge colours ----------------------------------------------------------

// satMean averages strongly saturated pixels (the badge background, not text).
func satMean(img image.Image, r image.Rectangle) [3]int {
	var s [3]int
	n := 0
	for y := r.Min.Y; y < r.Max.Y; y++ {
		for x := r.Min.X; x < r.Max.X; x++ {
			c := rgba(img, x, y)
			if int(max(c.R, c.G, c.B))-int(min(c.R, c.G, c.B)) > 60 {
				s[0] += int(c.R)
				s[1] += int(c.G)
				s[2] += int(c.B)
				n++
			}
		}
	}
	if n < 40 {
		return [3]int{-1, -1, -1}
	}
	return [3]int{s[0] / n, s[1] / n, s[2] / n}
}

type swatch struct {
	rgb   [3]int
	label string
}

// Badge palettes measured on real screens (see TestLoadoutFromRealScreens).
var attackSwatches = []swatch{{[3]int{147, 6, 13}, "폭발"}, {[3]int{191, 141, 15}, "관통"}, {[3]int{40, 113, 156}, "신비"}, {[3]int{155, 72, 168}, "진동"}}
var armorSwatches = []swatch{{[3]int{148, 12, 17}, "경장갑"}, {[3]int{192, 142, 19}, "중장갑"}, {[3]int{42, 115, 157}, "특수장갑"}, {[3]int{156, 75, 169}, "탄력장갑"}, {[3]int{25, 122, 117}, "복합장갑"}}

func nearestSwatch(c [3]int, set []swatch) string {
	if c[0] < 0 {
		return ""
	}
	best, bl := 1<<30, ""
	for _, s := range set {
		d := abs(c[0]-s.rgb[0]) + abs(c[1]-s.rgb[1]) + abs(c[2]-s.rgb[2])
		if d < best {
			best, bl = d, s.label
		}
	}
	if best > 60 {
		return ""
	}
	return bl
}

// ---- terrain faces ------------------------------------------------------------

// faceGrade classifies a terrain face emoji: 4 green+open mouth, 3 green,
// 2 yellow, 1 red frown, 0 red angry mouth.
func faceGrade(img image.Image, cx int) *int {
	r := image.Rect(cx-14, 903, cx+14, 931)
	var sum [3]int
	n, dark := 0, 0
	for y := r.Min.Y; y < r.Max.Y; y++ {
		for x := r.Min.X; x < r.Max.X; x++ {
			c := rgba(img, x, y)
			mx, mn := max(c.R, c.G, c.B), min(c.R, c.G, c.B)
			if int(mx)-int(mn) > 90 {
				sum[0] += int(c.R)
				sum[1] += int(c.G)
				sum[2] += int(c.B)
				n++
			}
			if mx < 90 {
				dark++
			}
		}
	}
	if n < 150 {
		return nil
	}
	R, G, B := sum[0]/n, sum[1]/n, sum[2]/n
	switch {
	case R < 200 && G > 215 && B < 160:
		if dark >= 50 {
			return intp(4)
		}
		return intp(3)
	case R > 235 && G > 215 && B < 130:
		return intp(2)
	case R > 235 && G > 120 && G < 165 && B < 150:
		// 0 and 1 share the red face; the angry open mouth (many dark pixels) is 0.
		if dark >= 28 {
			return intp(0)
		}
		return intp(1)
	}
	return nil
}

func readTerrain(img image.Image) Terrain {
	return Terrain{Urban: faceGrade(img, 303), Outdoor: faceGrade(img, 343), Indoor: faceGrade(img, 383)}
}

// ---- OCR-backed fields ------------------------------------------------------

var (
	trailingDigits = regexp.MustCompile(`(\d{1,6})\s*$`)
	expPattern     = regexp.MustCompile(`(\d{1,6})\s*/\s*(\d{1,6})`)
)

func readStats(img image.Image, tess, data string, raw map[string]string) (Stats, error) {
	var s Stats
	rows := []struct {
		key  string
		r    image.Rectangle
		dest **int
	}{
		{"stat_hp", image.Rect(684, 350, 860, 374), &s.MaxHP},
		{"stat_attack", image.Rect(890, 350, 1060, 374), &s.Attack},
		{"stat_defense", image.Rect(684, 386, 860, 410), &s.Defense},
		{"stat_healing", image.Rect(890, 386, 1060, 410), &s.Healing},
	}
	rects := make([]image.Rectangle, len(rows))
	for i, row := range rows {
		rects[i] = row.r
	}
	texts, err := ocrAll(img, tess, data, "kor+eng", 7, rects)
	if err != nil {
		return s, err
	}
	for i, row := range rows {
		raw[row.key] = texts[i]
		if m := trailingDigits.FindStringSubmatch(strings.TrimSpace(texts[i])); m != nil {
			if n, e := strconv.Atoi(m[1]); e == nil {
				*row.dest = &n
			}
		}
	}
	return s, nil
}

func readExp(img image.Image, tess, data string, raw map[string]string) (Exp, error) {
	var e Exp
	v, err := ocr(tess, data, img, image.Rect(210, 833, 390, 858), "eng", 7)
	if err != nil {
		return e, err
	}
	raw["exp"] = v
	if expPattern.FindStringSubmatch(v) == nil {
		// Retry as isolated bright ink: "0/10" is otherwise read as "UO/1it".
		if v2, err := ocrBright(tess, data, img, image.Rect(215, 836, 380, 856), 200, 1, "0123456789/"); err == nil {
			raw["exp_retry"] = v2
			v = v2
		}
	}
	if m := expPattern.FindStringSubmatch(v); m != nil {
		cur, _ := strconv.Atoi(m[1])
		req, _ := strconv.Atoi(m[2])
		if req > 0 && cur <= req {
			e.Current, e.Required = &cur, &req
		}
	}
	return e, nil
}

var tacticVocab = []string{"딜러", "탱커", "힐러", "서포터", "T.S"}
var positionVocab = []string{"FRONT", "MIDDLE", "BACK"}
var weaponVocab = []string{"HG", "AR", "SR", "SMG", "SG", "MG", "GL", "RL", "RG", "MT", "FT"}

// closestWord returns the vocabulary entry nearest to raw, or "" when nothing
// is within one jamo/letter of it (so noise never becomes a value).
func closestWord(raw string, vocab []string) string {
	raw = strings.ToUpper(strings.TrimSpace(raw))
	var kept []rune
	for _, c := range raw {
		if (c >= 0xAC00 && c <= 0xD7A3) || (c >= 'A' && c <= 'Z') || c == '.' {
			kept = append(kept, c)
		}
	}
	cj := toJamo(string(kept))
	best, bw, ties := 1<<30, "", 0
	for _, w := range vocab {
		d := editDistance(cj, toJamo(strings.ToUpper(w)))
		if d < best {
			best, bw, ties = d, w, 1
		} else if d == best {
			ties++
		}
	}
	// A tie (e.g. 일러 sits one jamo from both 딜러 and 힐러) is not evidence.
	if best <= 1 && len(cj) > 0 && ties == 1 {
		return bw
	}
	return ""
}

// Role icons (딜러/탱커/힐러/서포터) cut from real profile screens. T.S has no
// sample yet, so its icon simply matches nothing and the role stays null.
//
//go:embed roleicons/*.png
var roleIconFS embed.FS

var roleIcons = loadRoleIcons()

type roleIcon struct {
	label string
	mask  []bool
}

var roleIconRect = image.Rect(72, 869, 97, 897)

// iconMask thresholds the dark glyph pixels of the white role badge.
func iconMask(img image.Image, r image.Rectangle) []bool {
	m := make([]bool, 0, r.Dx()*r.Dy())
	for y := r.Min.Y; y < r.Max.Y; y++ {
		for x := r.Min.X; x < r.Max.X; x++ {
			c := rgba(img, x, y)
			m = append(m, int(c.R)+int(c.G)+int(c.B) < 3*130)
		}
	}
	return m
}

func loadRoleIcons() []roleIcon {
	files, _ := roleIconFS.ReadDir("roleicons")
	var out []roleIcon
	for _, f := range files {
		fh, err := roleIconFS.Open("roleicons/" + f.Name())
		if err != nil {
			continue
		}
		img, err := png.Decode(fh)
		fh.Close()
		if err != nil {
			continue
		}
		out = append(out, roleIcon{strings.TrimSuffix(f.Name(), ".png"), iconMask(img, img.Bounds())})
	}
	return out
}

// roleFromIcon returns the closest icon label, or "" when no template is
// close enough (differing-pixel fraction above 12%).
func roleFromIcon(img image.Image) string {
	m := iconMask(img, roleIconRect)
	best, bl := 2.0, ""
	for _, ic := range roleIcons {
		if len(ic.mask) != len(m) {
			continue
		}
		diff := 0
		for i := range m {
			if m[i] != ic.mask[i] {
				diff++
			}
		}
		if f := float64(diff) / float64(len(m)); f < best {
			best, bl = f, ic.label
		}
	}
	if best > 0.12 {
		return ""
	}
	return bl
}

func readLoadout(img image.Image, tess, data string, raw map[string]string) (Loadout, error) {
	var l Loadout
	role, err := ocr(tess, data, img, image.Rect(100, 868, 165, 898), "kor+eng", 7)
	if err != nil {
		return l, err
	}
	raw["tactic_role"] = role
	// The icon shape is the primary signal; the text only has to not contradict it.
	l.TacticRole = roleFromIcon(img)
	if txt := closestWord(role, tacticVocab); txt != "" && l.TacticRole != "" && txt != l.TacticRole {
		raw["tactic_role_text_disagrees"] = txt
	}
	pos, err := ocr(tess, data, img, image.Rect(35, 903, 160, 933), "eng", 7)
	if err != nil {
		return l, err
	}
	raw["position"] = pos
	l.Position = closestWord(pos, positionVocab)
	// Badge colours are the reliable signal; the white label is only a check.
	l.AttackType = nearestSwatch(satMean(img, image.Rect(205, 872, 285, 894)), attackSwatches)
	l.ArmorType = nearestSwatch(satMean(img, image.Rect(205, 907, 285, 929)), armorSwatches)
	wc, err := ocr(tess, data, img, image.Rect(690, 610, 750, 640), "eng", 7)
	if err != nil {
		return l, err
	}
	raw["weapon_class"] = wc
	l.WeaponClass = closestWord(wc, weaponVocab)
	return l, nil
}

// lookupReference finds the dictionary entry for a resolved name. Some names
// (호시노(무장)) exist twice; the pixel-read role/position picks between them
// and, when still undecided, the entry is flagged ambiguous.
func (r *NameResolver) lookupReference(name string, lo Loadout) *Reference {
	var cands []dictEntry
	for _, e := range r.entries {
		if e.Name == name {
			cands = append(cands, e)
		}
	}
	if len(cands) == 0 {
		return nil
	}
	pick, ambiguous := cands[0], false
	if len(cands) > 1 {
		var fit []dictEntry
		for _, e := range cands {
			if (lo.TacticRole == "" || e.TacticRole == lo.TacticRole) && (lo.Position == "" || e.Position == lo.Position) {
				fit = append(fit, e)
			}
		}
		if len(fit) >= 1 {
			pick = fit[0]
		}
		ambiguous = len(fit) != 1
	}
	return &Reference{Source: "schaledb", ID: pick.ID, School: pick.School, Club: pick.Club, SchoolYear: pick.SchoolYear,
		Age: pick.Age, Birthday: pick.Birthday, Height: pick.Height,
		FullName: pick.FullName, Hobby: pick.Hobby, Designer: pick.Designer, Illustrator: pick.Illustrator, Voice: pick.Voice, StarGrade: pick.StarGrade, WeaponType: pick.WeaponType, Ambiguous: ambiguous}
}

// crossCheck turns disagreements between pixels and reference data into
// warnings. Reference data never overwrites a pixel value.
func (r *NameResolver) crossCheck(s *Student) {
	if s.Name == "" {
		return
	}
	var ref *dictEntry
	for i := range r.entries {
		e := &r.entries[i]
		if e.Name == s.Name && (s.Loadout.TacticRole == "" || e.TacticRole == s.Loadout.TacticRole) {
			ref = e
			break
		}
	}
	if ref == nil {
		return
	}
	cmp := func(field, got, want string) {
		if got != "" && want != "" && got != want {
			s.Warnings = append(s.Warnings, field+"_mismatch: pixels="+got+" reference="+want)
		}
	}
	cmp("position", s.Loadout.Position, ref.Position)
	cmp("attack_type", s.Loadout.AttackType, ref.AttackType)
	cmp("armor_type", s.Loadout.ArmorType, ref.ArmorType)
	cmp("weapon_class", s.Loadout.WeaponClass, ref.WeaponType)
	for _, t := range []struct {
		f    string
		got  *int
		want int
	}{{"terrain_urban", s.Terrain.Urban, ref.Urban}, {"terrain_outdoor", s.Terrain.Outdoor, ref.Outdoor}, {"terrain_indoor", s.Terrain.Indoor, ref.Indoor}} {
		if t.got != nil && *t.got != t.want {
			s.Warnings = append(s.Warnings, t.f+"_mismatch: pixels="+strconv.Itoa(*t.got)+" reference="+strconv.Itoa(t.want))
		}
	}
	if s.RarityStars != nil && ref.StarGrade > 0 && *s.RarityStars < ref.StarGrade {
		s.Warnings = append(s.Warnings, "rarity_below_base_star_grade")
	}
}

// metaCol is one exported value. CSV header, HTML label and roster table all
// come from this single list so they cannot drift apart.
type metaCol struct{ Key, Label, Value string }

func boolCell(v *bool) string {
	if v == nil {
		return ""
	}
	return strconv.FormatBool(*v)
}

func metaColumns(s Student) []metaCol {
	nm, ref := "", Reference{}
	if s.NameMatch != nil {
		nm = s.NameMatch.Status
	}
	if s.Reference != nil {
		ref = *s.Reference
	}
	id := s.Identity
	refID := ""
	if s.Reference != nil {
		refID = strconv.Itoa(ref.ID)
	}
	return []metaCol{
		{"base_name", "원본 이름", id.Base},
		{"outfit", "복장", id.Outfit},
		{"outfit_key", "복장 키", id.OutfitKey},
		{"is_distinct_character", "별도 캐릭터(*)", strconv.FormatBool(id.IsDistinct)},
		{"name_status", "이름 판독", nm},
		{"max_hp", "최대체력", cell(s.Stats.MaxHP)},
		{"attack", "공격력", cell(s.Stats.Attack)},
		{"defense", "방어력", cell(s.Stats.Defense)},
		{"healing", "치유력", cell(s.Stats.Healing)},
		{"exp_current", "경험치", cell(s.Exp.Current)},
		{"exp_required", "필요 경험치", cell(s.Exp.Required)},
		{"tactic_role", "포지션", s.Loadout.TacticRole},
		{"position", "배치", s.Loadout.Position},
		{"attack_type", "공격 타입", s.Loadout.AttackType},
		{"armor_type", "방어 타입", s.Loadout.ArmorType},
		{"weapon_class", "무기 종류", s.Loadout.WeaponClass},
		{"terrain_urban", "도시 적응(4=S..0=D)", cell(s.Terrain.Urban)},
		{"terrain_outdoor", "야외 적응", cell(s.Terrain.Outdoor)},
		{"terrain_indoor", "실내 적응", cell(s.Terrain.Indoor)},
		{"badge_basic_info", "알림: 기본 정보 탭", boolCell(s.Badges.BasicInfoTab)},
		{"badge_level_up", "알림: 레벨 업 탭", boolCell(s.Badges.LevelUpTab)},
		{"badge_mystic_release", "알림: 신비 해방 탭", boolCell(s.Badges.MysticReleaseTab)},
		{"badge_skill_growth", "알림: 스킬 성장", boolCell(s.Badges.SkillGrowth)},
		{"badge_weapon_growth", "알림: 무기 성장", boolCell(s.Badges.WeaponGrowth)},
		{"badge_equipment_growth", "알림: 장비 성장", boolCell(s.Badges.EquipmentGrowth)},
		{"lock_ability_unlock", "능력개방 잠금", boolCell(s.Locks.AbilityUnlock)},
		{"lock_simsang_bloom", "심상개화 잠금", boolCell(s.Locks.SimsangBloom)},
		{"ref_id", "참조 ID(SchaleDB)", refID},
		{"ref_school", "참조: 학교", ref.School},
		{"ref_club", "참조: 동아리", ref.Club},
		{"ref_full_name", "참조: 본명", ref.FullName},
		{"ref_birthday", "참조: 생일", ref.Birthday},
		{"ref_height", "참조: 키", ref.Height},
		{"ref_hobby", "참조: 취미", ref.Hobby},
		{"ref_voice", "참조: CV", ref.Voice},
		{"fixed_meta_source", "고정 메타 출처", fixedSource(s)},
		{"potential_material", "능력개방 재료(고정)", fixedStr(s, func(f *FixedMeta) string { return f.PotentialMaterial })},
		{"skill_materials", "스킬 재료(고정)", fixedStr(s, func(f *FixedMeta) string { return strings.Join(f.SkillMaterials, " / ") })},
		{"equipment_slots", "장비 슬롯 종류(고정)", fixedStr(s, func(f *FixedMeta) string { return strings.Join(f.EquipmentSlots, " / ") })},
		{"weapon_terrain", "고유무기 지형 보너스(고정)", fixedStr(s, func(f *FixedMeta) string {
			if f.WeaponTerrain == "" {
				return ""
			}
			return f.WeaponTerrain + " +" + strconv.Itoa(f.WeaponTerrainBonus)
		})},
		{"ref_base_star_grade", "참조: 기본 성급", func() string {
			if ref.StarGrade == 0 {
				return ""
			}
			return strconv.Itoa(ref.StarGrade)
		}()},
		{"warnings", "경고", strings.Join(s.Warnings, "; ")},
	}
}

func fixedSource(s Student) string {
	if s.Fixed == nil {
		return ""
	}
	return s.Fixed.Source
}

func fixedStr(s Student, f func(*FixedMeta) string) string {
	if s.Fixed == nil {
		return ""
	}
	return f(s.Fixed)
}

// SkillLocked marks skills still locked by the student's star level. A locked
// card shows "★N Lv.1" over a dark overlay, which must not be read as a level.
type SkillLocked struct {
	EX      bool `json:"ex"`
	Basic   bool `json:"basic"`
	Passive bool `json:"passive"`
	Sub     bool `json:"sub"`
}

// skillCardLocked: unlocked cards have a white face under the icon, locked
// ones a dark grey overlay.
func skillCardLocked(img image.Image, cx int) bool {
	return fraction(img, image.Rect(cx-28, 522, cx+28, 538), func(c color.RGBA) bool { return c.R > 200 && c.G > 200 && c.B > 200 }) < 0.3
}

func readSkillLocks(img image.Image) SkillLocked {
	return SkillLocked{EX: skillCardLocked(img, 740), Basic: skillCardLocked(img, 846), Passive: skillCardLocked(img, 952), Sub: skillCardLocked(img, 1058)}
}

// expToLevel maps "exp needed for the next level" to the level. The value is a
// game constant and reads reliably, while the tiny italic "Lv.N" in the profile
// strip does not. Only pairs observed on a real roster (106 students, no
// conflicts) are listed; anything else stays null rather than guessed.
var expToLevel = map[int]int{10: 1, 25: 2, 55: 4, 70: 5, 925: 29, 1685: 39, 9445: 59, 12615: 62, 15220: 64, 16685: 65, 18265: 66, 19965: 67, 21790: 68}

// levelFromExp fills a level the strip OCR could not read.
func levelFromExp(s *Student) {
	if s.Level != nil || s.Exp.Required == nil {
		return
	}
	if lv, ok := expToLevel[*s.Exp.Required]; ok {
		s.Level = intp(lv)
		s.Warnings = append(s.Warnings, "level_from_exp_table")
	}
}
