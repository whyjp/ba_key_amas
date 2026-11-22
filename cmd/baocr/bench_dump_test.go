package main

import (
	"encoding/json"
	"os"
	"path/filepath"
	"testing"
)

// TestBenchDump writes the Tesseract pipeline's readings of the labelled
// fixtures to $BENCH_OUT for tools/ocr-bench/bench.py. Skipped unless set.
func TestBenchDump(t *testing.T) {
	outPath, bin, data := os.Getenv("BENCH_OUT"), os.Getenv("TESSERACT_BIN"), os.Getenv("TESSDATA_DIR")
	if outPath == "" || bin == "" || data == "" {
		t.Skip("set BENCH_OUT, TESSERACT_BIN, TESSDATA_DIR")
	}
	out := map[string]any{}
	for _, f := range []string{"detail-shun.png", "detail-rio-armed.png", "detail-saori.png", "detail-satsuki-swimsuit.png", "detail-shiroko-terror.png",
		"detail-rio-special.png", "detail-hoshino-armed.png", "detail-hoshino-plain.png", "detail-serika-newyear-lv1.png", "detail-tsurugi-swimsuit-lv1-locked.png", "detail-rumi-terrain1.png"} {
		img, err := loadPNG(filepath.Join("..", "..", "fixtures", f))
		if err != nil {
			t.Fatal(err)
		}
		s, err := extract(img, bin, data, f)
		if err != nil {
			t.Fatal(err)
		}
		out[f] = map[string]any{"name": s.Name, "level": s.Level, "skills": []*int{s.EXSkillLevel, s.BasicSkillLevel, s.PassiveSkillLevel, s.SubSkillLevel},
			"exp": s.Exp, "bond": s.BondRank, "stats": s.Stats}
	}
	d := &StatDetail{}
	raw := map[string]string{}
	for _, p := range []struct {
		f, pre string
		ys     []int
	}{{"popup-stats-top.png", "popup_top", []int{352, 419, 486, 553, 620}}, {"popup-stats-bottom.png", "popup_bottom", []int{386, 453, 520, 587, 654}}} {
		img, err := loadPNG(filepath.Join("..", "..", "fixtures", p.f))
		if err != nil {
			t.Fatal(err)
		}
		if err = readPopupRows(img, bin, data, p.ys, d, raw, p.pre); err != nil {
			t.Fatal(err)
		}
	}
	out["popup"] = d
	b, _ := json.MarshalIndent(out, "", " ")
	if err := os.WriteFile(outPath, b, 0o644); err != nil {
		t.Fatal(err)
	}
}
