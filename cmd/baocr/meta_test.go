package main

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestParseName(t *testing.T) {
	cases := []struct {
		name string
		want NameParts
	}{
		{"리오", NameParts{Base: "리오", OutfitKey: "base"}},
		{"리오(무장)", NameParts{Base: "리오", Outfit: "무장", OutfitKey: "armed", IsOutfit: true}},
		{"사츠키(수영복)", NameParts{Base: "사츠키", Outfit: "수영복", OutfitKey: "swimsuit", IsOutfit: true}},
		{"시로코*테러", NameParts{Base: "시로코*테러", OutfitKey: "distinct", IsDistinct: true, DistinctTail: "테러"}},
		{"노도카(온천)", NameParts{Base: "노도카", Outfit: "온천", OutfitKey: "hot_spring", IsOutfit: true}},
	}
	for _, c := range cases {
		if got := parseName(c.name); got != c.want {
			t.Errorf("%s: %+v want %+v", c.name, got, c.want)
		}
	}
	// Every outfit label in the dictionary must have a stable key.
	for _, e := range defaultResolver.entries {
		if p := parseName(e.Name); p.IsOutfit && p.OutfitKey == "other" {
			t.Errorf("outfit %q of %s has no key", p.Outfit, e.Name)
		}
		if p := parseName(e.Name); p.Base != e.Base {
			t.Errorf("%s: base %q != dictionary base %q", e.Name, p.Base, e.Base)
		}
	}
}

func TestClosestWordTieIsNotEvidence(t *testing.T) {
	if got := closestWord("일러", tacticVocab); got != "" {
		t.Errorf("tie resolved to %q", got)
	}
	if got := closestWord("서포터", tacticVocab); got != "서포터" {
		t.Errorf("exact read lost: %q", got)
	}
}

func ip(p *int) int {
	if p == nil {
		return -1
	}
	return *p
}

// Values below were read off the real screenshots by eye.
func TestMetaFromRealScreens(t *testing.T) {
	bin, data := os.Getenv("TESSERACT_BIN"), os.Getenv("TESSDATA_DIR")
	if bin == "" || data == "" {
		t.Skip("requires Tesseract and models")
	}
	type want struct {
		file                     string
		hp, atk, def, heal       int
		exp, expMax              int
		role, pos, attack, armor string
		terrain                  [3]int
		basicDot, equipDot       bool
	}
	cases := []want{
		{"detail-shun.png", 20221, 4343, 86, 4808, 3, 21790, "딜러", "BACK", "폭발", "경장갑", [3]int{4, 2, 0}, true, true},
		{"detail-rio-armed.png", 33907, 3444, 85, 4848, 41, 21790, "딜러", "MIDDLE", "폭발", "특수장갑", [3]int{4, 2, 0}, false, false},
	}
	for _, c := range cases {
		img, err := loadPNG(filepath.Join("..", "..", "fixtures", c.file))
		if err != nil {
			t.Fatal(err)
		}
		s, err := extractWith(img, bin, data, c.file, Options{VerifyFixed: true})
		if err != nil {
			t.Fatal(err)
		}
		if ip(s.Stats.MaxHP) != c.hp || ip(s.Stats.Attack) != c.atk || ip(s.Stats.Defense) != c.def || ip(s.Stats.Healing) != c.heal {
			t.Errorf("%s stats %+v raw=%q", c.file, s.Stats, s.RawOCR)
		}
		if ip(s.Exp.Current) != c.exp || ip(s.Exp.Required) != c.expMax {
			t.Errorf("%s exp %+v", c.file, s.Exp)
		}
		l := s.Loadout
		if l.TacticRole != c.role || l.Position != c.pos || l.AttackType != c.attack || l.ArmorType != c.armor {
			t.Errorf("%s loadout %+v", c.file, l)
		}
		if ip(s.Terrain.Urban) != c.terrain[0] || ip(s.Terrain.Outdoor) != c.terrain[1] || ip(s.Terrain.Indoor) != c.terrain[2] {
			t.Errorf("%s terrain %d %d %d", c.file, ip(s.Terrain.Urban), ip(s.Terrain.Outdoor), ip(s.Terrain.Indoor))
		}
		if *s.Badges.BasicInfoTab != c.basicDot || *s.Badges.EquipmentGrowth != c.equipDot {
			t.Errorf("%s badges %+v", c.file, s.Badges)
		}
		if s.Locks.AbilityUnlock == nil || s.Locks.SimsangBloom == nil {
			t.Errorf("%s locks %+v", c.file, s.Locks)
		}
		for _, w := range s.Warnings {
			if strings.Contains(w, "_mismatch") {
				t.Errorf("%s: %s", c.file, w)
			}
		}
		if s.Reference == nil {
			t.Errorf("%s: missing reference", c.file)
		}
	}
}

