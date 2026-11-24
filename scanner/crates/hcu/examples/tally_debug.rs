//! Prints the lines found on saved count pages: `tally_debug <dir> <offset> <offset> ...`
use baocr_pixels::{roster_cards, slot_occupied, unowned_banner};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dir = &args[0];
    for (i, off) in args[1..].iter().enumerate() {
        let off: i32 = off.parse().unwrap();
        let img = image::open(format!("{dir}/page_{i:02}.png")).unwrap().to_rgb8();
        let banner = unowned_banner(&img);
        let rows: Vec<String> = roster_cards(&img)
            .chunks(6)
            .map(|r| format!("y{}→{} n{}", r[0].y, r[0].y + off, r.iter().filter(|c| slot_occupied(&img, c.x, c.y)).count()))
            .collect();
        println!("page {i:2} off {off:5} banner {banner:?}: {}", rows.join(" | "));
    }
}
