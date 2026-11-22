package main

import (
	_ "embed"
	"encoding/json"
	"strings"
)

// The dictionary is community data (see cmd/namedict). It corrects OCR spelling
// only: nothing besides the name is ever filled from it.
//
//go:embed student_names_kr.json
var studentNamesJSON []byte

// dictEntry mirrors cmd/namedict output. Everything except Name/Base/Outfit is
// reference data used only to cross-check pixel-read values.
type dictEntry struct {
	ID          int    `json:"id"`
	Name        string `json:"name"`
	Base        string `json:"base"`
	Outfit      string `json:"outfit"`
	Role        string `json:"role"`
	TacticRole  string `json:"tactic_role"`
	Position    string `json:"position"`
	AttackType  string `json:"attack_type"`
	ArmorType   string `json:"armor_type"`
	WeaponType  string `json:"weapon_type"`
	StarGrade   int    `json:"star_grade"`
	School      string `json:"school"`
	Club        string `json:"club"`
	SchoolYear  string `json:"school_year"`
	Age         string `json:"age"`
	Birthday    string `json:"birthday"`
	Height      string `json:"height"`
	FullName    string `json:"full_name"`
	Hobby       string `json:"hobby"`
	Designer    string `json:"designer"`
	Illustrator string `json:"illustrator"`
	Voice       string `json:"voice"`

	EquipmentSlots     []string `json:"equipment_slots"`
	PotentialMaterial  string   `json:"potential_material"`
	SkillMaterials     []string `json:"skill_materials"`
	WeaponTerrain      string   `json:"weapon_terrain"`
	WeaponTerrainBonus int      `json:"weapon_terrain_bonus"`
	Urban              int      `json:"terrain_urban"`
	Outdoor            int      `json:"terrain_outdoor"`
	Indoor             int      `json:"terrain_indoor"`
}

type NameMatch struct {
	Status    string `json:"status"` // exact | corrected | ambiguous | unlisted | empty
	Cleaned   string `json:"cleaned"`
	Candidate string `json:"candidate,omitempty"`
	Distance  int    `json:"distance"`
	Margin    int    `json:"margin"`
}

type NameResolver struct {
	entries []dictEntry
	jamo    [][]rune
}

var defaultResolver = mustResolver()

func mustResolver() *NameResolver {
	var d struct {
		Names []dictEntry `json:"names"`
	}
	if err := json.Unmarshal(studentNamesJSON, &d); err != nil || len(d.Names) == 0 {
		panic("embedded student name dictionary is invalid")
	}
	r := &NameResolver{entries: d.Names}
	for _, e := range d.Names {
		r.jamo = append(r.jamo, toJamo(e.Name))
	}
	return r
}

// toJamo splits Hangul syllables into jamo so that a one-stroke misread costs 1.
func toJamo(s string) []rune {
	var out []rune
	for _, c := range s {
		if c >= 0xAC00 && c <= 0xD7A3 {
			v := int(c - 0xAC00)
			out = append(out, rune(0x1100+v/588), rune(0x1161+(v%588)/28))
			if t := v % 28; t != 0 {
				out = append(out, rune(0x11A7+t))
			}
		} else {
			out = append(out, c)
		}
	}
	return out
}

func editDistance(a, b []rune) int {
	prev := make([]int, len(b)+1)
	cur := make([]int, len(b)+1)
	for j := range prev {
		prev[j] = j
	}
	for i := 1; i <= len(a); i++ {
		cur[0] = i
		for j := 1; j <= len(b); j++ {
			cost := 1
			if a[i-1] == b[j-1] {
				cost = 0
			}
			cur[j] = min(prev[j]+1, cur[j-1]+1, prev[j-1]+cost)
		}
		prev, cur = cur, prev
	}
	return prev[len(b)]
}

// cleanName keeps only characters a student name can contain. Tesseract renders
// the collaboration star as ^; the dictionary spells it with ASCII *
func cleanName(raw string) string {
	raw = strings.NewReplacer("^", "*", "＊", "*").Replace(raw)
	var b strings.Builder
	for _, c := range raw {
		if (c >= 0xAC00 && c <= 0xD7A3) || c == '(' || c == ')' || c == '*' || c == ' ' {
			b.WriteRune(c)
		}
	}
	s := strings.Join(strings.Fields(b.String()), " ")
	// Edge noise only: a leading ")" or trailing "(" can never be part of a name.
	return strings.TrimRight(strings.TrimLeft(s, ")* "), "(* ")
}

// nearest returns the closest dictionary entry, the distance and the gap to
// the runner-up (the margin that guards against variant/short-name confusion).
func (r *NameResolver) nearest(cleaned string) (idx, best, margin int) {
	cj := toJamo(cleaned)
	best, second := 1<<30, 1<<30
	for i, j := range r.jamo {
		d := editDistance(cj, j)
		if d < best {
			second, best, idx = best, d, i
		} else if d < second {
			second = d
		}
	}
	return idx, best, second - best
}

// windows lists contiguous word runs, widest first, so stray OCR words beside
// the name ("리오 해") can be dropped while "하츠네 미쿠" stays whole.
func windows(cleaned string) []string {
	w := strings.Fields(cleaned)
	var out []string
	for n := len(w); n >= 1; n-- {
		for i := 0; i+n <= len(w); i++ {
			out = append(out, strings.Join(w[i:i+n], " "))
		}
	}
	return out
}

// maxCorrection: short names get one jamo of slack, longer ones two.
func maxCorrection(name string) int {
	if len(toJamo(name)) < 5 {
		return 1
	}
	return 2
}

func (r *NameResolver) Resolve(raw string) (string, NameMatch) {
	cleaned := cleanName(raw)
	m := NameMatch{Cleaned: cleaned}
	if cleaned == "" {
		m.Status = "empty"
		return "", m
	}
	bi, best, margin := -1, 1<<30, 0
	used := cleaned
	for _, w := range windows(cleaned) {
		i, d, mg := r.nearest(w)
		if d < best { // widest window wins ties
			bi, best, margin, used = i, d, mg, w
		}
	}
	m.Distance, m.Margin, m.Candidate = best, margin, r.entries[bi].Name
	switch {
	case best == 0:
		m.Status = "exact"
		return m.Candidate, m
	case best <= maxCorrection(used) && m.Margin >= 2:
		m.Status = "corrected"
		return m.Candidate, m
	case best <= 2:
		// Too close to another student to choose: keep the evidence, no name.
		m.Status = "ambiguous"
		return "", m
	}
	// Not near any known student (new release or garbage). Keep text only if
	// it is a plausible name shape; the reviewer sees status=unlisted.
	m.Status = "unlisted"
	if len([]rune(used)) >= 2 {
		return used, m
	}
	return "", m
}

// RoleOf returns the dictionary's squad role for a canonical name, if known.
func (r *NameResolver) RoleOf(name string) string {
	for _, e := range r.entries {
		if e.Name == name {
			return e.Role
		}
	}
	return ""
}
