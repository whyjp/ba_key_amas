package main

import (
	"image"
	"os"
	"path/filepath"
	"testing"
	"time"
)

type seqSource struct {
	frames []image.Image
	i      int
}

func (s *seqSource) Frame() (image.Image, error) {
	f := s.frames[min(s.i, len(s.frames)-1)]
	s.i++
	return f, nil
}

func fx(t *testing.T, f string) image.Image {
	t.Helper()
	img, err := loadPNG(filepath.Join("..", "..", "fixtures", f))
	if err != nil {
		t.Fatal(err)
	}
	return img
}

func intPtr(v int) *int { return &v }

// A level-up on the open profile must surface as a change; a blurry read that
// leaves a field empty must neither report a change nor erase the old value.
func TestStoreDiffAndKeep(t *testing.T) {
	st, err := OpenStore(t.TempDir())
	if err != nil {
		t.Fatal(err)
	}
	old := Student{Name: "슌", Level: intPtr(68), BondRank: intPtr(11), Stats: Stats{MaxHP: intPtr(20221)}}
	if _, isNew, err := st.Upsert(old, time.Now()); err != nil || !isNew {
		t.Fatalf("first upsert new=%v err=%v", isNew, err)
	}
	// Same data: nothing to report.
	if ch, isNew, _ := st.Upsert(old, time.Now()); isNew || len(ch) != 0 {
		t.Fatalf("unchanged data reported: %v %v", isNew, ch)
	}
	// Bond up, HP unreadable this time.
	cur := Student{Name: "슌", Level: intPtr(68), BondRank: intPtr(12)}
	ch, isNew, err := st.Upsert(cur, time.Now())
	if err != nil || isNew || len(ch) != 1 || ch[0].Field != "bond_rank" || ch[0].From != "11" || ch[0].To != "12" {
		t.Fatalf("changes %+v new=%v err=%v", ch, isNew, err)
	}
	all, _ := st.All()
	if len(all) != 1 || ip(all[0].BondRank) != 12 || ip(all[0].Stats.MaxHP) != 20221 {
		t.Fatalf("stored %+v", all)
	}
	b, _ := os.ReadFile(filepath.Join(st.dir, "history.jsonl"))
	if n := len(splitLines(string(b))); n != 2 {
		t.Errorf("history lines %d", n)
	}
	if _, _, err := st.Upsert(Student{}, time.Now()); err == nil {
		t.Error("nameless student stored")
	}
}

func splitLines(s string) []string {
	var out []string
	cur := ""
	for _, c := range s {
		if c == '\n' {
			out = append(out, cur)
			cur = ""
		} else {
			cur += string(c)
		}
	}
	return out
}

// The watcher reads a profile only after it holds still, ignores non-profile
// screens, and does not re-report an unchanged view.
func TestWatcherReadsStableProfilesOnly(t *testing.T) {
	bin, data := os.Getenv("TESSERACT_BIN"), os.Getenv("TESSDATA_DIR")
	if bin == "" || data == "" {
		t.Skip("requires Tesseract and models")
	}
	shun, rio, roster := fx(t, "detail-shun.png"), fx(t, "detail-rio-armed.png"), fx(t, "roster-top.png")
	src := &seqSource{frames: []image.Image{roster, shun, rio, rio, rio, shun, shun, shun, roster}}
	st, _ := OpenStore(t.TempDir())
	w := &Watcher{Src: src, Store: st, Tess: bin, TessData: data}
	var names []string
	for range src.frames {
		up, err := w.Step()
		if err != nil {
			t.Fatal(err)
		}
		if up != nil {
			if !up.New {
				t.Errorf("unexpected change report %v", up)
			}
			names = append(names, up.Name)
		}
	}
	// roster ignored; shun seen once (flicker) -> not stable; rio stable -> stored;
	// shun x3 stable -> stored. Exactly two new students.
	if len(names) != 2 || names[0] != "리오(무장)" || names[1] != "슌" {
		t.Fatalf("updates %v", names)
	}
	all, _ := st.All()
	if len(all) != 2 {
		t.Fatalf("stored %d", len(all))
	}
}
