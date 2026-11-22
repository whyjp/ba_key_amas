package main

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"time"
)

// FieldDiff is one student/field where the two engines disagree.
type FieldDiff struct {
	ID        string `json:"id"`
	Name      string `json:"name"`
	Field     string `json:"field"`
	Tesseract string `json:"tesseract"`
	PP        string `json:"pp"`
	Truth     string `json:"truth,omitempty"` // dictionary / expToLevel / popup, when one applies
}

// CompareSummary is written to <dir>/ocr-compare.json.
type CompareSummary struct {
	Students  int            `json:"students"`
	Seconds   float64        `json:"seconds"`
	Agree     map[string]int `json:"agree"`
	Disagree  map[string]int `json:"disagree"`
	PPNull    map[string]int `json:"pp_null"`
	TessNull  map[string]int `json:"tesseract_null"`
	Diffs     []FieldDiff    `json:"diffs"`
	PPLevelOK []int          `json:"pp_level_vs_exp_table"` // [match, mismatch, checked]
}

func itoa(p *int) string {
	if p == nil {
		return "null"
	}
	return fmt.Sprint(*p)
}

// compareEngines reads every saved detail frame with the frame-level engine and
// compares it with the Tesseract values already stored next to the frame. Where
// an independent reference exists it is attached so disagreements can be judged
// without hand labels: the dictionary for names, expToLevel for levels, and the
// stat popup for the four basic stats (menu-pass runs).
func compareEngines(dir string, rec Recognizer) (CompareSummary, error) {
	sum := CompareSummary{Agree: map[string]int{}, Disagree: map[string]int{}, PPNull: map[string]int{}, TessNull: map[string]int{}, PPLevelOK: []int{0, 0, 0}}
	ix, err := loadIndex(dir)
	if err != nil {
		return sum, err
	}
	t0 := time.Now()
	for _, e := range ix.sorted() {
		dd := detailDir(dir, e.ID)
		img, err := loadPNG(filepath.Join(dd, "detail.png"))
		if err != nil {
			continue
		}
		s, err := loadStudent(dd)
		if err != nil {
			continue
		}
		pp, err := readProfilePP(rec, img)
		if err != nil {
			return sum, fmt.Errorf("student %s: %w", e.ID, err)
		}
		sum.Students++
		cmp := func(field, tess, ppv, truth string) {
			if tess == "null" {
				sum.TessNull[field]++
			}
			if ppv == "null" {
				sum.PPNull[field]++
			}
			if tess == ppv {
				sum.Agree[field]++
				return
			}
			sum.Disagree[field]++
			sum.Diffs = append(sum.Diffs, FieldDiff{ID: e.ID, Name: s.Name, Field: field, Tesseract: tess, PP: ppv, Truth: truth})
		}
		ppName := pp.Name
		if ppName == "" {
			ppName = "null"
		}
		tName := s.Name
		if tName == "" {
			tName = "null"
		}
		cmp("name", tName, ppName, "")
		cmp("level", itoa(s.Level), itoa(pp.Level), expLevelTruth(s))
		cmp("bond", itoa(s.BondRank), itoa(pp.Bond), "")
		for i, k := range []string{"ex_skill", "basic_skill", "passive_skill", "sub_skill"} {
			tv := [4]*int{s.EXSkillLevel, s.BasicSkillLevel, s.PassiveSkillLevel, s.SubSkillLevel}[i]
			cmp(k, itoa(tv), itoa(pp.Skills[i]), "")
		}
		cmp("max_hp", itoa(s.Stats.MaxHP), itoa(pp.Stats.MaxHP), "")
		cmp("attack", itoa(s.Stats.Attack), itoa(pp.Stats.Attack), "")
		cmp("defense", itoa(s.Stats.Defense), itoa(pp.Stats.Defense), "")
		cmp("healing", itoa(s.Stats.Healing), itoa(pp.Stats.Healing), "")
		cmp("exp_required", itoa(s.Exp.Required), itoa(pp.Exp.Required), "")
		// level vs the independent exp table
		if pp.Level != nil && pp.Exp.Required != nil {
			if want, ok := expToLevel[*pp.Exp.Required]; ok {
				sum.PPLevelOK[2]++
				if want == *pp.Level {
					sum.PPLevelOK[0]++
				} else {
					sum.PPLevelOK[1]++
				}
			}
		}
	}
	sum.Seconds = time.Since(t0).Seconds()
	b, _ := json.MarshalIndent(sum, "", "  ")
	return sum, os.WriteFile(filepath.Join(dir, "ocr-compare.json"), b, 0o644)
}

func expLevelTruth(s Student) string {
	if s.Exp.Required != nil {
		if lv, ok := expToLevel[*s.Exp.Required]; ok {
			return fmt.Sprint(lv)
		}
	}
	return ""
}

func (c CompareSummary) String() string {
	var fields []string
	for f := range c.Agree {
		fields = append(fields, f)
	}
	for f := range c.Disagree {
		if _, ok := c.Agree[f]; !ok {
			fields = append(fields, f)
		}
	}
	sort.Strings(fields)
	var sb strings.Builder
	fmt.Fprintf(&sb, "students=%d  %.0fs (%.1fs/profile)\n", c.Students, c.Seconds, c.Seconds/float64(max(c.Students, 1)))
	fmt.Fprintf(&sb, "%-14s %6s %9s %8s %8s\n", "field", "agree", "disagree", "tessNull", "ppNull")
	for _, f := range fields {
		fmt.Fprintf(&sb, "%-14s %6d %9d %8d %8d\n", f, c.Agree[f], c.Disagree[f], c.TessNull[f], c.PPNull[f])
	}
	fmt.Fprintf(&sb, "PP level vs exp table: match=%d mismatch=%d checked=%d\n", c.PPLevelOK[0], c.PPLevelOK[1], c.PPLevelOK[2])
	return sb.String()
}
