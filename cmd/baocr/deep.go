package main

import (
	"errors"
	"fmt"
	"image"
	"image/color"
	"regexp"
	"strconv"
	"strings"
	"sync"
	"time"
)

// Deep collection walks the read-only menus of one student profile: the
// "상세" stat popup, the 레벨 업 tab and the 신비 해방 tab. It never presses an
// upgrade/unlock/purchase button; it only opens tabs and closes the popup.

// StatDetail is the full "학생 능력치 상세 정보" popup.
type StatDetail struct {
	Main              Stats  `json:"main"` // popup copy of the four basic stats
	Accuracy          *int   `json:"accuracy"`
	Evasion           *int   `json:"evasion"`
	Critical          *int   `json:"critical"`
	CriticalDamagePct *int   `json:"critical_damage_pct"`
	DefensePierce     *int   `json:"defense_pierce"`
	Stability         *int   `json:"stability"`
	Range             *int   `json:"range"`
	CostRecovery      *int   `json:"cost_recovery"`
	CCPower           *int   `json:"cc_power"`
	CCResist          *int   `json:"cc_resist"`
	TypeBonusKind     string `json:"type_bonus_kind"` // 폭발/관통/신비/진동
	TypeBonusPct      *int   `json:"type_bonus_pct"`  // 특효 %
	MaxCostBonus      *int   `json:"max_cost_bonus"`
}

// Mystic is the 신비 해방 tab.
type Mystic struct {
	Stars            *int   `json:"stars"`
	NextStars        *int   `json:"next_stars"` // star count after the unlock; nil at max
	UnlockSkillText  string `json:"unlock_skill_text"`
	MaxBondRankText  string `json:"max_bond_rank_text"`
	MaxBondRank      *int   `json:"max_bond_rank"`
	NextMaxBondRank  *int   `json:"next_max_bond_rank"`
	NextMaxHP        *int   `json:"next_max_hp"`
	NextAttack       *int   `json:"next_attack"`
	NextHealing      *int   `json:"next_healing"`
	StatusText       string `json:"status_text"`
	MaxGrowthReached *bool  `json:"max_growth_reached"`
}

type Deep struct {
	StatDetail *StatDetail `json:"stat_detail,omitempty"`
	// BaseStats come from the 레벨 업 tab: growth stats without gear/bond.
	BaseStats *Stats  `json:"base_stats,omitempty"`
	Mystic    *Mystic `json:"mystic,omitempty"`
}

// Account is the header currency strip, read once per run.
type Account struct {
	AP        string `json:"ap"`
	Credits   *int   `json:"credits"`
	Pyroxene  *int   `json:"pyroxene"`
	RawCredit string `json:"raw_credits"`
}

// ---- helpers ------------------------------------------------------------------

// ocrAll reads several rectangles concurrently (each is a separate process).
func ocrAll(img image.Image, tess, data, lang string, psm int, rects []image.Rectangle, extra ...string) ([]string, error) {
	out := make([]string, len(rects))
	errs := make([]error, len(rects))
	var wg sync.WaitGroup
	for i, r := range rects {
		wg.Add(1)
		go func(i int, r image.Rectangle) {
			defer wg.Done()
			out[i], errs[i] = ocrExtra(tess, data, img, r, lang, psm, extra...)
		}(i, r)
	}
	wg.Wait()
	for _, e := range errs {
		if e != nil {
			return out, e
		}
	}
	return out, nil
}

// panelRect is the right-hand info panel/popup area. The left side holds the
// idle-animated character, which must not count as "the screen changed".
var panelRect = image.Rect(660, 230, 1215, 780)

// panelDelta is the fraction of sampled panel pixels that differ noticeably.
func panelDelta(a, b image.Image) float64 {
	n, diff := 0, 0
	for y := panelRect.Min.Y; y < panelRect.Max.Y; y += 3 {
		for x := panelRect.Min.X; x < panelRect.Max.X; x += 3 {
			n++
			ca, cb := rgba(a, x, y), rgba(b, x, y)
			if abs(int(ca.R)-int(cb.R))+abs(int(ca.G)-int(cb.G))+abs(int(ca.B)-int(cb.B)) > 60 {
				diff++
			}
		}
	}
	return float64(diff) / float64(n)
}

