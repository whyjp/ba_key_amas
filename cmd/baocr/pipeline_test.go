package main

import (
	"image"
	"os"
	"path/filepath"
	"testing"
)

func fixtureFrames(t *testing.T, detail string) map[string]image.Image {
	t.Helper()
	load := func(f string) image.Image {
		img, err := loadPNG(filepath.Join("..", "..", "fixtures", f))
		if err != nil {
			t.Fatal(err)
		}
		return img
	}
	return map[string]image.Image{
		frameDetail: load(detail), framePopupTop: load("popup-stats-top.png"), framePopupBottom: load("popup-stats-bottom.png"),
		frameLevelUp: load("tab-levelup.png"), frameMystic: load("tab-mystic.png"),
	}
}

func TestAsyncPipelineAndReextract(t *testing.T) {
	bin, data := os.Getenv("TESSERACT_BIN"), os.Getenv("TESSDATA_DIR")
	if bin == "" || data == "" {
		t.Skip("requires Tesseract and models")
	}
	out := t.TempDir()
	p := newPipeline(out, bin, data, Options{}, true, 2)
	p.submit(0, fixtureFrames(t, "detail-shun.png"))
	p.submit(1, fixtureFrames(t, "detail-rio-armed.png"))
	if st, err := p.wait(0); err != nil || st.Name != "슌" {
		t.Fatalf("student 0: %q err=%v", st.Name, err)
	}
	p.close()

	// Index: both extracted, frames kept on disk for re-extraction.
	for _, e := range p.idx.sorted() {
		if e.Status != statusExtracted || len(e.Frames) != 5 || e.Attempts != 1 {
			t.Errorf("index entry %+v", e)
		}
		for _, f := range e.Frames {
			if _, err := os.Stat(filepath.Join(detailDir(out, e.ID), f+".png")); err != nil {
				t.Errorf("frame %s missing: %v", f, err)
			}
		}
	}
	rep := rebuildReport(out, p.idx, RosterReport{SchemaVersion: 3})
	if len(rep.Entries) != 2 || rep.Entries[0].Student.Name != "슌" || rep.Entries[1].Student.Name != "리오(무장)" || len(rep.Unresolved) != 0 {
		t.Fatalf("report: %+v", rep.Unresolved)
	}
	if rep.Entries[0].Student.Deep == nil || rep.Entries[0].Student.Deep.StatDetail == nil {
		t.Error("menu pass not parsed from frames")
	}

	// Simulate a failed extraction: drop student 2's output and mark it failed.
	if err := os.Remove(filepath.Join(detailDir(out, "002"), "student.json")); err != nil {
		t.Fatal(err)
	}
	p.idx.update("002", func(e *IndexEntry) { e.Status, e.Error = statusFailed, "simulated" })
	rep, err := reextract(out, nil, bin, data, Options{}, true, 2)
	if err != nil {
		t.Fatal(err)
	}
	if len(rep.Entries) != 2 || rep.Entries[1].Student.Name != "리오(무장)" || rep.Entries[1].Status != "detail_ocr" {
		t.Fatalf("after reextract: %+v", rep.Entries[1].Status)
	}
	ix, _ := loadIndex(out)
	for _, e := range ix.sorted() {
		if e.ID == "002" && (e.Status != statusExtracted || e.Attempts != 2) {
			t.Errorf("reextracted entry %+v", e)
		}
		if e.ID == "001" && e.Attempts != 1 {
			t.Errorf("untouched entry was re-run: %+v", e)
		}
	}
}
