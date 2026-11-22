package main

import (
	"encoding/json"
	"image"
	"image/color"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestSlotDetection(t *testing.T) {
	img := image.NewRGBA(image.Rect(0, 0, 1280, 960))
	for _, x := range []int{1061, 1077} {
		for xx := x - 5; xx <= x+5; xx++ {
			for y := 633; y < 641; y++ {
				img.Set(xx, y, color.RGBA{0, 220, 230, 255})
			}
		}
	}
	got := weaponStars(img)
	if got == nil || *got != 2 {
		t.Fatalf("weapon stars = %v, want 2", got)
	}
}
func TestExportEscapesOCR(t *testing.T) {
	out := t.TempDir()
	s := Student{Name: "<script>alert(1)</script>", RawOCR: map[string]string{"test": "<bad>"}}
	if err := export(s, image.NewRGBA(image.Rect(0, 0, 1280, 960)), out); err != nil {
		t.Fatal(err)
	}
	b, err := os.ReadFile(filepath.Join(out, "student.html"))
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(string(b), "<script>alert(1)</script>") {
		t.Fatal("unescaped OCR text")
	}
}
func TestEquipmentUnknownIsNotInvented(t *testing.T) {
	if equipmentTier("unreadable") != nil {
		t.Fatal("unreadable tier became a value")
	}
	blank := image.NewRGBA(image.Rect(0, 0, 1280, 960))
	var catalog equipmentCatalog
	data, err := equipmentAssets.ReadFile("equipment_catalog.json")
	if err != nil {
		t.Fatal(err)
	}
	if err := json.Unmarshal(data, &catalog); err != nil {
		t.Fatal(err)
	}
	if len(catalog.Entries) != 90 {
		t.Fatalf("catalog entries = %d", len(catalog.Entries))
	}
	kind, name, tier, score, err := matchEquipment(blank, 1, nil, catalog)
	if err != nil || kind != nil || name != nil || tier != nil || score != nil {
		t.Fatalf("blank image produced item: %v %v %v %v %v", kind, name, tier, score, err)
	}
}
func TestSaoriEquipment(t *testing.T) {
	bin, data := os.Getenv("TESSERACT_BIN"), os.Getenv("TESSDATA_DIR")
	if bin == "" || data == "" {
		t.Skip("requires Tesseract and models")
	}
	img, err := loadPNG("../../fixtures/detail-saori.png")
	if err != nil {
		t.Fatal(err)
	}
	eq, raw, err := detectEquipment(img, bin, data)
	if err != nil {
		t.Fatal(err)
	}
	if len(eq) != 4 {
		t.Fatalf("slots=%d, want 4", len(eq))
	}
	types := []string{"Hat", "Hairpin", "Watch"}
	names := []string{"Ribbon Beret", "Winged Hairpin", "Antique Pocket Watch"}
	for i := 0; i < 3; i++ {
		e := eq[i]
		if e.Equipped == nil || !*e.Equipped || e.Type == nil || *e.Type != types[i] || e.ItemName == nil || *e.ItemName != names[i] || e.Tier == nil || *e.Tier != 4 || e.Level == nil || *e.Level != 40 {
			t.Errorf("slot %d: %+v, raw=%v", i+1, e, raw)
		}
	}
	if eq[3].Equipped == nil || *eq[3].Equipped || eq[3].Level != nil {
		t.Errorf("unique gear empty: %+v, raw=%v", eq[3], raw)
	}
}
func TestPinnedAndRoleFromRealScreens(t *testing.T) {
	for _, tc := range []struct {
		file string
		pin  bool
		role string
	}{
		{"../../fixtures/detail-saori.png", true, "STRIKER"},
		{"../../fixtures/detail-satsuki-swimsuit.png", false, "STRIKER"},
		{"../../fixtures/detail-rio-special.png", true, "SPECIAL"},
	} {
		img, err := loadPNG(tc.file)
		if err != nil {
			t.Fatal(err)
		}
		got := pinState(img)
		if got == nil || *got != tc.pin {
			t.Errorf("%s pin=%v", tc.file, got)
		}
		if bin, data := os.Getenv("TESSERACT_BIN"), os.Getenv("TESSDATA_DIR"); bin != "" && data != "" {
			s, err := extract(img, bin, data, "fixture")
			if err != nil {
				t.Fatal(err)
			}
			if s.CombatRole != tc.role || s.IsPinned == nil || *s.IsPinned != tc.pin {
				t.Errorf("%s role=%q pin=%v", tc.file, s.CombatRole, s.IsPinned)
			}
		}
	}
}
func TestShirokoTerrorVariantName(t *testing.T) {
	bin, data := os.Getenv("TESSERACT_BIN"), os.Getenv("TESSDATA_DIR")
	if bin == "" || data == "" {
		t.Skip("requires OCR")
	}
	img, err := loadPNG("../../fixtures/detail-shiroko-terror.png")
	if err != nil {
		t.Fatal(err)
	}
	s, err := extract(img, bin, data, "fixture")
	if err != nil {
		t.Fatal(err)
	}
	if s.Name != "시로코*테러" {
		t.Fatalf("variant name=%q raw=%q", s.Name, s.RawOCR["name"])
	}
}
func TestSaoriRealOCR(t *testing.T) {
	bin := os.Getenv("TESSERACT_BIN")
	data := os.Getenv("TESSDATA_DIR")
	if bin == "" || data == "" {
		t.Skip("set TESSERACT_BIN and TESSDATA_DIR for real OCR integration")
	}
	img, err := loadPNG("../../fixtures/detail-saori.png")
	if err != nil {
		t.Fatal(err)
	}
	s, err := extract(img, bin, data, "test fixture")
	if err != nil {
		t.Fatal(err)
	}
	checks := map[string]*int{"level": s.Level, "rarity": s.RarityStars, "ex": s.EXSkillLevel, "basic": s.BasicSkillLevel, "passive": s.PassiveSkillLevel, "sub": s.SubSkillLevel, "bond": s.BondRank, "weapon stars": s.UniqueWeaponStars, "weapon level": s.UniqueWeaponLevel}
	wants := map[string]int{"level": 68, "rarity": 5, "ex": 2, "basic": 4, "passive": 5, "sub": 3, "bond": 5, "weapon stars": 2, "weapon level": 7}
	if s.Name != "사오리(드레스)" || s.CombatRole != "STRIKER" {
		t.Errorf("name=%q role=%q", s.Name, s.CombatRole)
	}
	for k, v := range checks {
		if v == nil || *v != wants[k] {
			t.Errorf("%s=%v, want %d", k, v, wants[k])
		}
	}
}

func TestWindowRefExactTitle(t *testing.T) {
	apps := "w13 \"Blue Archive\"  app=BlueArchive  active\nw15 \"Main Page - Blue Archive Wiki - Chrome\"  app=chrome\n"
	if got := windowRef(apps, "Blue Archive"); got != "w13" {
		t.Fatalf("got %q", got)
	}
	if got := windowRef("w15 \"Main Page - Blue Archive Wiki - Chrome\"  app=chrome\n", "Blue Archive"); got != "" {
		t.Fatalf("substring matched: %q", got)
	}
}

func TestWindowActive(t *testing.T) {
	apps := "w13 \"Blue Archive\"  app=BlueArchive  active\nw15 \"Main Page - Blue Archive Wiki - Chrome\"  app=chrome\n"
	if !windowActive(apps, "Blue Archive") {
		t.Fatal("active game window not detected")
	}
	if windowActive("w13 \"Blue Archive\"  app=BlueArchive\nw15 \"x\"  app=chrome  active\n", "Blue Archive") {
		t.Fatal("inactive game window reported active")
	}
}

func TestCheckAspect(t *testing.T) {
	if err := checkAspect([4]float64{230.36, 91.73, 685.39, 433.07}); err != nil {
		t.Fatalf("4:3 default region rejected: %v", err)
	}
	if err := checkAspect([4]float64{0, 0, 1920, 1080}); err == nil {
		t.Fatal("16:9 region accepted")
	}
}

func TestDetailVisibleVsRoster(t *testing.T) {
	for _, f := range []string{"detail-saori.png", "detail-shun.png", "detail-rio-armed.png", "detail-shiroko-terror.png",
		"detail-satsuki-swimsuit.png", "detail-serika-newyear-lv1.png", "detail-tsurugi-swimsuit-lv1-locked.png"} {
		img, err := loadPNG("../../fixtures/" + f)
		if err != nil {
			t.Fatal(err)
		}
		if !detailVisible(img) {
			t.Errorf("%s: profile not recognised", f)
		}
		if ok, _ := rosterVisible(img, "", ""); ok {
			t.Errorf("%s: profile mistaken for the roster", f)
		}
	}
	for _, f := range []string{"roster-top.png", "roster-scroll1.png"} {
		img, err := loadPNG("../../fixtures/" + f)
		if err != nil {
			t.Fatal(err)
		}
		if detailVisible(img) {
			t.Errorf("%s: roster mistaken for a profile", f)
		}
	}
}