// settle waits for the panel to CHANGE from `from` (the click took effect) and
// then hold still for two consecutive captures, so neither the old screen nor
// a half-drawn animation is ever read.
func settle(h HCU, region [4]float64, from image.Image) (image.Image, error) {
	var last image.Image
	changed := false
	for i := 0; i < 16; i++ {
		time.Sleep(150 * time.Millisecond)
		cur, err := gameCapture(h, region)
		if err != nil {
			return nil, err
		}
		if panelDelta(from, cur) > 0.004 {
			changed = true
		}
		if changed && last != nil && panelDelta(last, cur) < 0.0008 {
			return cur, nil
		}
		last = cur
	}
	return nil, errors.New("screen did not change and settle after the click")
}

// settleQuiet waits for two identical consecutive captures without requiring a
// change (used after a scroll that may already be at its limit).
func settleQuiet(h HCU, region [4]float64) (image.Image, error) {
	var last image.Image
	for i := 0; i < 12; i++ {
		time.Sleep(200 * time.Millisecond)
		cur, err := gameCapture(h, region)
		if err != nil {
			return nil, err
		}
		if last != nil && panelDelta(last, cur) < 0.0008 {
			return cur, nil
		}
		last = cur
	}
	return nil, errors.New("screen did not settle")
}

func clickGame(h HCU, region [4]float64, x, y int) error {
	fx, fy := toFrame(region, x, y)
	return h.click(fx, fy)
}

var valuePattern = regexp.MustCompile(`^(.*?)\s*(-?[\d,]+)\s*%?\s*$`)

func atoiLoose(s string) *int {
	n, err := strconv.Atoi(strings.ReplaceAll(s, ",", ""))
	if err != nil {
		return nil
	}
	return &n
}

// statLabels are the popup row labels (spaces removed for comparison).
var statLabels = []string{"최대체력", "공격력", "방어력", "치유력", "명중수치", "회피수치", "치명수치", "치명대미지",
	"방어관통수치", "안정수치", "일반공격사거리", "코스트회복력", "군중제어강화력", "군중제어저항력", "최대코스트증가"}

// matchLabel maps OCR'd label text to a canonical label, "" if unsure.
func matchLabel(text string) string {
	var hangul []rune
	for _, c := range text {
		if c >= 0xAC00 && c <= 0xD7A3 {
			hangul = append(hangul, c)
		}
	}
	t := string(hangul)
	if strings.Contains(t, "특효") {
		return "특효"
	}
	cj := toJamo(t)
	best, bl, ties := 1<<30, "", 0
	for _, l := range statLabels {
		d := editDistance(cj, toJamo(l))
		if d < best {
			best, bl, ties = d, l, 1
		} else if d == best {
			ties++
		}
	}
	limit := 1
	if len(cj) >= 8 {
		limit = 2
	}
	if best <= limit && ties == 1 {
		return bl
	}
	// A truncated label ("회복력" for 코스트회복력) is fine if it is a unique suffix.
	if tt := []rune(t); len(tt) >= 3 {
		found := ""
		for _, l := range statLabels {
			if strings.HasSuffix(l, t) {
				if found != "" {
					return ""
				}
				found = l
			}
		}
		return found
	}
	return ""
}