func TestStatPopupFromRealScreens(t *testing.T) {
	bin, data := os.Getenv("TESSERACT_BIN"), os.Getenv("TESSDATA_DIR")
	if bin == "" || data == "" {
		t.Skip("requires Tesseract and models")
	}
	d := &StatDetail{}
	raw := map[string]string{}
	for _, f := range []struct {
		file, prefix string
		ys           []int
	}{
		{"popup-stats-top.png", "popup_top", []int{352, 419, 486, 553, 620}},
		{"popup-stats-bottom.png", "popup_bottom", []int{386, 453, 520, 587, 654}},
	} {
		img, err := loadPNG(filepath.Join("..", "..", "fixtures", f.file))
		if err != nil {
			t.Fatal(err)
		}
		if err = readPopupRows(img, bin, data, f.ys, d, raw, f.prefix); err != nil {
			t.Fatal(err)
		}
	}
	got := map[string]int{"hp": ip(d.Main.MaxHP), "atk": ip(d.Main.Attack), "def": ip(d.Main.Defense), "heal": ip(d.Main.Healing),
		"acc": ip(d.Accuracy), "eva": ip(d.Evasion), "crit": ip(d.Critical), "critdmg": ip(d.CriticalDamagePct),
		"pierce": ip(d.DefensePierce), "stab": ip(d.Stability), "range": ip(d.Range), "cost": ip(d.CostRecovery),
		"ccp": ip(d.CCPower), "ccr": ip(d.CCResist), "bonus": ip(d.TypeBonusPct), "maxcost": ip(d.MaxCostBonus)}
	want := map[string]int{"hp": 33907, "atk": 3444, "def": 85, "heal": 4848, "acc": 101, "eva": 1079, "crit": 523, "critdmg": 234,
		"pierce": 0, "stab": 980, "range": 550, "cost": 700, "ccp": 100, "ccr": 124, "bonus": 200, "maxcost": 0}
	for k, w := range want {
		if got[k] != w {
			t.Errorf("%s = %d, want %d (raw %v)", k, got[k], w, raw)
		}
	}
	if d.TypeBonusKind != "폭발" {
		t.Errorf("type bonus kind: %q", d.TypeBonusKind)
	}
}

func TestMysticTabFromRealScreens(t *testing.T) {
	bin, data := os.Getenv("TESSERACT_BIN"), os.Getenv("TESSDATA_DIR")
	if bin == "" || data == "" {
		t.Skip("requires Tesseract and models")
	}
	load := func(f string) *Mystic {
		img, err := loadPNG(filepath.Join("..", "..", "fixtures", f))
		if err != nil {
			t.Fatal(err)
		}
		m, err := parseMystic(img, bin, data, map[string]string{})
		if err != nil {
			t.Fatal(err)
		}
		return m
	}
	three := load("tab-mystic-3star.png")
	if ip(three.Stars) != 3 || ip(three.NextStars) != 4 || ip(three.MaxBondRank) != 20 || ip(three.NextMaxBondRank) != 30 ||
		ip(three.NextMaxHP) != 13837 || ip(three.NextAttack) != 1424 || ip(three.NextHealing) != 5179 {
		t.Errorf("3-star mystic: %+v", three)
	}
	maxed := load("tab-mystic.png")
	if ip(maxed.Stars) != 5 || maxed.NextStars != nil || maxed.MaxGrowthReached == nil || !*maxed.MaxGrowthReached {
		t.Errorf("maxed mystic: %+v", maxed)
	}
}

