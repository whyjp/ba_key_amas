package main

import (
	"context"
	"fmt"
	"image"
	"time"
)

// Autonomous updating.
//
// A Watcher turns a stream of frames into store updates. It never sends input:
// frames come from a FrameSource (a passive screenshot of the game window, a
// folder of files in tests, later a native window-capture API). Whenever a
// student profile is on screen and has settled, the variable fields are read
// and compared with the stored record, so a level-up, a new equipment piece or
// a bond change shows up in the data without running a scan.

// FrameSource supplies the current game frame (1280x960 client layout).
type FrameSource interface {
	Frame() (image.Image, error)
}

// hcuSource takes passive HCU screenshots (no click, no key, no scroll).
type hcuSource struct {
	h      HCU
	region [4]float64
}

func (s hcuSource) Frame() (image.Image, error) { return gameCapture(s.h, s.region) }

// Update is what one stable profile view produced.
type Update struct {
	Name    string
	New     bool
	Changes []Change
}

// Watcher reads stable profile views and stores what changed.
type Watcher struct {
	Src      FrameSource
	Store    *Store
	Tess     string
	TessData string
	Opts     Options
	Interval time.Duration
	OnUpdate func(Update)
	OnError  func(error)

	lastRead   image.Image // frame that was last extracted
	candidate  image.Image // newest frame, waiting to prove it is still
	candidateN int
}

// stillFrames is how many consecutive near-identical frames a view needs
// before it is read: transitions and animations never get extracted.
const stillFrames = 2

// Step processes one frame. It returns the update when a stable, changed
// profile view was stored, and (nil, nil) otherwise.
func (w *Watcher) Step() (*Update, error) {
	img, err := w.Src.Frame()
	if err != nil {
		return nil, err
	}
	if !detailVisible(img) { // not on a profile: forget the pending view
		w.candidate, w.candidateN, w.lastRead = nil, 0, nil
		return nil, nil
	}
	if w.candidate != nil && panelDelta(w.candidate, img) < 0.0008 && frameDistance(frameKey(w.candidate), frameKey(img)) < 3 {
		w.candidateN++
	} else {
		w.candidateN = 1
	}
	w.candidate = img
	if w.candidateN < stillFrames {
		return nil, nil
	}
	// Same view as the one already stored: nothing to do.
	if w.lastRead != nil && panelDelta(w.lastRead, img) < 0.0008 && detailDistance(detailKey(w.lastRead), detailKey(img)) < 2 {
		return nil, nil
	}
	s, err := extractWith(img, w.Tess, w.TessData, "watch", w.Opts)
	if err != nil {
		return nil, err
	}
	w.lastRead = img
	if s.Name == "" || !layoutLooksCalibrated(s) {
		return nil, nil // unreadable view: never store a guess
	}
	changes, isNew, err := w.Store.Upsert(s, time.Now())
	if err != nil {
		return nil, err
	}
	if !isNew && len(changes) == 0 {
		return nil, nil
	}
	return &Update{Name: s.Name, New: isNew, Changes: changes}, nil
}

// Run steps until ctx is cancelled.
func (w *Watcher) Run(ctx context.Context) {
	interval := w.Interval
	if interval <= 0 {
		interval = time.Second
	}
	t := time.NewTicker(interval)
	defer t.Stop()
	for {
		select {
		case <-ctx.Done():
			return
		case <-t.C:
			up, err := w.Step()
			if err != nil && w.OnError != nil {
				w.OnError(err)
			}
			if up != nil && w.OnUpdate != nil {
				w.OnUpdate(*up)
			}
		}
	}
}

func (u Update) String() string {
	if u.New {
		return fmt.Sprintf("new student %s", u.Name)
	}
	s := fmt.Sprintf("%s:", u.Name)
	for _, c := range u.Changes {
		s += fmt.Sprintf(" %s %s->%s", c.Field, c.From, c.To)
	}
	return s
}
