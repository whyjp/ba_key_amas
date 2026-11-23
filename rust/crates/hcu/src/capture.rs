use crate::hcu::{Hcu, Record};
use anyhow::{anyhow, bail, Result};
use baocr_core::model::{Capture, Student};
use image::{imageops, RgbImage};

pub use baocr_core::layout::Layout;

/// The layout a capture region has, or `None` when it is neither 4:3 nor 16:9.
pub fn detect_layout(region: [f64; 4]) -> Option<Layout> {
    Layout::detect(region)
}

/// Rejects capture regions that are neither 4:3 nor 16:9.
pub fn check_aspect(region: [f64; 4]) -> Result<Layout> {
    let (w, h) = (region[2] - region[0], region[3] - region[1]);
    if w <= 0.0 || h <= 0.0 {
        bail!("empty capture region {:?}", region);
    }
    detect_layout(region).ok_or_else(|| {
        anyhow!(
            "capture region is {:.3}:1 but the game offers 4:3 (1.333:1) and 16:9 (1.778:1) only. Set the screen ratio in the game's options (그래픽), in windowed mode, then pass its client rectangle with --region",
            w / h
        )
    })
}

/// Cheap anchor test on the first profile: unreadable panel text means the
/// on-screen layout is not the calibrated one.
pub fn layout_looks_calibrated(s: &Student) -> bool {
    s.stats.max_hp.is_some() || s.stats.attack.is_some() || s.level.is_some()
}

/// `--region x0,y0,x1,y1` in HCU frame coordinates.
pub fn parse_frame_region(text: &str) -> Result<[f64; 4]> {
    let parts: Vec<&str> = text.split(',').collect();
    if parts.len() != 4 {
        bail!("--region must have four comma-separated frame coordinates");
    }
    let mut region = [0.0; 4];
    for (i, p) in parts.iter().enumerate() {
        region[i] = p.parse::<f64>().map_err(|e| anyhow!("--region {p:?}: {e}"))?;
    }
    if region[0] >= region[2] || region[1] >= region[3] {
        bail!("--region must have positive width and height");
    }
    Ok(region)
}

/// Map a 1280x960 client coordinate to HCU frame coordinates.
pub fn to_frame(region: [f64; 4], x: i32, y: i32) -> (f64, f64) {
    let rows = detect_layout(region).map_or(960, Layout::frame_height) as f64;
    (region[0] + x as f64 * (region[2] - region[0]) / 1280.0, region[1] + y as f64 * (region[3] - region[1]) / rows)
}

/// Capture the game client as a 1280x960 frame: two zooms (top and bottom half, at
/// native detail) cropped to the region and stitched.
///
/// `zoom` looks at the live screen, so no screenshot is needed per capture: the
/// frame size comes from the last result HCU returned (every input call returns a
/// fresh frame). A screenshot is taken only when none is known yet, or when HCU
/// answers `gui_frame_stale` (the screen's layout changed) and a retry follows.
pub fn game_capture(h: &Hcu, region: [f64; 4]) -> Result<RgbImage> {
    let mut last = None;
    for attempt in 0..4 {
        match capture_once(h, region, attempt > 0) {
            Ok(img) => return Ok(img),
            Err(e) if format!("{e:#}").contains("gui_frame_stale") => {
                last = Some(e);
                std::thread::sleep(std::time::Duration::from_millis(150));
            }
            Err(e) => return Err(e),
        }
    }
    Err(last.unwrap())
}

/// One zoomed half, cropped to the game region and brought to 1280 x `rows`.
fn crop_half(h: &Hcu, z: &Record, region: [f64; 4], ya: f64, yb: f64, rows: u32) -> Result<RgbImage> {
    let [x0, _, x1, _] = region;
    if z.images.is_empty() || z.evidence.zoom.region.len() != 4 {
        bail!("HCU zoom missing image/mapping");
    }
    let src = h.load_image(&z.images[0])?;
    let zr = &z.evidence.zoom.region;
    let (sx, sy) = (z.evidence.zoom.scale_x, z.evidence.zoom.scale_y);
    // Go: image.Rect(round((x0-zr0)*sx), round((ya-zr1)*sy), round((x1-zr0)*sx), round((yb-zr1)*sy)), intersected with the image.
    let rx0 = (((x0 - zr[0]) * sx).round() as i64).clamp(0, src.width() as i64) as u32;
    let ry0 = (((ya - zr[1]) * sy).round() as i64).clamp(0, src.height() as i64) as u32;
    let rx1 = (((x1 - zr[0]) * sx).round() as i64).clamp(0, src.width() as i64) as u32;
    let ry1 = (((yb - zr[1]) * sy).round() as i64).clamp(0, src.height() as i64) as u32;
    if rx1 <= rx0 || ry1 <= ry0 {
        bail!("HCU zoom crop is empty");
    }
    let crop = imageops::crop_imm(&src, rx0, ry0, rx1 - rx0, ry1 - ry0).to_image();
    Ok(if crop.dimensions() == (1280, rows) { crop } else { imageops::resize(&crop, 1280, rows, imageops::FilterType::CatmullRom) })
}

