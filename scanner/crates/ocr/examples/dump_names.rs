//! Prints the add-on's student names (one per line, sorted, unique): the vocabulary `scripts/trocr_seed.py` tokenises.
fn main() {
    let mut v = baocr_names::all_names();
    v.sort();
    v.dedup();
    for n in v {
        println!("{n}");
    }
}
