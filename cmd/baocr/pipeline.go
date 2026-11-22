package main

import (
	"encoding/json"
	"errors"
	"fmt"
	"image"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"sync"
)

// Asynchronous extraction.
//
// The capture loop (clicks, screenshots) never waits for OCR: it hands each
// student's frames to a worker pool and moves on. Every student gets an entry
// in out/index.json (captured -> extracted | failed) and its frames stay on
// disk under details/NNN/, so a failed or doubtful student can be re-extracted
// later with --reextract without touching the game.

const (
	statusCaptured  = "captured"
	statusExtracted = "extracted"
	statusFailed    = "failed"
)

// IndexEntry tracks one student through capture and extraction.
type IndexEntry struct {
	ID       string   `json:"id"`
	Status   string   `json:"status"`
	Name     string   `json:"name,omitempty"`
	Attempts int      `json:"attempts"`
	Error    string   `json:"error,omitempty"`
	Frames   []string `json:"frames,omitempty"`
	Warnings int      `json:"warnings,omitempty"`
}

// Index is the on-disk work list, safe for concurrent workers.
type Index struct {
	path string
	mu   sync.Mutex
	m    map[string]*IndexEntry
}

func newIndex(out string) *Index {
	return &Index{path: filepath.Join(out, "index.json"), m: map[string]*IndexEntry{}}
}

func loadIndex(out string) (*Index, error) {
	ix := newIndex(out)
	b, err := os.ReadFile(ix.path)
	if err != nil {
		return nil, err
	}
	var f struct {
		Entries []IndexEntry `json:"entries"`
	}
	if err = json.Unmarshal(b, &f); err != nil {
		return nil, err
	}
	for i := range f.Entries {
		e := f.Entries[i]
		ix.m[e.ID] = &e
	}
	return ix, nil
}

// update applies fn to the entry (creating it) and persists the index.
func (ix *Index) update(id string, fn func(*IndexEntry)) {
	ix.mu.Lock()
	defer ix.mu.Unlock()
	e := ix.m[id]
	if e == nil {
		e = &IndexEntry{ID: id}
		ix.m[id] = e
	}
	fn(e)
	_ = ix.saveLocked()
}

func (ix *Index) sorted() []IndexEntry {
	ix.mu.Lock()
	defer ix.mu.Unlock()
	return ix.sortedLocked()
}

func (ix *Index) sortedLocked() []IndexEntry {
	out := make([]IndexEntry, 0, len(ix.m))
	for _, e := range ix.m {
		out = append(out, *e)
	}
	sort.Slice(out, func(i, j int) bool { return out[i].ID < out[j].ID })
	return out
}

func (ix *Index) saveLocked() error {
	b, err := json.MarshalIndent(struct {
		Version int          `json:"version"`
		Entries []IndexEntry `json:"entries"`
	}{1, ix.sortedLocked()}, "", "  ")
	if err != nil {
		return err
	}
	if err = os.MkdirAll(filepath.Dir(ix.path), 0o755); err != nil {
		return err
	}
	return os.WriteFile(ix.path, b, 0o644)
}

// ---- frames on disk -----------------------------------------------------------

func studentID(i int) string { return fmt.Sprintf("%03d", i+1) }

func detailDir(out, id string) string { return filepath.Join(out, "details", id) }

func saveFrames(dir string, frames map[string]image.Image) ([]string, error) {
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return nil, err
	}
	var names []string
	for name, img := range frames {
		if err := savePNG(filepath.Join(dir, name+".png"), img); err != nil {
			return nil, err
		}
		names = append(names, name)
	}
	sort.Strings(names)
	return names, nil
}

func loadFrames(dir string, names []string) (map[string]image.Image, error) {
	frames := map[string]image.Image{}
	for _, n := range names {
		img, err := loadPNG(filepath.Join(dir, n+".png"))
		if err != nil {
			return nil, err
		}
		frames[n] = img
	}
	return frames, nil
}

// ---- extraction from frames ---------------------------------------------------

