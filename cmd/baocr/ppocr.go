package main

import (
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	xdraw "golang.org/x/image/draw"
	"image"
	"image/color"
	"image/png"
	"io"
	"net/http"
	"regexp"
	"sort"
	"strconv"
	"strings"
	"time"
)

// A second OCR engine (PP-OCR) behind a frame-level interface.
//
// Unlike Tesseract's per-crop calls, a frame-level recognizer detects text on
// the whole frame once and hands back boxes; field readers then pick boxes by
// region. Tesseract keeps working through the existing ROI readers, so both
// engines can run side by side and be compared on the same frames.

// TextBox is one recognised text run in frame coordinates.
type TextBox struct {
	Rect  image.Rectangle
	Text  string
	Score float64
}

// Recognizer is the engine-neutral surface for frame-level reading.
type Recognizer interface {
	// Boxes detects and recognises all text on a frame.
	Boxes(img image.Image) ([]TextBox, error)
	// Line recognises one already-cropped text line (no detection).
	Line(img image.Image) (string, float64, error)
}

// sidecarRecognizer talks to tools/ocr-sidecar/server.py (development backend).
type sidecarRecognizer struct {
	url    string
	client *http.Client
}

func newSidecarRecognizer(url string) *sidecarRecognizer {
	return &sidecarRecognizer{url: strings.TrimRight(url, "/"), client: &http.Client{Timeout: 60 * time.Second}}
}

func (s *sidecarRecognizer) post(path string, img image.Image, out any) error {
	var buf bytes.Buffer
	if err := png.Encode(&buf, img); err != nil {
		return err
	}
	resp, err := s.client.Post(s.url+path, "image/png", &buf)
	if err != nil {
		return fmt.Errorf("OCR sidecar unreachable at %s (start tools/ocr-sidecar/server.py): %w", s.url, err)
	}
	defer resp.Body.Close()
	body, _ := io.ReadAll(resp.Body)
	if resp.StatusCode != http.StatusOK {
		return fmt.Errorf("OCR sidecar %s: %s", path, strings.TrimSpace(string(body)))
	}
	return json.Unmarshal(body, out)
}

func (s *sidecarRecognizer) Boxes(img image.Image) ([]TextBox, error) {
	var generic []map[string]any // the server uses lower-case keys
	if err := s.post("/boxes", img, &generic); err != nil {
		return nil, err
	}
	out := make([]TextBox, 0, len(generic))
	f := func(m map[string]any, k string) float64 { v, _ := m[k].(float64); return v }
	for _, m := range generic {
		t, _ := m["text"].(string)
		out = append(out, TextBox{
			Rect:  image.Rect(int(f(m, "x0")), int(f(m, "y0")), int(f(m, "x1")+0.5), int(f(m, "y1")+0.5)),
			Text:  t,
			Score: f(m, "score"),
		})
	}
	return out, nil
}

func (s *sidecarRecognizer) Line(img image.Image) (string, float64, error) {
	var r struct {
		Text  string  `json:"text"`
		Score float64 `json:"score"`
	}
	if err := s.post("/line", img, &r); err != nil {
		return "", 0, err
	}
	return r.Text, r.Score, nil
}

// textIn joins the boxes whose centre lies in roi, left to right.
func textIn(boxes []TextBox, roi image.Rectangle) string {
	type hit struct {
		x int
		t string
	}
	var hits []hit
	grown := roi.Inset(-4)
	for _, b := range boxes {
		c := image.Pt((b.Rect.Min.X+b.Rect.Max.X)/2, (b.Rect.Min.Y+b.Rect.Max.Y)/2)
		if c.In(grown) {
			hits = append(hits, hit{b.Rect.Min.X, b.Text})
		}
	}
	sort.Slice(hits, func(i, j int) bool { return hits[i].x < hits[j].x })
	var sb strings.Builder
	for _, h := range hits {
		sb.WriteString(h.t)
	}
	return sb.String()
}

var intRun = regexp.MustCompile(`\d+`)

func lastInt(s string) *int {
	m := intRun.FindAllString(strings.ReplaceAll(s, ",", ""), -1)
	if len(m) == 0 {
		return nil
	}
	n, err := strconv.Atoi(m[len(m)-1])
	if err != nil {
		return nil
	}
	return &n
}

// readCropDet reads a small overlaid text region (name strip, bond heart) that
// frame-level detection misses: take the region with real surrounding pixels
// as padding, enlarge it 2x, and run detection+recognition on that crop. This
// is the setup that read all 8 hearts in the benchmark; recognition-only on a
// tight crop did not.
func readCropDet(rec Recognizer, img image.Image, roi image.Rectangle) (string, error) {
	padded := roi.Inset(-12).Intersect(img.Bounds())
	src := crop(img, padded)
	dst := image.NewRGBA(image.Rect(0, 0, src.Bounds().Dx()*2, src.Bounds().Dy()*2))
	xdraw.CatmullRom.Scale(dst, dst.Bounds(), src, src.Bounds(), xdraw.Over, nil)
	boxes, err := rec.Boxes(dst)
	if err != nil {
		return "", err
	}
	// boxes are in the enlarged crop: map the original roi into it.
	scaled := image.Rect((roi.Min.X-padded.Min.X)*2, (roi.Min.Y-padded.Min.Y)*2, (roi.Max.X-padded.Min.X)*2, (roi.Max.Y-padded.Min.Y)*2)
	return textIn(boxes, scaled), nil
}