// Dictionary mode (default) must reproduce what pixel verification reads, and
// must not spend OCR on the fixed groups.
func TestFixedStageMatchesPixels(t *testing.T) {
	bin, data := os.Getenv("TESSERACT_BIN"), os.Getenv("TESSDATA_DIR")
	if bin == "" || data == "" {
		t.Skip("requires Tesseract and models")
	}
	for _, f := range []string{"detail-shun.png", "detail-rio-armed.png", "detail-saori.png", "detail-satsuki-swimsuit.png"} {
		img, err := loadPNG(filepath.Join("..", "..", "fixtures", f))
		if err != nil {
			t.Fatal(err)
		}
		fast, err := extractWith(img, bin, data, f, Options{})
		if err != nil {
			t.Fatal(err)
		}
		audit, err := extractWith(img, bin, data, f, Options{VerifyFixed: true})
		if err != nil {
			t.Fatal(err)
		}
		if fast.Fixed == nil || fast.Fixed.Source != "dictionary" || len(fast.Fixed.EquipmentSlots) == 0 {
			t.Errorf("%s: fixed meta %+v", f, fast.Fixed)
		}
		if _, ok := fast.RawOCR["tactic_role"]; ok {
			t.Errorf("%s: dictionary mode still OCR'd the fixed loadout", f)
		}
		if fast.Loadout.TacticRole != audit.Loadout.TacticRole || fast.Loadout.Position != audit.Loadout.Position ||
			fast.Loadout.AttackType != audit.Loadout.AttackType || fast.Loadout.ArmorType != audit.Loadout.ArmorType {
			t.Errorf("%s: dictionary loadout %+v != pixels %+v", f, fast.Loadout, audit.Loadout)
		}
		if audit.Fixed == nil || audit.Fixed.Source != "pixels+dictionary" {
			t.Errorf("%s: audit source %+v", f, audit.Fixed)
		}
	}
}

// Regression for the first full scan: Lv.1 students read as "Lvl"/"lul" and
// the bond number inside the pink heart was almost never read.
func TestLowLevelAndBondFromRealScreens(t *testing.T) {
	bin, data := os.Getenv("TESSERACT_BIN"), os.Getenv("TESSDATA_DIR")
	if bin == "" || data == "" {
		t.Skip("requires Tesseract and models")
	}
	img, err := loadPNG(filepath.Join("..", "..", "fixtures", "detail-serika-newyear-lv1.png"))
	if err != nil {
		t.Fatal(err)
	}
	s, err := extract(img, bin, data, "serika")
	if err != nil {
		t.Fatal(err)
	}
	if s.Name != "세리카(새해)" || ip(s.EXSkillLevel) != 1 || ip(s.BasicSkillLevel) != 1 ||
		ip(s.PassiveSkillLevel) != 1 || ip(s.SubSkillLevel) != 1 || ip(s.BondRank) != 2 || ip(s.Exp.Current) != 0 || ip(s.Exp.Required) != 10 {
		t.Errorf("serika: name=%q level=%d skills=%d/%d/%d/%d bond=%d exp=%d/%d raw=%v", s.Name, ip(s.Level), ip(s.EXSkillLevel), ip(s.BasicSkillLevel),
			ip(s.PassiveSkillLevel), ip(s.SubSkillLevel), ip(s.BondRank), ip(s.Exp.Current), ip(s.Exp.Required), s.RawOCR)
	}
	for f, want := range map[string]int{"detail-shun.png": 11, "detail-rio-armed.png": 13, "detail-saori.png": 5, "detail-shiroko-terror.png": 12} {
		im, _ := loadPNG(filepath.Join("..", "..", "fixtures", f))
		st, err := extract(im, bin, data, f)
		if err != nil {
			t.Fatal(err)
		}
		if ip(st.BondRank) != want {
			t.Errorf("%s: bond %d want %d (raw %q)", f, ip(st.BondRank), want, st.RawOCR["bond"])
		}
	}
}

