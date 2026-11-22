package main

import (
	"net/http"
	"os"
	"testing"
	"time"
)

// pp reads fixtures with the frame-level engine and checks them against
// values read off the screens by eye. Skipped unless the dev sidecar runs.
func sidecarOrSkip(t *testing.T) *sidecarRecognizer {
	t.Helper()
	url := os.Getenv("OCR_SIDECAR_URL")
	if url == "" {
		url = "http://127.0.0.1:47321"
	}
	c := http.Client{Timeout: 2 * time.Second}
	if resp, err := c.Get(url + "/health"); err != nil {
		t.Skip("OCR sidecar not running (python tools/ocr-sidecar/server.py)")
	} else {
		resp.Body.Close()
	}
	return newSidecarRecognizer(url)
}

func TestPPReadsPanelFromRealScreens(t *testing.T) {
	rec := sidecarOrSkip(t)
	cases := []struct {
		file                  string
		name                  string
		hp, atk, def, heal    int
		skills                [4]int
		level, bond, exp, req int
	}{
		{"detail-shun.png", "슌", 20221, 4343, 86, 4808, [4]int{2, 3, 3, 2}, 68, 11, 3, 21790},
		{"detail-serika-newyear-lv1.png", "세리카(새해)", 2290, 260, 18, 2064, [4]int{1, 1, 1, 1}, 1, 2, 0, 10},
	}
	for _, c := range cases {
		r, err := readProfilePP(rec, fx(t, c.file))
		if err != nil {
			t.Fatal(err)
		}
		got := []int{ip(r.Stats.MaxHP), ip(r.Stats.Attack), ip(r.Stats.Defense), ip(r.Stats.Healing), ip(r.Level), ip(r.Bond), ip(r.Exp.Current), ip(r.Exp.Required)}
		want := []int{c.hp, c.atk, c.def, c.heal, c.level, c.bond, c.exp, c.req}
		for i := range want {
			if got[i] != want[i] {
				t.Errorf("%s field %d: got %d want %d (%+v)", c.file, i, got[i], want[i], r)
			}
		}
		for i, w := range c.skills {
			if ip(r.Skills[i]) != w {
				t.Errorf("%s skill %d: got %d want %d", c.file, i, ip(r.Skills[i]), w)
			}
		}
		if r.Name != c.name {
			t.Errorf("%s name %q (raw %q) want %q", c.file, r.Name, r.NameRaw, c.name)
		}
	}
}