// PPReading is what the frame-level engine read from one profile frame.
type PPReading struct {
	Name     string
	NameRaw  string
	Level    *int
	Skills   [4]*int
	Bond     *int
	Stats    Stats
	Exp      Exp
	BoxCount int
}

var (
	ppStatROIs  = [4]image.Rectangle{image.Rect(684, 350, 860, 374), image.Rect(890, 350, 1060, 374), image.Rect(684, 386, 860, 410), image.Rect(890, 386, 1060, 410)}
	ppSkillROIs = [4]image.Rectangle{image.Rect(694, 520, 767, 548), image.Rect(800, 520, 883, 548), image.Rect(908, 520, 987, 548), image.Rect(1010, 520, 1100, 548)}
)

// readProfilePP reads a profile with one frame-level pass for the panel numbers
// and recognition-only reads of the two small crops frame detection misses
// (the name strip and the bond heart).
func readProfilePP(rec Recognizer, img image.Image) (PPReading, error) {
	var r PPReading
	boxes, err := rec.Boxes(img)
	if err != nil {
		return r, err
	}
	r.BoxCount = len(boxes)
	for i, roi := range ppStatROIs {
		v := lastInt(textIn(boxes, roi))
		switch i {
		case 0:
			r.Stats.MaxHP = v
		case 1:
			r.Stats.Attack = v
		case 2:
			r.Stats.Defense = v
		case 3:
			r.Stats.Healing = v
		}
	}
	locked := readSkillLocks(img)
	lockedAt := [4]bool{locked.EX, locked.Basic, locked.Passive, locked.Sub}
	for i, roi := range ppSkillROIs {
		if !lockedAt[i] {
			r.Skills[i] = lastInt(textIn(boxes, roi))
		}
	}
	if m := intRun.FindAllString(strings.ReplaceAll(textIn(boxes, image.Rect(210, 833, 390, 858)), ",", ""), -1); len(m) >= 2 {
		cur, req := atoiLoose(m[0]), atoiLoose(m[1])
		if cur != nil && req != nil && *req > 0 && *cur <= *req {
			r.Exp = Exp{Current: cur, Required: req}
		}
	}
	r.Level = lastInt(textIn(boxes, image.Rect(30, 827, 104, 865)))

	// Two independent reads of the small overlaid text; the dictionary picks.
	nameInk, _, err := rec.Line(inkLine(nameInkImage(img)))
	if err != nil {
		return r, err
	}
	nameDet, err := readCropDet(rec, img, image.Rect(64, 789, 300, 835))
	if err != nil {
		return r, err
	}
	r.NameRaw = nameInk + " | " + nameDet
	r.Name = bestName(nameInk, nameDet)
	heartInk, _, err := rec.Line(inkLine(heartInkImage(img)))
	if err != nil {
		return r, err
	}
	heartDet, err := readCropDet(rec, img, image.Rect(34, 794, 84, 832))
	if err != nil {
		return r, err
	}
	r.Bond = bestBond(heartInk, heartDet)
	if len(boxes) == 0 {
		return r, errors.New("frame-level OCR returned no text")
	}
	return r, nil
}

// inkLine renders an isolated-ink gray image as a padded, enlarged text line
// for a recognition-only call.
func inkLine(bw *image.Gray) image.Image {
	const pad, k = 8, 1 // the sidecar enlarges lines itself
	w, h := bw.Bounds().Dx(), bw.Bounds().Dy()
	out := image.NewRGBA(image.Rect(0, 0, (w+2*pad)*k, (h+2*pad)*k))
	for i := 0; i < len(out.Pix); i += 4 {
		out.Pix[i], out.Pix[i+1], out.Pix[i+2], out.Pix[i+3] = 255, 255, 255, 255
	}
	for y := 0; y < h*k; y++ {
		for x := 0; x < w*k; x++ {
			v := bw.GrayAt(x/k, y/k).Y
			out.SetRGBA(x+pad*k, y+pad*k, color.RGBA{v, v, v, 255})
		}
	}
	return out
}

// nameInkImage isolates the white name glyphs on the name strip.
func nameInkImage(img image.Image) *image.Gray {
	r := image.Rect(64, 789, 300, 835)
	bw := image.NewGray(image.Rect(0, 0, r.Dx(), r.Dy()))
	for y := 0; y < r.Dy(); y++ {
		for x := 0; x < r.Dx(); x++ {
			v := uint8(255)
			if whiteInk(rgba(img, r.Min.X+x, r.Min.Y+y)) {
				v = 0
			}
			bw.SetGray(x, y, color.Gray{Y: v})
		}
	}
	return bw
}

// bestName resolves each candidate against the dictionary and keeps the closest
// (exact beats corrected beats anything else).
func bestName(cands ...string) string {
	rank := map[string]int{"exact": 3, "corrected": 2, "unlisted": 1}
	best, bestRank, bestDist := "", 0, 1<<30
	for _, c := range cands {
		n, m := defaultResolver.Resolve(c)
		if n == "" {
			continue
		}
		if rk := rank[m.Status]; rk > bestRank || (rk == bestRank && m.Distance < bestDist) {
			best, bestRank, bestDist = n, rk, m.Distance
		}
	}
	return best
}

// bestBond keeps a plausible 1..100 read; when the two reads disagree the
// ink-isolated one wins (it saw only the heart's glyphs).
func bestBond(ink, det string) *int {
	ok := func(s string) *int {
		if v := lastInt(s); v != nil && *v >= 1 && *v <= 100 {
			return v
		}
		return nil
	}
	if v := ok(ink); v != nil {
		return v
	}
	return ok(det)
}