// extractFrames turns captured frames into a Student. A menu-pass failure keeps
// the base profile (so nothing is lost) and returns the error for the index.
func extractFrames(frames map[string]image.Image, tess, data, source string, opts Options, deep bool) (Student, error) {
	detail := frames[frameDetail]
	if detail == nil {
		return Student{}, errors.New("detail frame missing")
	}
	s, err := extractWith(detail, tess, data, source, opts)
	if err != nil {
		return Student{}, err
	}
	s.frames = frames
	if deep {
		if err = extractDeep(frames, tess, data, &s); err != nil {
			s.Warnings = append(s.Warnings, "deep_failed: "+err.Error())
			return s, err
		}
	}
	return s, nil
}

// ---- worker pool --------------------------------------------------------------

type job struct {
	i      int
	frames map[string]image.Image
}

type outcome struct {
	student Student
	err     error
}

type pipeline struct {
	out, tess, data string
	opts            Options
	deep            bool
	idx             *Index
	jobs            chan job
	wg              sync.WaitGroup

	mu      sync.Mutex
	results map[int]*outcome
	done    map[int]chan struct{}
	account *Account
	abort   error
	reason  string
}

func newPipeline(out, tess, data string, opts Options, deep bool, workers int) *pipeline {
	if workers < 1 {
		workers = 1
	}
	p := &pipeline{out: out, tess: tess, data: data, opts: opts, deep: deep, idx: newIndex(out),
		jobs: make(chan job, workers*2), results: map[int]*outcome{}, done: map[int]chan struct{}{}}
	for w := 0; w < workers; w++ {
		p.wg.Add(1)
		go func() {
			defer p.wg.Done()
			for j := range p.jobs {
				p.process(j)
			}
		}()
	}
	return p
}

// submit queues a captured student; it blocks only when every worker is busy
// and the queue is full (bounded memory).
func (p *pipeline) submit(i int, frames map[string]image.Image) {
	p.mu.Lock()
	p.done[i] = make(chan struct{})
	p.mu.Unlock()
	p.idx.update(studentID(i), func(e *IndexEntry) { e.Status = statusCaptured })
	p.jobs <- job{i, frames}
}

func (p *pipeline) process(j job) {
	id := studentID(j.i)
	dir := detailDir(p.out, id)
	oc := &outcome{}
	defer func() {
		p.mu.Lock()
		p.results[j.i] = oc
		ch := p.done[j.i]
		p.checkAbortLocked(j.i)
		p.mu.Unlock()
		close(ch)
	}()
	names, err := saveFrames(dir, j.frames)
	if err != nil {
		oc.err = err
		p.idx.update(id, func(e *IndexEntry) { e.Status, e.Error = statusFailed, "save frames: "+err.Error() })
		return
	}
	p.idx.update(id, func(e *IndexEntry) { e.Frames = names; e.Attempts++ })
	s, err := extractFrames(j.frames, p.tess, p.data, fmt.Sprintf("profile arrow #%d", j.i+1), p.opts, p.deep)
	oc.student, oc.err = s, err
	if j.i == 0 {
		if acc, e := readAccount(j.frames[frameDetail], p.tess, p.data); e == nil {
			p.mu.Lock()
			p.account = acc
			p.mu.Unlock()
		}
	}
	if s.Name != "" || err == nil {
		if e := exportStudent(s, dir); e != nil && err == nil {
			oc.err, err = e, e
		}
	}
	p.idx.update(id, func(e *IndexEntry) {
		e.Name, e.Warnings = s.Name, len(s.Warnings)
		if err != nil {
			e.Status, e.Error = statusFailed, err.Error()
		} else {
			e.Status, e.Error = statusExtracted, ""
		}
	})
	status := "ok"
	if err != nil {
		status = "FAILED: " + err.Error()
	} else if s.Name == "" {
		status = "unresolved_name"
	}
	fmt.Printf("  extracted %s %s %s pin=%s role=%s\n", id, s.Name, status, pinCell(s.IsPinned), s.CombatRole)
}

// checkAbortLocked stops the scan after two consecutive unreadable names.
func (p *pipeline) checkAbortLocked(i int) {
	cur, prev := p.results[i], p.results[i-1]
	if i == 0 || cur == nil || prev == nil || p.abort != nil {
		return
	}
	if cur.student.Name == "" && prev.student.Name == "" {
		p.abort, p.reason = errors.New("two consecutive profiles had no readable name; stopping"), "two_empty_names"
	}
}

