//! Spike: can oar-ocr run Korean PP-OCRv5 in-process on a saved profile frame?
use oar_ocr::oarocr::OAROCRBuilder;
use std::time::Instant;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let img_path = args.get(1).expect("usage: spike <frame.png>");
    let m = "../models/";
    let t0 = Instant::now();
    let ocr = OAROCRBuilder::new(
        format!("{m}PP-OCRv6_det_small.onnx"),
        format!("{m}korean_PP-OCRv5_rec_mobile.onnx"),
        format!("{m}korean_PP-OCRv5_dict.txt"),
    )
    .build()?;
    println!("built in {:?}", t0.elapsed());
    let img = image::open(img_path)?.to_rgb8();
    for round in 0..2 {
        let t = Instant::now();
        let res = ocr.predict(vec![img.clone()])?;
        println!("round {round}: {:?}, {} regions", t.elapsed(), res[0].text_regions.len());
        if round == 1 {
            for r in &res[0].text_regions {
                if let Some(text) = &r.text {
                    let b = &r.bounding_box;
                    println!("[{:.0},{:.0},{:.0},{:.0}] {} ({:.2})", b.x_min(), b.y_min(), b.x_max(), b.y_max(), text, r.confidence.unwrap_or(0.0));
                }
            }
        }
    }
    Ok(())
}
