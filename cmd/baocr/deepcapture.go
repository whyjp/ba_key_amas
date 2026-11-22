package main

import (
	"errors"
	"fmt"
	"image"
	"image/color"
	"regexp"
	"strings"
)

// Frame names written next to detail.png. They are the contract between the
// capture loop (clicks, no OCR) and the extraction workers (OCR, no clicks).
const (
	frameDetail      = "detail"
	framePopupTop    = "popup-top"
	framePopupBottom = "popup-bottom"
	frameLevelUp     = "tab-levelup"
	frameMystic      = "tab-mystic"
)

// captureDeep walks the read-only menus and returns the frames it saw. It does
// no OCR, so the scan loop is never blocked by text recognition. The UI is left
// on the basic-info tab; if that cannot be confirmed it returns an error.
func captureDeep(h HCU, region [4]float64, before image.Image) (map[string]image.Image, error) {
	frames := map[string]image.Image{}
	if err := clickGame(h, region, 1138, 320); err != nil { // 상세
		return nil, err
	}
	top, err := settle(h, region, before)
	if err != nil {
		return nil, fmt.Errorf("stat popup: %w", err)
	}
	// The popup remembers its last scroll offset: always start from the top.
	fx, fy := toFrame(region, 640, 560)
	if err = h.scroll(fx, fy, -12); err != nil {
		return nil, err
	}
	if top, err = settleQuiet(h, region); err != nil {
		return nil, fmt.Errorf("stat popup top: %w", err)
	}
	frames[framePopupTop] = top
	if err = h.scroll(fx, fy, 6); err != nil {
		return nil, err
	}
	bottom, err := settle(h, region, top)
	if err != nil {
		return nil, fmt.Errorf("stat popup scroll: %w", err)
	}
	frames[framePopupBottom] = bottom
	if err = clickGame(h, region, 1057, 267); err != nil { // close
		return nil, err
	}
	closed, err := settle(h, region, bottom)
	if err != nil {
		return nil, fmt.Errorf("stat popup close: %w", err)
	}
	if err = clickGame(h, region, 937, 258); err != nil { // 레벨 업
		return nil, err
	}
	lv, err := settle(h, region, closed)
	if err != nil {
		return nil, fmt.Errorf("level-up tab: %w", err)
	}
	frames[frameLevelUp] = lv
	if err = clickGame(h, region, 1120, 262); err != nil { // 신비 해방
		return nil, err
	}
	my, err := settle(h, region, lv)
	if err != nil {
		return nil, fmt.Errorf("mystic tab: %w", err)
	}
	frames[frameMystic] = my
	if err = clickGame(h, region, 755, 262); err != nil { // 기본 정보
		return nil, err
	}
	back, err := settle(h, region, my)
	if err != nil {
		return nil, fmt.Errorf("basic tab: %w", err)
	}
	if panelDelta(before, back) > 0.02 {
		return nil, errors.New("did not return to the basic-info tab")
	}
	return frames, nil
}

// parseLevelUp reads the 레벨 업 tab's growth stats (no gear/bond).
func parseLevelUp(lv image.Image, tess, data string, raw map[string]string) (*Stats, error) {
	var base Stats
	cells, err := ocrAll(lv, tess, data, "kor+eng", 7, []image.Rectangle{
		image.Rect(686, 380, 945, 410), image.Rect(960, 380, 1200, 410), image.Rect(686, 414, 945, 444), image.Rect(960, 414, 1200, 444)})
	if err != nil {
		return nil, err
	}
	for i, dest := range []**int{&base.MaxHP, &base.Attack, &base.Defense, &base.Healing} {
		raw[fmt.Sprintf("levelup_%d", i)] = cells[i]
		if m := trailingDigits.FindStringSubmatch(strings.TrimSpace(cells[i])); m != nil {
			*dest = atoiLoose(m[1])
		}
	}
	return &base, nil
}

// parseLevelTabLevel reads "Lv.N" from the 레벨 업 tab, where the font is large
// enough to be reliable (the profile-strip level is tiny italic text).
func parseLevelTabLevel(lv image.Image, tess, data string, raw map[string]string) *int {
	v, err := ocrExtra(tess, data, lv, image.Rect(760, 350, 860, 376), "eng", 7, "-c", "tessedit_char_whitelist=Lv.0123456789")
	if err != nil {
		return nil
	}
	raw["levelup_level"] = v
	return value(v, 150)
}

