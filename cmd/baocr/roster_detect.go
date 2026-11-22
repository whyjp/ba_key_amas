package main

import (
	"image"

	xdraw "golang.org/x/image/draw"
)

type Card struct {
	X         int        `json:"x"`
	Y         int        `json:"y"`
	Row       int        `json:"row"`
	Column    int        `json:"column"`
	Signature [768]uint8 `json:"-"`
}
type Thumb struct{ Top, Bottom int }

func scrollThumb(img image.Image) Thumb {
	best := Thumb{-1, -1}
	start := -1
	for y := 175; y <= 945; y++ {
		r, g, b, _ := img.At(1230, y).RGBA()
		hit := r>>8 < 180 && g>>8 < 200 && b>>8 < 210
		if hit && start < 0 {
			start = y
		}
		if (!hit || y == 945) && start >= 0 {
			end := y - 1
			if hit {
				end = y
			}
			if end-start >= 20 && end-start > best.Bottom-best.Top {
				best = Thumb{start, end}
			}
			start = -1
		}
	}
	return best
}
func cardSignature(img image.Image, cx, end int) [768]uint8 {
	area := crop(img, image.Rect(cx-55, end-170, cx+55, end-55))
	dst := image.NewRGBA(image.Rect(0, 0, 16, 16))
	xdraw.CatmullRom.Scale(dst, dst.Bounds(), area, area.Bounds(), xdraw.Over, nil)
	var sig [768]uint8
	k := 0
	for y := 0; y < 16; y++ {
		for x := 0; x < 16; x++ {
			c := dst.RGBAAt(x, y)
			sig[k] = c.R
			sig[k+1] = c.G
			sig[k+2] = c.B
			k += 3
		}
	}
	return sig
}
func signatureDistance(a, b [768]uint8) float64 {
	var sum int
	for i := range a {
		sum += abs(int(a[i]) - int(b[i]))
	}
	return float64(sum) / float64(len(a))
}
func rosterCards(img image.Image) []Card {
	if img.Bounds().Dx() != 1280 || img.Bounds().Dy() != 960 {
		return nil
	}
	type group struct{ start, end, count int }
	var groups []group
	for y := 170; y <= 930; y++ {
		a := img.At(50, y)
		b := img.At(58, y)
		c := img.At(65, y)
		ar, ag, ab, _ := a.RGBA()
		br, bg, bb, _ := b.RGBA()
		cr, cg, cb, _ := c.RGBA()
		diff := abs(int(ar>>8)-int(br>>8)) + abs(int(ag>>8)-int(bg>>8)) + abs(int(ab>>8)-int(bb>>8)) +
			abs(int(ar>>8)-int(cr>>8)) + abs(int(ag>>8)-int(cg>>8)) + abs(int(ab>>8)-int(cb>>8))
		if diff <= 120 {
			continue
		}
		if len(groups) == 0 || y-groups[len(groups)-1].end > 35 {
			groups = append(groups, group{y, y, 1})
		} else {
			g := &groups[len(groups)-1]
			g.end = y
			g.count++
		}
	}
	var cards []Card
	for _, g := range groups {
		if g.count < 60 || g.end > 920 || g.end < 290 || g.end-g.start < 90 {
			continue
		}
		row := len(cards) / 6
		for col := 0; col < 6; col++ {
			x := 140 + 195*col
			cards = append(cards, Card{X: x, Y: g.end - 90, Row: row, Column: col, Signature: cardSignature(img, x, g.end)})
		}
	}
	return cards
}
func sameCards(a, b []Card) bool {
	if len(a) != len(b) {
		return false
	}
	for i := range a {
		if signatureDistance(a[i].Signature, b[i].Signature) > 2 {
			return false
		}
	}
	return true
}
