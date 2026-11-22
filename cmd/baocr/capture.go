package main

import (
	"fmt"
	"image"
	"image/color"
	"math"
)

// Every ROI in this program was calibrated on the game's 4:3 layout (a
// 1280x960 client). 16:9 moves panels and shrinks text, and fullscreen has not
// been verified, so scans refuse anything else instead of guessing.
const (
	requiredAspect = 4.0 / 3.0
	aspectTol      = 0.03
)

// Capture records the conditions a result was collected under.
type Capture struct {
	AspectRatio string     `json:"aspect_ratio"`
	FrameRegion [4]float64 `json:"frame_region"`
	WindowMode  string     `json:"window_mode"`
	Note        string     `json:"note"`
}

func newCapture(region [4]float64) Capture {
	return Capture{
		AspectRatio: "4:3",
		FrameRegion: region,
		WindowMode:  "windowed",
		Note:        "calibrated for the game's 4:3 setting in windowed mode; 16:9 and fullscreen are not supported/verified",
	}
}

// checkAspect rejects capture regions that are not 4:3.
func checkAspect(region [4]float64) error {
	w, h := region[2]-region[0], region[3]-region[1]
	if w <= 0 || h <= 0 {
		return fmt.Errorf("empty capture region %v", region)
	}
	if r := w / h; math.Abs(r-requiredAspect) > aspectTol {
		return fmt.Errorf("capture region is %.3f:1 but this scanner only supports the game's 4:3 setting (1.333:1). "+
			"Set Blue Archive to a 4:3 screen ratio in windowed mode, then pass its client rectangle with --region", r)
	}
	return nil
}

// layoutLooksCalibrated is a cheap anchor test on the first profile: if the
// panel text is unreadable, the on-screen layout is not the calibrated one.
func layoutLooksCalibrated(s Student) bool {
	return s.Stats.MaxHP != nil || s.Stats.Attack != nil || s.Level != nil
}

// detailVisible recognises the profile layout by the dark-navy 능력치 header
// band, which no roster frame has (roster portraits/backgrounds vary freely).
func detailVisible(img image.Image) bool {
	return fraction(img, image.Rect(700, 314, 1060, 330), func(c color.RGBA) bool {
		return c.R < 75 && c.G > 50 && c.G < 105 && c.B > 85 && c.B < 150
	}) > 0.6
}

// textTouchesRightEdge reports whether white name glyphs reach the right edge of
// r, i.e. the strip cut the text off instead of ending in background.
func textTouchesRightEdge(img image.Image, r image.Rectangle) bool {
	band := image.Rect(r.Max.X-4, r.Min.Y+4, r.Max.X, r.Max.Y-4)
	return fraction(img, band, func(c color.RGBA) bool { return c.R > 200 && c.G > 200 && c.B > 200 }) > 0.04
}