fn capture_once(h: &Hcu, region: [f64; 4], fresh_frame: bool) -> Result<RgbImage> {
    if fresh_frame || h.frame_size().is_none() {
        h.screenshot()?;
    }
    let (fw, fh) = h.frame_size().ok_or_else(|| anyhow!("HCU reported no frame size"))?;
    let [x0, y0, x1, y1] = region;
    if x0 < 0.0 || y0 < 0.0 || x1 > fw as f64 || y1 > fh as f64 {
        bail!("game region outside HCU frame");
    }
    let layout = detect_layout(region).ok_or_else(|| anyhow!("the game region is neither 4:3 nor 16:9"))?;
    let rows = layout.frame_height() / 2;
    let mid = (y0 + y1) / 2.0;
    let zoom = |ya: f64, yb: f64| h.zoom(x0.floor() as i32 - 2, ya.floor() as i32 - 2, x1.ceil() as i32 + 2, yb.ceil() as i32 + 2);
    let top = zoom(y0, mid)?;
    // Decoding the top half overlaps the second zoom call (HCU runs in its own process).
    let (top_img, bottom_img) = std::thread::scope(|s| -> Result<(RgbImage, RgbImage)> {
        let top_job = s.spawn(|| crop_half(h, &top, region, y0, mid, rows));
        let bottom = zoom(mid, y1)?;
        let bottom_img = crop_half(h, &bottom, region, mid, y1, rows)?;
        let top_img = top_job.join().map_err(|_| anyhow!("decoding the top half panicked"))??;
        Ok((top_img, bottom_img))
    })?;
    let mut result = RgbImage::new(1280, layout.frame_height());
    imageops::replace(&mut result, &top_img, 0, 0);
    imageops::replace(&mut result, &bottom_img, 0, rows as i64);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hcu::{HcuSession, Runner};
    use image::Rgb;
    use std::sync::Mutex;

    #[test]
    fn aspect_checks() {
        assert!(check_aspect([230.36, 91.73, 685.39, 433.07]).is_ok());
        assert!(check_aspect([0.0, 0.0, 1280.0, 960.0]).is_ok());
        assert_eq!(check_aspect([0.0, 0.0, 1920.0, 1080.0]).unwrap(), Layout::Ratio169);
        assert_eq!(check_aspect([0.0, 0.0, 1280.0, 960.0]).unwrap(), Layout::Ratio43);
        assert!(check_aspect([0.0, 0.0, 1920.0, 1200.0]).is_err(), "16:10 is neither");
        assert!(check_aspect([10.0, 10.0, 10.0, 50.0]).is_err());
        assert!(check_aspect([0.0, 0.0, 1280.0 * 1.02, 960.0]).is_ok());
        assert!(check_aspect([0.0, 0.0, 1280.0 * 1.04, 960.0]).is_err());
    }

    #[test]
    fn region_parsing() {
        assert_eq!(parse_frame_region("230.36,91.73,685.39,433.07").unwrap(), [230.36, 91.73, 685.39, 433.07]);
        assert!(parse_frame_region("1,2,3").is_err());
        assert!(parse_frame_region("1,2,x,4").is_err());
        assert!(parse_frame_region("5,2,3,4").is_err());
        assert!(parse_frame_region("1,5,3,4").is_err());
    }

    #[test]
    fn to_frame_maps_client_coordinates() {
        let r = [230.36, 91.73, 685.39, 433.07];
        let (x, y) = to_frame(r, 0, 0);
        assert!((x - 230.36).abs() < 1e-9 && (y - 91.73).abs() < 1e-9);
        let (x, y) = to_frame(r, 1280, 960);
        assert!((x - 685.39).abs() < 1e-9 && (y - 433.07).abs() < 1e-9);
        let (x, y) = to_frame([100.0, 50.0, 500.0, 350.0], 640, 480);
        assert!((x - 300.0).abs() < 1e-9 && (y - 200.0).abs() < 1e-9);
    }

    #[test]
    fn layout_anchor() {
        let mut s = Student::default();
        assert!(!layout_looks_calibrated(&s));
        s.level = Some(5);
        assert!(layout_looks_calibrated(&s));
        let mut s = Student::default();
        s.stats.attack = Some(1);
        assert!(layout_looks_calibrated(&s));
    }

    #[test]
    fn new_capture_fields() {
        let c = new_capture([1.0, 2.0, 3.0, 4.0]);
        assert_eq!((c.aspect_ratio.as_str(), c.window_mode.as_str(), c.frame_region), ("4:3", "windowed", [1.0, 2.0, 3.0, 4.0]));
    }

    /// Screen 1000x600, scale 2: a zoom of (X0,Y0,X1,Y1) is a (2*(X1-X0)) x (2*(Y1-Y0)) image.
    /// The padding outside the region is blue; top half red, bottom half green.
    struct Geo {
        log: std::sync::Arc<Mutex<Vec<Vec<String>>>>,
    }
    impl Runner for Geo {
        fn run(&self, args: &[String]) -> anyhow::Result<String> {
            self.log.lock().unwrap().push(args.to_vec());
            if args[1] == "screenshot" {
                return Ok(r#"{"ok":true,"screen":{"width":1000,"height":600}}"#.into());
            }
            let n: Vec<f64> = args[2..6].iter().map(|s| s.parse().unwrap()).collect();
            Ok(format!(
                r#"{{"ok":true,"images":["z:{}:{}:{}:{}"],"evidence":{{"zoom":{{"region":[{},{},{},{}],"scale_x":2.0,"scale_y":2.0}}}}}}"#,
                n[0], n[1], n[2], n[3], n[0], n[1], n[2], n[3]
            ))
        }
        fn load_image(&self, path: &str) -> anyhow::Result<RgbImage> {
            let n: Vec<f64> = path[2..].split(':').map(|s| s.parse().unwrap()).collect();
            let (w, h) = (((n[2] - n[0]) * 2.0) as u32, ((n[3] - n[1]) * 2.0) as u32);
            // region content spans pad(=2px*2) .. ; the top zoom ends at mid(200)+2, the bottom starts at mid-2.
            let top = n[1] == 48.0;
            let c = if top { Rgb([200, 0, 0]) } else { Rgb([0, 200, 0]) };
            let mut im = RgbImage::from_pixel(w, h, Rgb([0, 0, 255]));
            // inside the requested region (x 100..500 -> px 4..804; first/last rows excluded by the crop rect)
            let (cy0, cy1) = if top { (4, h - 4) } else { (4, h - 4) };
            for y in cy0..cy1 {
                for x in 4..w - 4 {
                    im.put_pixel(x, y, c);
                }
            }
            Ok(im)
        }
    }

    #[test]
    fn game_capture_stitches_halves() {
        let h = HcuSession::with_runner(Box::new(Geo { log: Default::default() }));
        let img = game_capture(&h, [100.0, 50.0, 500.0, 350.0]).unwrap();
        assert_eq!(img.dimensions(), (1280, 960));
        // No blue padding leaked into the stitched frame, halves are the right colour.
        for (x, y) in [(0, 0), (1279, 0), (0, 479), (1279, 479), (640, 240)] {
            assert_eq!(img.get_pixel(x, y).0, [200, 0, 0], "top {x},{y}");
        }
        for (x, y) in [(0, 480), (1279, 480), (0, 959), (1279, 959), (640, 700)] {
            assert_eq!(img.get_pixel(x, y).0, [0, 200, 0], "bottom {x},{y}");
        }
    }

    #[test]
    fn game_capture_zoom_arguments() {
        let geo = Geo { log: Default::default() };
        let log = geo.log.clone();
        let h = HcuSession::with_runner(Box::new(geo));
        game_capture(&h, [100.4, 50.0, 500.2, 350.0]).unwrap();
        game_capture(&h, [100.4, 50.0, 500.2, 350.0]).unwrap();
        let log = log.lock().unwrap();
        let cmds: Vec<String> = log.iter().map(|a| a.join(" ")).collect();
        // only the first capture needs a screenshot (to learn the frame size); zooms look at the live screen
        assert_eq!(
            cmds,
            vec![
                "gui screenshot --records",
                "gui zoom 98 48 503 202 --records",
                "gui zoom 98 198 503 352 --records",
                "gui zoom 98 48 503 202 --records",
                "gui zoom 98 198 503 352 --records"
            ]
        );
    }

    #[test]
    fn game_capture_rejects_outside_region() {
        let h = HcuSession::with_runner(Box::new(Geo { log: Default::default() }));
        assert!(game_capture(&h, [100.0, 50.0, 1100.0, 350.0]).is_err());
        assert!(game_capture(&h, [-1.0, 50.0, 500.0, 350.0]).is_err());
    }
}
