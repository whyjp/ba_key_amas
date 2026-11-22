package main

import (
	"errors"
	"fmt"
	"image"
	"time"

	xdraw "golang.org/x/image/draw"
)

func detailKey(img image.Image) [2304]uint8 {
	src := crop(img, image.Rect(65, 794, 304, 837))
	dst := image.NewRGBA(image.Rect(0, 0, 64, 12))
	xdraw.CatmullRom.Scale(dst, dst.Bounds(), src, src.Bounds(), xdraw.Over, nil)
	var key [2304]uint8
	k := 0
	for y := 0; y < 12; y++ {
		for x := 0; x < 64; x++ {
			c := dst.RGBAAt(x, y)
			key[k] = c.R
			key[k+1] = c.G
			key[k+2] = c.B
			k += 3
		}
	}
	return key
}
func detailDistance(a, b [2304]uint8) float64 {
	sum := 0
	for i := range a {
		sum += abs(int(a[i]) - int(b[i]))
	}
	return float64(sum) / float64(len(a))
}
func scanArrows(h HCU, tesseract, tessdata string, region [4]float64, out string, maxStudents int, fromFirst, deep bool, opts Options, workers int) (RosterReport, error) {
	base := RosterReport{SchemaVersion: 3, Entries: []RosterEntry{}, Unresolved: []string{}, StopReason: "running"}
	c := newCapture(region)
	base.Capture = &c
	if err := h.activate(); err != nil {
		return base, err
	}
	img, err := gameCapture(h, region)
	if err != nil {
		return base, err
	}
	if visible, _ := rosterVisible(img, tesseract, tessdata); visible {
		return base, errors.New("open the first student profile before --scan-arrows")
	}
	p := newPipeline(out, tesseract, tessdata, opts, deep, workers)
	// finish drains the extraction workers, then writes the report from disk.
	finish := func(reason string, wrapped bool, retErr error) (RosterReport, error) {
		p.close()
		p.mu.Lock()
		base.Account = p.account
		p.mu.Unlock()
		base.StopReason = reason
		rep := rebuildReport(out, p.idx, base)
		rep.Complete = wrapped && fromFirst && len(rep.Unresolved) == 0
		if err := saveReport(rep, out); err != nil && retErr == nil {
			retErr = err
		}
		return rep, retErr
	}
	first := detailKey(img)
	var seen [][2304]uint8
	// quickName reads only the name strip; used to confirm a repeat/wrap by name.
	quickName := func(im image.Image) string {
		raw, _ := ocr(tesseract, tessdata, im, image.Rect(64, 789, 300, 835), "kor+eng", 7)
		n, _ := readName(im, tesseract, tessdata, raw, map[string]string{})
		return n
	}
	nameOf := func(i int) string { s, _ := p.wait(i); return s.Name }
	for i := 0; i < maxStudents; i++ {
		if err := h.foreground(); err != nil {
			return finish("focus_lost", false, err)
		}
		if reason, err := p.aborted(); err != nil {
			return finish(reason, false, err)
		}
		key := detailKey(img)
		for j, prior := range seen {
			// A matching fingerprint only counts as a repeat when the names agree too.
			if detailDistance(key, prior) < 2 {
				if n := quickName(img); n == "" || n == nameOf(j) {
					return finish("repeated_profile", false, errors.New("profile repeated before advancing"))
				}
			}
		}
		seen = append(seen, key)
		frames := map[string]image.Image{frameDetail: img}
		if deep {
			more, err := captureDeep(h, region, img)
			if err != nil {
				return finish("deep_failed", false, fmt.Errorf("student %d menu capture: %w", i+1, err))
			}
			for k, v := range more {
				frames[k] = v
			}
		}
		p.submit(i, frames)
		fmt.Printf("  captured %s\n", studentID(i))
		if i == 0 {
			// The first profile proves the layout before the scan goes on.
			if st, _ := p.wait(0); !layoutLooksCalibrated(st) {
				return finish("layout_mismatch", false, errors.New("first profile is unreadable: the screen is not in the calibrated 4:3 layout (set the game to 4:3, windowed)"))
			}
		}
		if i+1 == maxStudents {
			return finish("max_students", false, nil)
		}
		x, y := toFrame(region, 1250, 505)
		if err := h.click(x, y); err != nil {
			return finish("arrow_click_failed", false, err)
		}
		next, advanced, e := waitStableNext(h, region, key, tesseract, tessdata)
		if e != nil {
			return finish(e.Error(), false, e)
		}
		if !advanced {
			return finish("next_not_confirmed", false, nil)
		}
		if detailDistance(first, detailKey(next)) < 2 {
			if n := quickName(next); n != "" && n == nameOf(0) {
				return finish("wrapped_to_first", true, nil)
			}
		}
		img = next
	}
	return finish("max_students", false, nil)
}

// frameKey is a coarse whole-screen fingerprint; the slide-in/fade animation
// after a profile switch changes it until the new profile has settled.
func frameKey(img image.Image) [3072]uint8 {
	dst := image.NewRGBA(image.Rect(0, 0, 32, 32))
	xdraw.CatmullRom.Scale(dst, dst.Bounds(), img, img.Bounds(), xdraw.Over, nil)
	var k [3072]uint8
	i := 0
	for y := 0; y < 32; y++ {
		for x := 0; x < 32; x++ {
			c := dst.RGBAAt(x, y)
			k[i] = c.R
			k[i+1] = c.G
			k[i+2] = c.B
			i += 3
		}
	}
	return k
}
func frameDistance(a, b [3072]uint8) float64 {
	sum := 0
	for i := range a {
		sum += abs(int(a[i]) - int(b[i]))
	}
	return float64(sum) / float64(len(a))
}

// waitStableNext waits for the profile to change AND hold still for two
// consecutive captures, so OCR never reads a mid-transition frame.
func waitStableNext(h HCU, region [4]float64, prev [2304]uint8, tesseract, tessdata string) (image.Image, bool, error) {
	var last image.Image
	var lastKey [3072]uint8
	changed := false
	for attempt := 0; attempt < 12; attempt++ {
		time.Sleep(350 * time.Millisecond)
		cur, err := gameCapture(h, region)
		if err != nil {
			return nil, false, err
		}
		if visible, _ := rosterVisible(cur, tesseract, tessdata); visible {
			return nil, false, errors.New("left_detail_screen")
		}
		k := frameKey(cur)
		if detailDistance(prev, detailKey(cur)) >= 2 {
			changed = true
		}
		if changed && last != nil && frameDistance(lastKey, k) < 1.5 {
			return cur, true, nil
		}
		last, lastKey = cur, k
	}
	return last, false, nil
}
