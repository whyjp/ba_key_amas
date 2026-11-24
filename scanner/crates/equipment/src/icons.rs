//! Embedded icon templates (data/icons/<Type>-T<tier>.png).
macro_rules! tiers {
    ($kind:literal, $tier:expr, [$($t:literal),*]) => {
        match $tier {
            $( $t => return Some(&include_bytes!(concat!("../data/icons/", $kind, "-T", $t, ".png"))[..]), )*
            _ => {}
        }
    };
}
macro_rules! kinds {
    ($kind:expr, $tier:expr, [$($k:literal),*]) => {
        match $kind {
            $( $k => tiers!($k, $tier, [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]), )*
            _ => {}
        }
    };
}

/// PNG bytes of the template for (type, tier), if embedded.
pub fn bytes(kind: &str, tier: i32) -> Option<&'static [u8]> {
    kinds!(
        kind,
        tier,
        ["Hat", "Gloves", "Shoes", "Bag", "Badge", "Hairpin", "Charm", "Watch", "Necklace"]
    );
    None
}
