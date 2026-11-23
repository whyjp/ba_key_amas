//! Prints the detail fingerprint distance between two saved detail frames.
use baocr_pixels::{detail_distance, detail_key};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let load = |p: &String| image::open(p).expect("image").to_rgb8();
    let (a, b) = (detail_key(&load(&args[0])), detail_key(&load(&args[1])));
    println!("distance {:.3}", detail_distance(&a, &b));
}