// applyCell parses "<label> <value>" and stores the value in d.
func (d *StatDetail) applyCell(text string) {
	text = strings.TrimSpace(strings.ReplaceAll(text, "|", ""))
	m := valuePattern.FindStringSubmatch(text)
	if m == nil {
		return
	}
	label, val := matchLabel(m[1]), atoiLoose(m[2])
	if val == nil {
		return
	}
	switch label {
	case "최대체력":
		d.Main.MaxHP = val
	case "공격력":
		d.Main.Attack = val
	case "방어력":
		d.Main.Defense = val
	case "치유력":
		d.Main.Healing = val
	case "명중수치":
		d.Accuracy = val
	case "회피수치":
		d.Evasion = val
	case "치명수치":
		d.Critical = val
	case "치명대미지":
		d.CriticalDamagePct = val
	case "방어관통수치":
		d.DefensePierce = val
	case "안정수치":
		d.Stability = val
	case "일반공격사거리":
		d.Range = val
	case "코스트회복력":
		d.CostRecovery = val
	case "군중제어강화력":
		d.CCPower = val
	case "군중제어저항력":
		d.CCResist = val
	case "최대코스트증가":
		d.MaxCostBonus = val
	case "특효":
		d.TypeBonusPct = val
		for _, k := range []string{"폭발", "관통", "신비", "진동"} {
			if strings.Contains(m[1], k) {
				d.TypeBonusKind = k
			}
		}
	}
}

func rowRects(ys []int) []image.Rectangle {
	var rs []image.Rectangle
	for _, y := range ys {
		rs = append(rs, image.Rect(232, y-21, 640, y+21), image.Rect(650, y-21, 1062, y+21))
	}
	return rs
}

// readPopupRows OCRs both cells of each row and folds them into d.
func readPopupRows(img image.Image, tess, data string, ys []int, d *StatDetail, raw map[string]string, prefix string) error {
	rects := rowRects(ys)
	if len(rects) > 9 { // the 5th bottom row has only a left cell
		rects = rects[:9]
	}
	if prefix == "popup_top" {
		rects = rowRects(ys)
	}
	cells, err := ocrAll(img, tess, data, "kor+eng", 7, rects)
	if err != nil {
		return err
	}
	for i, c := range cells {
		raw[fmt.Sprintf("%s_%d", prefix, i)] = c
		d.applyCell(c)
	}
	return nil
}

// deepCrossCheck compares menu values with the basic panel. Disagreements are
// warnings; nothing is overwritten.
func (s *Student) deepCrossCheck() {
	d := s.Deep
	if d == nil || d.StatDetail == nil {
		return
	}
	pairs := []struct {
		f    string
		a, b *int
	}{{"max_hp", s.Stats.MaxHP, d.StatDetail.Main.MaxHP}, {"attack", s.Stats.Attack, d.StatDetail.Main.Attack},
		{"defense", s.Stats.Defense, d.StatDetail.Main.Defense}, {"healing", s.Stats.Healing, d.StatDetail.Main.Healing}}
	for _, p := range pairs {
		if p.a != nil && p.b != nil && *p.a != *p.b {
			s.Warnings = append(s.Warnings, fmt.Sprintf("popup_%s_mismatch: panel=%d popup=%d", p.f, *p.a, *p.b))
		}
	}
}

func readAccount(img image.Image, tess, data string) (*Account, error) {
	cells, err := ocrAll(img, tess, data, "eng", 7, []image.Rectangle{
		image.Rect(505, 10, 600, 36), image.Rect(718, 10, 832, 36), image.Rect(936, 10, 995, 36)})
	if err != nil {
		return nil, err
	}
	a := &Account{AP: strings.TrimSpace(cells[0]), RawCredit: cells[1]}
	a.Credits = atoiLoose(strings.NewReplacer(" ", "", ".", "").Replace(strings.TrimSpace(cells[1])))
	a.Pyroxene = atoiLoose(strings.NewReplacer(" ", "", ".", "").Replace(strings.TrimSpace(cells[2])))
	return a, nil
}