func TestLockedSkillsHaveNoLevel(t *testing.T) {
	bin, data := os.Getenv("TESSERACT_BIN"), os.Getenv("TESSDATA_DIR")
	if bin == "" || data == "" {
		t.Skip("requires Tesseract and models")
	}
	img, err := loadPNG(filepath.Join("..", "..", "fixtures", "detail-tsurugi-swimsuit-lv1-locked.png"))
	if err != nil {
		t.Fatal(err)
	}
	s, err := extract(img, bin, data, "tsurugi")
	if err != nil {
		t.Fatal(err)
	}
	if s.SkillLocked != (SkillLocked{Passive: true, Sub: true}) {
		t.Errorf("locks: %+v", s.SkillLocked)
	}
	if ip(s.EXSkillLevel) != 1 || ip(s.BasicSkillLevel) != 1 || s.PassiveSkillLevel != nil || s.SubSkillLevel != nil {
		t.Errorf("levels ex=%d basic=%d passive=%v sub=%v", ip(s.EXSkillLevel), ip(s.BasicSkillLevel), s.PassiveSkillLevel, s.SubSkillLevel)
	}
}

// 루미 has urban/outdoor grade 1 in the dictionary: the red frown, not the
// angry face (grade 0). Both share a colour, only the mouth differs.
func TestTerrainGradeOneVersusZero(t *testing.T) {
	img, err := loadPNG(filepath.Join("..", "..", "fixtures", "detail-rumi-terrain1.png"))
	if err != nil {
		t.Fatal(err)
	}
	tr := readTerrain(img)
	var rumi dictEntry
	for _, e := range defaultResolver.entries {
		if e.Name == "루미" {
			rumi = e
		}
	}
	if rumi.Urban != 1 || rumi.Outdoor != 1 {
		t.Skipf("dictionary changed: %+v", rumi)
	}
	if ip(tr.Urban) != rumi.Urban || ip(tr.Outdoor) != rumi.Outdoor || ip(tr.Indoor) != rumi.Indoor {
		t.Errorf("terrain %d/%d/%d, dictionary %d/%d/%d", ip(tr.Urban), ip(tr.Outdoor), ip(tr.Indoor), rumi.Urban, rumi.Outdoor, rumi.Indoor)
	}
	shun, _ := loadPNG(filepath.Join("..", "..", "fixtures", "detail-shun.png"))
	if st := readTerrain(shun); ip(st.Urban) != 4 || ip(st.Outdoor) != 2 || ip(st.Indoor) != 0 {
		t.Errorf("shun terrain regressed: %d/%d/%d", ip(st.Urban), ip(st.Outdoor), ip(st.Indoor))
	}
}

func TestLevelFromExpTable(t *testing.T) {
	var s Student
	s.Exp.Required = intPtr(10)
	levelFromExp(&s)
	if ip(s.Level) != 1 {
		t.Fatalf("level %d", ip(s.Level))
	}
	known := Student{Level: intPtr(68)}
	known.Exp.Required = intPtr(10)
	levelFromExp(&known) // a read level is never overridden
	if ip(known.Level) != 68 {
		t.Fatal("read level overridden")
	}
	unknown := Student{}
	unknown.Exp.Required = intPtr(4242)
	levelFromExp(&unknown)
	if unknown.Level != nil {
		t.Fatal("unlisted exp must stay null")
	}
}
