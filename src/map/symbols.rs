//! The Vaults of Forbidden Knowledge's eight symbols: SVGs in `assets/symbols/`,
//! drawn for the mod after the game's dials (white strokes; any colour on use) — for
//! the panel's vault notebook and puzzle answers (ui/svg.rs shows an SVG).
//!
//! The game numbers them 1–8 (`ECacheSymbols::CacheSymbol1…8`): Plutchik's eight
//! emotions in alphabetical order — matched against every vault's code and the
//! guides, and seen right at a vault door (.spec/GUIDE.md §29).

/// The symbols, by number − 1: their names and SVGs.
pub const NAMES: [&str; 8] = ["admiration", "amazement", "ecstasy", "grief", "loathing", "rage", "terror", "vigilance"];

const SOURCES: [&str; 8] = [
    include_str!("../../assets/symbols/admiration.svg"),
    include_str!("../../assets/symbols/amazement.svg"),
    include_str!("../../assets/symbols/ecstasy.svg"),
    include_str!("../../assets/symbols/grief.svg"),
    include_str!("../../assets/symbols/loathing.svg"),
    include_str!("../../assets/symbols/rage.svg"),
    include_str!("../../assets/symbols/terror.svg"),
    include_str!("../../assets/symbols/vigilance.svg"),
];

/// A symbol's name in the mod's words.
pub fn label(n: u8) -> &'static str {
    match n {
        1 => tr!("ADMIRATION"),
        2 => tr!("AMAZEMENT"),
        3 => tr!("ECSTASY"),
        4 => tr!("GRIEF"),
        5 => tr!("LOATHING"),
        6 => tr!("RAGE"),
        7 => tr!("TERROR"),
        8 => tr!("VIGILANCE"),
        _ => "?",
    }
}

/// Symbol `n` (1–8) as an SVG document in `colour` (`#rrggbb`), or `None`.
pub fn svg(n: u8, colour: &str) -> Option<String> {
    let source = SOURCES.get(n.checked_sub(1)? as usize)?;
    Some(source.replace("#ffffff", colour))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_symbol_draws_in_any_colour() {
        for n in 1..=8 {
            let s = svg(n, "#ff0000").unwrap();
            let icon = crate::icons::render(&s, 48).unwrap();
            let drawn: Vec<&u32> = icon.px.iter().filter(|p| *p >> 24 > 128).collect();
            assert!(drawn.len() > 100, "symbol {n} ({}) draws little", NAMES[n as usize - 1]);
            assert!(drawn.iter().all(|p| (*p >> 8) & 0xFF < 40), "symbol {n}: only the colour asked for");
        }
        assert!(svg(0, "#fff").is_none() && svg(9, "#fff").is_none());
    }
}