// aborted reports a stop request raised by a worker.
func (p *pipeline) aborted() (string, error) {
	p.mu.Lock()
	defer p.mu.Unlock()
	return p.reason, p.abort
}

// wait blocks until student i has been extracted.
func (p *pipeline) wait(i int) (Student, error) {
	p.mu.Lock()
	ch := p.done[i]
	p.mu.Unlock()
	if ch == nil {
		return Student{}, fmt.Errorf("student %d was never submitted", i+1)
	}
	<-ch
	p.mu.Lock()
	defer p.mu.Unlock()
	return p.results[i].student, p.results[i].err
}

// close drains the queue and waits for every worker.
func (p *pipeline) close() {
	close(p.jobs)
	p.wg.Wait()
}

// ---- report from disk ---------------------------------------------------------

func loadStudent(dir string) (Student, error) {
	b, err := os.ReadFile(filepath.Join(dir, "student.json"))
	if err != nil {
		return Student{}, err
	}
	var s Student
	return s, json.Unmarshal(b, &s)
}

// rebuildReport assembles the roster report from the index and the per-student
// files, so a scan and a later --reextract produce the same report.
func rebuildReport(out string, ix *Index, base RosterReport) RosterReport {
	base.Entries, base.Unresolved = []RosterEntry{}, []string{}
	for n, e := range ix.sorted() {
		entry := RosterEntry{ID: e.ID, Column: n, DetailPath: "details/" + e.ID, Status: "failed"}
		if s, err := loadStudent(detailDir(out, e.ID)); err == nil {
			entry.Student = s
			entry.Status = "detail_ocr"
			if s.Name == "" {
				entry.Status = "unresolved_name"
				base.Unresolved = append(base.Unresolved, e.ID)
			}
		} else {
			base.Unresolved = append(base.Unresolved, e.ID)
		}
		if e.Status == statusFailed {
			entry.Status = "failed"
		}
		base.Entries = append(base.Entries, entry)
	}
	return base
}

// ---- re-extraction -----------------------------------------------------------

// reextract re-runs extraction from the saved frames of the selected students
// (all non-extracted ones when ids is empty). It never touches the game.
func reextract(out string, ids []string, tess, data string, opts Options, deep bool, workers int) (RosterReport, error) {
	ix, err := loadIndex(out)
	if err != nil {
		return RosterReport{}, fmt.Errorf("no index in %s: %w", out, err)
	}
	want := map[string]bool{}
	for _, id := range ids {
		want[strings.TrimSpace(id)] = true
	}
	var todo []IndexEntry
	for _, e := range ix.sorted() {
		if (len(want) > 0 && want[e.ID]) || (len(want) == 0 && e.Status != statusExtracted) {
			todo = append(todo, e)
		}
	}
	ch := make(chan IndexEntry)
	var wg sync.WaitGroup
	if workers < 1 {
		workers = 1
	}
	for w := 0; w < workers; w++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			for e := range ch {
				dir := detailDir(out, e.ID)
				frames, err := loadFrames(dir, e.Frames)
				var s Student
				if err == nil {
					s, err = extractFrames(frames, tess, data, "re-extract "+e.ID, opts, deep)
				}
				if s.Name != "" || err == nil {
					if ex := exportStudent(s, dir); ex != nil && err == nil {
						err = ex
					}
				}
				ix.update(e.ID, func(x *IndexEntry) {
					x.Attempts++
					x.Name, x.Warnings = s.Name, len(s.Warnings)
					if err != nil {
						x.Status, x.Error = statusFailed, err.Error()
					} else {
						x.Status, x.Error = statusExtracted, ""
					}
				})
				fmt.Printf("  re-extracted %s %s err=%v\n", e.ID, s.Name, err)
			}
		}()
	}
	for _, e := range todo {
		ch <- e
	}
	close(ch)
	wg.Wait()
	base := RosterReport{SchemaVersion: 3, StopReason: "reextracted"}
	if b, err := os.ReadFile(filepath.Join(out, "students.json")); err == nil {
		_ = json.Unmarshal(b, &base) // keep Complete/StopReason/Account/Capture from the scan
	}
	rep := rebuildReport(out, ix, base)
	return rep, saveReport(rep, out)
}
