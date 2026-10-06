//! The banner under the compass at the top centre: what needs saying once, for a while,
//! not all the time (.spec/JOURNEY.md §3.5, §3.8) — a good deed the next story beat ends,
//! and where the last session left off. The game keeps the top centre for its own
//! compass only briefly, when it is raised; the rest of its HUD is elsewhere.

use super::pen::Pen;
use crate::raster::{Canvas, Rgba};

pub const W: i32 = 620;
pub const H: i32 = 110;
const PAD: i32 = 12;

const CARD: Rgba = Rgba(0x13, 0x18, 0x1F, 220);
const TEXT: Rgba = Rgba(0xD9, 0xE1, 0xEA, 255);

/// Draw a banner: a title in `colour` over `body`, on a card with a stripe of `colour`
/// down its left edge. The height used.
pub fn draw(cv: &mut Canvas, pen: &mut Pen, title: &str, body: &str, colour: Rgba) -> i32 {
    cv.clear();
    pen.clear();
    let x = PAD + 10;
    let width = W - x - PAD;
    let mut y = PAD;
    y += pen.write(cv, x, y, width, title, 13, true, colour, 1) + 3;
    y += pen.write(cv, x, y, width, body, 12, false, TEXT, 3);
    let used = (y + PAD).min(H);
    let card = (0, 0, W, used);
    super::tracker::under(cv, card, 8.0, super::tracker::back(CARD));
    super::tracker::edge(cv, card, 8.0, Rgba(colour.0, colour.1, colour.2, 160));
    cv.rect(PAD - 2, PAD, PAD + 1, used - PAD, colour);
    used
}
