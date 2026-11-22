package main

import (
	"image"
	"os"
	"path/filepath"
	"testing"
)

func TestRosterRowsAndOverlap(t *testing.T) {
	a, err := loadPNG("../../fixtures/roster-top.png")
	if err != nil {
		t.Fatal(err)
	}
	b, err := loadPNG("../../fixtures/roster-scroll1.png")
	if err != nil {
		t.Fatal(err)
	}
	first := rosterCards(a)
	second := rosterCards(b)
	if len(first) != 12 || len(second) < 12 {
		t.Fatalf("top=%d scroll=%d", len(first), len(second))
	}
	count := 0
	for _, card := range second {
		for _, prev := range first {
			if signatureDistance(card.Signature, prev.Signature) < 2 {
				count++
				break
			}
		}
	}
	if count != 6 {
		t.Fatalf("overlap=%d, want 6", count)
	}
}
func TestRosterScrollbar(t *testing.T) {
	a, _ := loadPNG("../../fixtures/roster-top.png")
	b, _ := loadPNG("../../fixtures/roster-scroll1.png")
	p, q := scrollThumb(a), scrollThumb(b)
	for _, im := range []image.Image{a, b} {
		valid, err := rosterVisible(im, "", "")
		if err != nil || !valid {
			t.Fatalf("fixture roster not visible: valid=%t err=%v", valid, err)
		}
	}
	if p.Top < 179 || p.Top > 185 || q.Top <= p.Top {
		t.Fatalf("thumb %v -> %v", p, q)
	}
}
func TestBlankRosterHasNoCards(t *testing.T) {
	cards := rosterCards(image.NewRGBA(image.Rect(0, 0, 1280, 960)))
	if len(cards) != 0 {
		t.Fatalf("blank image yielded %d cards", len(cards))
	}
	detail, err := loadPNG("../../fixtures/detail-saori.png")
	if err != nil {
		t.Fatal(err)
	}
	valid, err := rosterVisible(detail, "", "")
	if err != nil {
		t.Fatal(err)
	}
	if valid {
		t.Fatal("detail mistaken for roster")
	}
}
func TestRosterFixtureExists(t *testing.T) {
	for _, f := range []string{"../../fixtures/roster-top.png", "../../fixtures/roster-scroll1.png"} {
		if _, err := os.Stat(f); err != nil {
			t.Fatal(err)
		}
	}
}

func TestSaveReportRendersHTML(t *testing.T) {
	pin := true
	r := RosterReport{SchemaVersion: 3, Entries: []RosterEntry{{ID: "001", Student: Student{Name: "리오", IsPinned: &pin}}}}
	dir := t.TempDir()
	if err := saveReport(r, dir); err != nil {
		t.Fatal(err)
	}
	if _, err := os.Stat(filepath.Join(dir, "students.html")); err != nil {
		t.Fatal(err)
	}
}
