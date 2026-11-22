package main

import (
	"crypto/sha1"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strconv"
	"sync"
	"time"
)

// Store keeps the latest record of every student plus an append-only change
// history. It is independent of how frames are obtained (computer-use scan,
// passive watcher, re-extraction), so any of them can feed it.
//
//	<dir>/students/<key>.json   latest Student
//	<dir>/history.jsonl         one line per detected change
type Store struct {
	dir string
	mu  sync.Mutex
}

func OpenStore(dir string) (*Store, error) {
	if err := os.MkdirAll(filepath.Join(dir, "students"), 0o755); err != nil {
		return nil, err
	}
	return &Store{dir: dir}, nil
}

// Change is one field that differs from the stored record.
type Change struct {
	Field string `json:"field"`
	From  string `json:"from"`
	To    string `json:"to"`
}

// HistoryEntry is one line of history.jsonl.
type HistoryEntry struct {
	Time    time.Time `json:"time"`
	Key     string    `json:"key"`
	Name    string    `json:"name"`
	New     bool      `json:"new,omitempty"`
	Changes []Change  `json:"changes,omitempty"`
}

// studentKey identifies a student: the dictionary id when it is unambiguous,
// otherwise a hash of the name (names are unique per costume).
func studentKey(s Student) string {
	if s.Fixed != nil && s.Fixed.DictionaryID > 0 && !s.Fixed.Ambiguous {
		return strconv.Itoa(s.Fixed.DictionaryID)
	}
	return fmt.Sprintf("n%x", sha1.Sum([]byte(s.Name)))[:13]
}

func ints(p *int) string {
	if p == nil {
		return ""
	}
	return strconv.Itoa(*p)
}

// variableView lists only the variable-stage fields (see fixed.go), which are
// the ones a background updater has to keep fresh. Fixed meta never changes
// here, and raw OCR text is evidence, not data.
func variableView(s Student) map[string]string {
	v := map[string]string{
		"level": ints(s.Level), "rarity_stars": ints(s.RarityStars),
		"ex_skill_level": ints(s.EXSkillLevel), "normal_skill_level": ints(s.BasicSkillLevel),
		"passive_skill_level": ints(s.PassiveSkillLevel), "sub_skill_level": ints(s.SubSkillLevel),
		"bond_rank": ints(s.BondRank), "unique_weapon_stars": ints(s.UniqueWeaponStars), "unique_weapon_level": ints(s.UniqueWeaponLevel),
		"is_pinned": boolCell(s.IsPinned),
		"max_hp":    ints(s.Stats.MaxHP), "attack": ints(s.Stats.Attack), "defense": ints(s.Stats.Defense), "healing": ints(s.Stats.Healing),
		"exp_current": ints(s.Exp.Current), "exp_required": ints(s.Exp.Required),
	}
	for i, eq := range s.Equipment {
		p := fmt.Sprintf("equipment_%d_", i+1)
		v[p+"equipped"], v[p+"item_name"], v[p+"tier"], v[p+"level"] = boolCell(eq.Equipped), stringCell(eq.ItemName), ints(eq.Tier), ints(eq.Level)
	}
	return v
}

// diffVariable lists the variable fields that changed. A field the new read
// could not determine (empty) never overwrites a known value: a blurry frame
// must not erase data.
func diffVariable(old, cur Student) (changes []Change, merged Student) {
	merged = cur
	a, b := variableView(old), variableView(cur)
	keys := make([]string, 0, len(b))
	for k := range b {
		keys = append(keys, k)
	}
	sort.Strings(keys)
	for _, k := range keys {
		if b[k] == "" || a[k] == b[k] {
			continue
		}
		changes = append(changes, Change{Field: k, From: a[k], To: b[k]})
	}
	// Keep previously known values for fields this read left empty.
	keep := func(dst **int, old *int) {
		if *dst == nil {
			*dst = old
		}
	}
	keep(&merged.Level, old.Level)
	keep(&merged.RarityStars, old.RarityStars)
	keep(&merged.EXSkillLevel, old.EXSkillLevel)
	keep(&merged.BasicSkillLevel, old.BasicSkillLevel)
	keep(&merged.PassiveSkillLevel, old.PassiveSkillLevel)
	keep(&merged.SubSkillLevel, old.SubSkillLevel)
	keep(&merged.BondRank, old.BondRank)
	keep(&merged.UniqueWeaponStars, old.UniqueWeaponStars)
	keep(&merged.UniqueWeaponLevel, old.UniqueWeaponLevel)
	keep(&merged.Stats.MaxHP, old.Stats.MaxHP)
	keep(&merged.Stats.Attack, old.Stats.Attack)
	keep(&merged.Stats.Defense, old.Stats.Defense)
	keep(&merged.Stats.Healing, old.Stats.Healing)
	return changes, merged
}

// Upsert stores s and reports what changed against the previous record.
func (st *Store) Upsert(s Student, at time.Time) (changes []Change, isNew bool, err error) {
	if s.Name == "" {
		return nil, false, fmt.Errorf("refusing to store a student without a name")
	}
	st.mu.Lock()
	defer st.mu.Unlock()
	key := studentKey(s)
	path := filepath.Join(st.dir, "students", key+".json")
	var old Student
	if b, e := os.ReadFile(path); e == nil {
		if e = json.Unmarshal(b, &old); e != nil {
			return nil, false, e
		}
	} else {
		isNew = true
	}
	merged := s
	if !isNew {
		changes, merged = diffVariable(old, s)
		if len(changes) == 0 {
			return nil, false, nil
		}
	}
	b, err := json.MarshalIndent(merged, "", "  ")
	if err != nil {
		return nil, false, err
	}
	if err = os.WriteFile(path, b, 0o644); err != nil {
		return nil, false, err
	}
	line, _ := json.Marshal(HistoryEntry{Time: at, Key: key, Name: s.Name, New: isNew, Changes: changes})
	f, err := os.OpenFile(filepath.Join(st.dir, "history.jsonl"), os.O_APPEND|os.O_CREATE|os.O_WRONLY, 0o644)
	if err != nil {
		return changes, isNew, err
	}
	defer f.Close()
	_, err = f.Write(append(line, '\n'))
	return changes, isNew, err
}

// All returns every stored student, sorted by name.
func (st *Store) All() ([]Student, error) {
	files, err := filepath.Glob(filepath.Join(st.dir, "students", "*.json"))
	if err != nil {
		return nil, err
	}
	var out []Student
	for _, f := range files {
		b, e := os.ReadFile(f)
		if e != nil {
			return nil, e
		}
		var s Student
		if e = json.Unmarshal(b, &s); e != nil {
			return nil, e
		}
		out = append(out, s)
	}
	sort.Slice(out, func(i, j int) bool { return out[i].Name < out[j].Name })
	return out, nil
}
