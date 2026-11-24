//! Prints the fingerprint distances between two saved frames: the name strip (detail) and the whole
//! frame, which is what decides whether a view is still moving.
use baocr_pixels::{detail_distance, detail_key, fast_frame_key, frame_distance};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let load = |p: &String| image::open(p).expect("image").to_rgb8();
    let (a, b) = (load(&args[0]), load(&args[1]));
    println!("detail {:.3}  frame {:.3}", detail_distance(&detail_key(&a), &detail_key(&b)), frame_distance(&fast_frame_key(&a), &fast_frame_key(&b)));
}
