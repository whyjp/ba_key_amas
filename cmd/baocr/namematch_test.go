package main

import (
	"os"
	"path/filepath"
	"regexp"
	"testing"
)

func TestResolveName(t *testing.T) {
	cases := []struct{ raw, want, status string }{
		{"『사오리(드레스) wey", "사오리(드레스)", "exact"},
		{"' 시로코^테러       the", "시로코*테러", "exact"},
		{"사오리(트레스)", "사오리(드레스)", "corrected"},
		{"' 미카                 tet", "미카", "exact"},
		{"rs iy          whi", "", "empty"},
		{"oa", "", "empty"},
	}
	for _, c := range cases {
		got, m := defaultResolver.Resolve(c.raw)
		if got != c.want || m.Status != c.status {
			t.Errorf("%q -> %q/%s (%+v), want %q/%s", c.raw, got, m.Status, m, c.want, c.status)
		}
	}
}

// Every dictionary name must survive cleaning unchanged and resolve to itself.
func TestDictionaryRoundTrip(t *testing.T) {
	shape := regexp.MustCompile(`^[가-힣]+(?: [가-힣]+)*(?:\([가-힣]+\)|\*[가-힣]+)?$`)
	for _, e := range defaultResolver.entries {
		if !shape.MatchString(e.Name) {
			t.Errorf("name shape not covered: %q", e.Name)
		}
		if got, m := defaultResolver.Resolve(e.Name); got != e.Name || m.Status != "exact" {
			t.Errorf("%q resolved to %q (%s)", e.Name, got, m.Status)
		}
	}
}

// A single-jamo misread of a short name must not silently pick a neighbour.
func TestShortNameNotForced(t *testing.T) {
	got, m := defaultResolver.Resolve("츄")
	if m.Status == "corrected" {
		t.Errorf("short garbage corrected to %q", got)
	}
}

func TestStrayWordsDropped(t *testing.T) {
	for _, raw := range []string{"' 리오         해", "리오 해", "0 리오"} {
		if got, m := defaultResolver.Resolve(raw); got != "리오" || m.Status != "exact" {
			t.Errorf("%q -> %q/%s", raw, got, m.Status)
		}
	}
	if got, _ := defaultResolver.Resolve("하츠네 미쿠"); got != "하츠네 미쿠" {
		t.Errorf("spaced name split: %q", got)
	}
}

// Gold set: names verified by eye on real 1280x960 detail screens.
func TestGoldNamesFromRealScreens(t *testing.T) {
	bin, data := os.Getenv("TESSERACT_BIN"), os.Getenv("TESSDATA_DIR")
	if bin == "" || data == "" {
		t.Skip("requires Tesseract and models")
	}
	gold := map[string]string{
		"detail-saori.png":            "사오리(드레스)",
		"detail-satsuki-swimsuit.png": "사츠키(수영복)",
		"detail-shiroko-terror.png":   "시로코*테러",
		"detail-rio-special.png":      "리오",
		"detail-rio-armed.png":        "리오(무장)",
		"detail-shun.png":             "슌", // one syllable, read as 순 by Tesseract
	}
	for file, want := range gold {
		img, err := loadPNG(filepath.Join("..", "..", "fixtures", file))
		if err != nil {
			t.Fatal(err)
		}
		s, err := extract(img, bin, data, file)
		if err != nil {
			t.Fatal(err)
		}
		if s.Name != want {
			t.Errorf("%s: name %q want %q (%+v raw=%q)", file, s.Name, want, s.NameMatch, s.RawOCR["name"])
		}
	}
}

// A garbled long name must never collapse to its base name via a truncated
// narrow read (호시노(무장) -> 호시노), while the genuine base still resolves.
func TestTruncatedNameIsNotAccepted(t *testing.T) {
	bin, data := os.Getenv("TESSERACT_BIN"), os.Getenv("TESSDATA_DIR")
	if bin == "" || data == "" {
		t.Skip("requires Tesseract and models")
	}
	for f, want := range map[string]string{"detail-hoshino-armed.png": "호시노(무장)", "detail-hoshino-plain.png": "호시노"} {
		img, err := loadPNG(filepath.Join("..", "..", "fixtures", f))
		if err != nil {
			t.Fatal(err)
		}
		s, err := extract(img, bin, data, f)
		if err != nil {
			t.Fatal(err)
		}
		if s.Name != want && s.Name != "" {
			t.Errorf("%s: name %q (want %q or unresolved) raw=%v", f, s.Name, want, s.RawOCR)
		}
		if f == "detail-hoshino-plain.png" && s.Name != want {
			t.Errorf("%s: plain name lost: %q", f, s.Name)
		}
		t.Logf("%s -> %q", f, s.Name)
	}
}
