package main

import (
	"embed"
	"encoding/json"
	"fmt"
	"image"
	"image/color"
	"image/draw"
	"image/png"
	"math"
	"regexp"
	"strconv"
	"strings"

	xdraw "golang.org/x/image/draw"
)

//go:embed equipment_catalog.json icons/*.png
var equipmentAssets embed.FS

type Equipment struct {
	Slot       int      `json:"slot"`
	SlotKind   string   `json:"slot_kind"`
	Equipped   *bool    `json:"equipped"`
	Type       *string  `json:"type"`
	ItemName   *string  `json:"item_name"`
	Tier       *int     `json:"tier"`
	Level      *int     `json:"level"`
	MatchError *float64 `json:"icon_match_error,omitempty"`
}
type equipmentCatalog struct {
	Source  string `json:"source"`
	Entries []struct {
		Slot int    `json:"slot"`
		Type string `json:"type"`
		Tier int    `json:"tier"`
		Name string `json:"name"`
	} `json:"entries"`
}

var tierPattern = regexp.MustCompile(`(?i)[T7I1\[]\s*(10|[1-9])`)
var levelPattern = regexp.MustCompile(`\d+`)

func equipBadge(img image.Image, i int) bool {
	x := 760 + 95*i
	hits := 0
	for y := 680; y < 699; y++ {
		for xx := x; xx < x+18; xx++ {
			r, g, b, _ := img.At(xx, y).RGBA()
			if r>>8 > 145 && g>>8 > 105 && b>>8 < 125 && r > g {
				hits++
			}
		}
	}
	return hits >= 8
}
func equipmentTier(text string) *int {
	m := tierPattern.FindStringSubmatch(strings.TrimSpace(text))
	if len(m) != 2 {
		return nil
	}
	n, _ := strconv.Atoi(m[1])
	return &n
}
func equipmentLevel(img image.Image, tesseract, tessdata string, i int) (*int, string, error) {
	// Outlined 12px text is poorly read against the blue card. Threshold the
	// 10x interpolation, and only accept a plausible numeric suffix.
	var observations []string
	for _, dx := range []int{3, 6, 9} {
		x := 683 + 95*i + dx
		r := image.Rect(x, 675, x+53, 693)
		small := crop(img, r)
		scaled := image.NewRGBA(image.Rect(0, 0, 530, 180))
		xdraw.CatmullRom.Scale(scaled, scaled.Bounds(), small, small.Bounds(), draw.Over, nil)
		for _, threshold := range []int{210, 225} {
			gray := image.NewGray(scaled.Bounds())
			for y := 0; y < 180; y++ {
				for xx := 0; xx < 530; xx++ {
					c := color.GrayModel.Convert(scaled.At(xx, y)).(color.Gray).Y
					if int(c) > threshold {
						gray.SetGray(xx, y, color.Gray{Y: 255})
					}
				}
			}
			text, err := recognizePNG(tesseract, tessdata, gray, "eng", 7)
			if err != nil {
				return nil, "", err
			}
			observations = append(observations, text)
			// Require a two-digit reading preceded by a level-like glyph. A
			// stray 6 in "l6u%4 O" is not the level; "4 O" reads 40.
			normalized := strings.ReplaceAll(strings.ToUpper(text), "O", "0")
			normalized = strings.ReplaceAll(normalized, " ", "")
			if !strings.ContainsAny(strings.ToLower(text), "luv") {
				continue
			}
			for _, token := range levelPattern.FindAllString(normalized, -1) {
				if len(token) != 2 {
					continue
				}
				n, e := strconv.Atoi(token)
				if e == nil && n >= 1 && n <= 100 {
					return &n, strings.Join(observations, " | "), nil
				}
			}
		}
	}
	return nil, strings.Join(observations, " | "), nil
}
func iconError(game image.Image, icon image.Image, x0, y0 int) float64 {
	best := math.Inf(1)
	for _, width := range []int{65, 72, 80, 88} {
		height := int(math.Round(float64(width) * float64(icon.Bounds().Dy()) / float64(icon.Bounds().Dx())))
		scaled := image.NewRGBA(image.Rect(0, 0, width, height))
		xdraw.CatmullRom.Scale(scaled, scaled.Bounds(), icon, icon.Bounds(), xdraw.Over, nil)
		for dx := -4; dx <= 12; dx += 2 {
			for dy := -10; dy <= 14; dy += 2 {
				var sum, count int
				for y := 0; y < height; y += 2 {
					for x := 0; x < width; x += 2 {
						c := scaled.RGBAAt(x, y)
						if c.A < 175 {
							continue
						}
						r, g, b, _ := game.At(x0+dx+x, y0+dy+y).RGBA()
						sum += abs(int(c.R)-int(r>>8)) + abs(int(c.G)-int(g>>8)) + abs(int(c.B)-int(b>>8))
						count += 3
					}
				}
				if count > 100 {
					score := float64(sum) / float64(count)
					if score < best {
						best = score
					}
				}
			}
		}
	}
	return best
}
func abs(v int) int {
	if v < 0 {
		return -v
	}
	return v
}
func matchEquipment(img image.Image, slot int, tier *int, catalog equipmentCatalog) (*string, *string, *int, *float64, error) {
	best, second := math.Inf(1), math.Inf(1)
	var winner *struct {
		Type, Name string
		Tier       int
	}
	origins := [3][2]int{{685, 696}, {780, 696}, {875, 692}}
	for _, entry := range catalog.Entries {
		if entry.Slot != slot || tier != nil && entry.Tier != *tier {
			continue
		}
		path := fmt.Sprintf("icons/%s-T%d.png", entry.Type, entry.Tier)
		f, err := equipmentAssets.Open(path)
		if err != nil {
			return nil, nil, nil, nil, err
		}
		icon, err := png.Decode(f)
		_ = f.Close()
		if err != nil {
			return nil, nil, nil, nil, err
		}
		at := origins[slot-1]
		score := iconError(img, icon, at[0], at[1])
		if score < best {
			second = best
			best = score
			winner = &struct {
				Type, Name string
				Tier       int
			}{entry.Type, entry.Name, entry.Tier}
		} else if score < second {
			second = score
		}
	}
	// Low score and a distinct runner-up are needed; otherwise the pixels
	// do not justify an item identity. The catalog is never used to guess.
	if winner == nil || best > 40 || second-best < 12 {
		return nil, nil, nil, nil, nil
	}
	score := best
	return &winner.Type, &winner.Name, &winner.Tier, &score, nil
}
func detectEquipment(img image.Image, tesseract, tessdata string) ([]Equipment, map[string]string, error) {
	var catalog equipmentCatalog
	data, err := equipmentAssets.ReadFile("equipment_catalog.json")
	if err != nil {
		return nil, nil, err
	}
	if err = json.Unmarshal(data, &catalog); err != nil {
		return nil, nil, err
	}
	raw := map[string]string{}
	result := make([]Equipment, 0, 4)
	for i := 0; i < 3; i++ {
		e := Equipment{Slot: i + 1, SlotKind: "standard"}
		if equipBadge(img, i) {
			yes := true
			e.Equipped = &yes
		}
		x := 685 + 95*i
		tierText, err := ocr(tesseract, tessdata, img, image.Rect(x, 737, x+32, 757), "eng", 13)
		if err != nil {
			return nil, nil, err
		}
		raw[fmt.Sprintf("equipment_%d_tier", i+1)] = tierText
		e.Tier = equipmentTier(tierText)
		if e.Equipped != nil {
			e.Level, raw[fmt.Sprintf("equipment_%d_level", i+1)], err = equipmentLevel(img, tesseract, tessdata, i)
			if err != nil {
				return nil, nil, err
			}
			kind, name, matchTier, score, err := matchEquipment(img, i+1, e.Tier, catalog)
			if err != nil {
				return nil, nil, err
			}
			e.Type, e.ItemName, e.MatchError = kind, name, score
			if e.Tier == nil {
				e.Tier = matchTier
			}
		}
		result = append(result, e)
	}
	extra := Equipment{Slot: 4, SlotKind: "unique_gear"}
	text, err := ocr(tesseract, tessdata, img, image.Rect(969, 706, 1046, 740), "eng", 7)
	if err != nil {
		return nil, nil, err
	}
	raw["equipment_4_state"] = text
	if strings.Contains(strings.ToUpper(text), "EMPTY") {
		no := false
		extra.Equipped = &no
	}
	result = append(result, extra)
	return result, raw, nil
}