// ocrInk reads text drawn in a colour set apart from its background (e.g. a red
// count on a dark pill): pixels accepted by ink become black on white first.
func ocrInk(tess, data string, img image.Image, r image.Rectangle, lang string, psm int, ink func(color.RGBA) bool, extra ...string) (string, error) {
	bw := image.NewGray(image.Rect(0, 0, r.Dx(), r.Dy()))
	for y := 0; y < r.Dy(); y++ {
		for x := 0; x < r.Dx(); x++ {
			v := uint8(255)
			if ink(rgba(img, r.Min.X+x, r.Min.Y+y)) {
				v = 0
			}
			bw.SetGray(x, y, color.Gray{Y: v})
		}
	}
	return recognizePNG(tess, data, resizeNearest(bw, bw.Bounds().Dx()*6, bw.Bounds().Dy()*6), lang, psm, extra...)
}

// greenInk accepts the green "after unlock" numbers on the light panel.
func greenInk(c color.RGBA) bool {
	return c.G > 130 && int(c.G) > int(c.R)+40 && int(c.G) > int(c.B)+20
}

// whiteInk accepts near-white glyphs (e.g. the bond number inside the pink heart).
func whiteInk(c color.RGBA) bool { return c.R > 225 && c.G > 225 && c.B > 225 }

// readHeartNumber reads the bond rank drawn in dark ink inside the pink heart.
// Only dark pixels with pink on both sides of the same row count as glyphs, so
// the blue panel around the heart can never leak into the read.
func readHeartNumber(img image.Image, tess, data string) (string, error) {
	r := image.Rect(34, 794, 84, 832)
	pink := func(c color.RGBA) bool {
		return c.R > 225 && c.G > 150 && c.G < 235 && c.B > 180 && int(c.R)-int(c.G) > 15
	}
	bw := image.NewGray(image.Rect(0, 0, r.Dx(), r.Dy()))
	for y := 0; y < r.Dy(); y++ {
		left, right := -1, -1
		for x := 0; x < r.Dx(); x++ {
			if pink(rgba(img, r.Min.X+x, r.Min.Y+y)) {
				if left < 0 {
					left = x
				}
				right = x
			}
		}
		for x := 0; x < r.Dx(); x++ {
			v := uint8(255)
			if left >= 0 && x > left && x < right {
				if c := rgba(img, r.Min.X+x, r.Min.Y+y); c.R < 140 && c.G < 150 && c.B < 180 {
					v = 0
				}
			}
			bw.SetGray(x, y, color.Gray{Y: v})
		}
	}
	return recognizePNG(tess, data, resizeNearest(bw, bw.Bounds().Dx()*6, bw.Bounds().Dy()*6), "eng", 7, "-c", "tessedit_char_whitelist=0123456789")
}

// ocrBright is the fallback read for small light text on a dark bar: bright
// pixels become black ink, strokes are thickened, then a closed alphabet is
// enforced. It rescues thin italic glyphs such as the "1" in "Lv.1".
func ocrBright(tess, data string, img image.Image, r image.Rectangle, thr uint8, dil int, whitelist string) (string, error) {
	const k = 6
	w, h := r.Dx()*k, r.Dy()*k
	bw := image.NewGray(image.Rect(0, 0, w, h))
	for y := 0; y < h; y++ {
		for x := 0; x < w; x++ {
			c := rgba(img, r.Min.X+x/k, r.Min.Y+y/k)
			v := uint8(255)
			if c.R > thr && c.G > thr && c.B > thr {
				v = 0
			}
			bw.SetGray(x, y, color.Gray{Y: v})
		}
	}
	out := image.NewGray(bw.Bounds())
	for y := 0; y < h; y++ {
		for x := 0; x < w; x++ {
			v := uint8(255)
		search:
			for dy := -dil; dy <= dil; dy++ {
				for dx := -dil; dx <= dil; dx++ {
					if xx, yy := x+dx, y+dy; xx >= 0 && yy >= 0 && xx < w && yy < h && bw.GrayAt(xx, yy).Y == 0 {
						v = 0
						break search
					}
				}
			}
			out.SetGray(x, y, color.Gray{Y: v})
		}
	}
	return recognizePNG(tess, data, out, "eng", 7, "-c", "tessedit_char_whitelist="+whitelist)
}