// extractDeep parses the menu frames captured by captureDeep into s.Deep.
func extractDeep(frames map[string]image.Image, tess, data string, s *Student) error {
	top, bottom, lv, my := frames[framePopupTop], frames[framePopupBottom], frames[frameLevelUp], frames[frameMystic]
	if top == nil || bottom == nil || lv == nil || my == nil {
		return errors.New("menu frames missing")
	}
	var d Deep
	d.StatDetail = &StatDetail{}
	if err := readPopupRows(top, tess, data, []int{352, 419, 486, 553, 620}, d.StatDetail, s.RawOCR, "popup_top"); err != nil {
		return err
	}
	if err := readPopupRows(bottom, tess, data, []int{386, 453, 520, 587, 654}, d.StatDetail, s.RawOCR, "popup_bottom"); err != nil {
		return err
	}
	var err error
	if d.BaseStats, err = parseLevelUp(lv, tess, data, s.RawOCR); err != nil {
		return err
	}
	if d.Mystic, err = parseMystic(my, tess, data, s.RawOCR); err != nil {
		return err
	}
	if tab := parseLevelTabLevel(lv, tess, data, s.RawOCR); tab != nil {
		switch {
		case s.Level == nil:
			s.Level = tab
			s.Warnings = append(s.Warnings, "level_from_levelup_tab")
		case *s.Level != *tab:
			s.Warnings = append(s.Warnings, fmt.Sprintf("level_mismatch: strip=%d levelup_tab=%d", *s.Level, *tab))
		}
	}
	s.Deep = &d
	s.deepCrossCheck()
	return nil
}

// starsAt counts gold stars on the 신비 해방 tab's star row.
func starsAt(img image.Image, centers []int) *int {
	n := 0
	for _, cx := range centers {
		if fraction(img, image.Rect(cx-9, 353, cx+9, 371), func(c color.RGBA) bool { return c.R > 220 && c.G > 170 && c.B < 110 }) > 0.35 {
			n++
		}
	}
	return &n
}

func goldStars(img image.Image) *int { return starsAt(img, []int{779, 807, 835, 863, 891}) }

var allInts = regexp.MustCompile(`\d+`)

// parseMystic reads the 신비 해방 tab. A student below max shows the next star
// level, the stat preview and the next max bond rank; a maxed student shows
// "최대 성장에 도달했습니다". Material quantities and credit costs are
// deliberately not collected (they depend on what the player did); which
// materials a student needs is fixed data (fixed_meta).
func parseMystic(my image.Image, tess, data string, raw map[string]string) (*Mystic, error) {
	m := &Mystic{Stars: goldStars(my)}
	if next := starsAt(my, []int{1012, 1040, 1068, 1096, 1124}); *next > 0 {
		m.NextStars = next
	}
	txt, err := ocrAll(my, tess, data, "kor+eng", 6, []image.Rectangle{
		image.Rect(676, 457, 1010, 519), image.Rect(1018, 485, 1200, 515), image.Rect(670, 570, 1205, 640)})
	if err != nil {
		return nil, err
	}
	m.UnlockSkillText, m.MaxBondRankText, m.StatusText = txt[0], txt[1], txt[2]
	if cleaned := strings.ReplaceAll(cleanName(txt[2]), " ", ""); cleaned != "" {
		m.MaxGrowthReached = boolp(strings.Contains(cleaned, "도달"))
	}
	if ranks := allInts.FindAllString(txt[1], -1); len(ranks) >= 2 {
		m.MaxBondRank, m.NextMaxBondRank = atoiLoose(ranks[0]), atoiLoose(ranks[1])
	}
	if m.NextStars == nil {
		return m, nil // maxed: no preview block on screen
	}
	last := func(s string) *int {
		if f := allInts.FindAllString(strings.ReplaceAll(s, ",", ""), -1); len(f) > 0 {
			return atoiLoose(f[len(f)-1])
		}
		return nil
	}
	// The preview numbers are green on a light panel: read them as isolated ink.
	var inks [3]*int
	keys := []string{"mystic_next_hp_ink", "mystic_next_attack_ink", "mystic_next_healing_ink"}
	for i, r := range []image.Rectangle{image.Rect(858, 383, 940, 409), image.Rect(1100, 383, 1180, 409), image.Rect(826, 420, 900, 440)} {
		v, err := ocrInk(tess, data, my, r, "eng", 7, greenInk, "-c", "tessedit_char_whitelist=0123456789")
		if err != nil {
			return nil, err
		}
		raw[keys[i]] = v
		inks[i] = last(v)
	}
	m.NextMaxHP, m.NextAttack, m.NextHealing = inks[0], inks[1], inks[2]
	return m, nil
}
