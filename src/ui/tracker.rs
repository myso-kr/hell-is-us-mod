//! The quest tracker at the right of the screen, as MMORPGs have it: the quests
//! under way, the followed one first and opened up — its description, the leads
//! the hero is chasing, how many of its clues are found — the others one line each.
//!
//! It looks like the panel's cards (theme.rs): a dark translucent card with a hairline
//! edge, each quest marked by a dot in its kind's colour, the followed one as a header
//! in the title colour with a thin meter for its clues, notes as tinted callouts.

use super::pen::Pen;
use crate::quests::{Kind, Quest};
use crate::raster::{Canvas, Rgba};

pub const W: i32 = 340;
pub const H: i32 = 560;
/// Inside the card, round its content.
const PAD: i32 = 12;
/// Where the text starts: past the column the kind dots sit in.
const TEXT_X: i32 = PAD + 14;
/// Quests shown besides the followed one.
const OTHERS: usize = 5;

/// The panel's palette (theme.rs) as the raster's colours.
const CARD: Rgba = Rgba(0x13, 0x18, 0x1F, 210);
const EDGE: Rgba = Rgba(0x2A, 0x32, 0x3D, 255);
const TITLE: Rgba = Rgba(0xE6, 0xEE, 0xF7, 255);
const TEXT: Rgba = Rgba(0xD9, 0xE1, 0xEA, 255);
const DIM: Rgba = Rgba(0x9A, 0xA6, 0xB3, 255);
const ACCENT: Rgba = Rgba(0x5A, 0x9C, 0xE6, 255);
const WAIT: Rgba = Rgba(0xE8, 0xC0, 0x6A, 255);

/// Each kind's colour, close to its marks on the map and the compass.
const MAIN: Rgba = Rgba(236, 112, 196, 255);
const DEED: Rgba = Rgba(122, 204, 150, 255);
const MYSTERY: Rgba = Rgba(112, 204, 214, 255);
const TIMELOOP: Rgba = Rgba(156, 132, 232, 255);

fn kind_colour(q: &Quest) -> Rgba {
    match q.kind {
        Kind::Main(_) => MAIN,
        Kind::GoodDeed => DEED,
        Kind::Mystery => MYSTERY,
        Kind::Timeloop => TIMELOOP,
    }
}

/// Draw the tracker into `cv` (cleared first); the height used, 0 when there is
/// nothing to show.
#[allow(clippy::too_many_arguments)]
pub fn draw(
    cv: &mut Canvas,
    pen: &mut Pen,
    journal: &[Quest],
    followed: Option<&Quest>,
    near: bool,
    stuck: bool,
    needs: &str,
    besides: &[(String, [u8; 3])],
) -> i32 {
    cv.clear();
    let mut list: Vec<&Quest> = Vec::new();
    if let Some(f) = followed {
        list.push(f);
    }
    // The quests followed besides (guide/track.rs) next, then the others.
    let colour_of = |q: &Quest| besides.iter().find(|(k, _)| *k == q.key).map(|(_, c)| *c);
    let rest = |q: &&Quest| q.active() && Some(q.key.as_str()) != followed.map(|f| f.key.as_str());
    list.extend(journal.iter().filter(rest).filter(|q| colour_of(q).is_some()));
    list.extend(journal.iter().filter(rest).filter(|q| colour_of(q).is_none()).take(OTHERS));
    if list.is_empty() {
        return 0;
    }
    let width = W - TEXT_X - PAD;
    let mut y = PAD;
    let count = journal.iter().filter(|q| q.active()).count().max(list.len());
    y += pen.write(cv, PAD, y, W - 2 * PAD, &format!("{}  {count}", tr!("QUESTS")), 11, true, DIM, 1) + 8;
    for (i, q) in list.iter().enumerate() {
        let open = i == 0 && followed.is_some();
        let c = kind_colour(q);
        if !open {
            // One line: the dot, the name; followed besides, ringed in its colour and bold.
            let besides = colour_of(q);
            let h = pen.write(cv, TEXT_X, y, width, &q.name, 12, besides.is_some(), TEXT, 1);
            let (dx, dy) = (PAD as f32 + 4.0, y as f32 + h as f32 / 2.0 + 0.5);
            if let Some([r, g, b]) = besides {
                cv.disc(dx, dy, 5.0, Rgba(r, g, b, 255));
            }
            cv.disc(dx, dy, 3.0, c);
            y += h + 4;
        } else {
            // The followed quest as a header: the dot, the name in the title colour.
            let h = pen.write(cv, TEXT_X, y, width, &q.name, 15, true, TITLE, 2);
            cv.disc(PAD as f32 + 4.0, y as f32 + 11.0, 4.0, c);
            y += h + 2;
            let mut sub = q.kind.label();
            let progress = q.progress.filter(|(_, all)| *all > 0);
            if let Some((got, all)) = progress {
                sub += &trf!("CLUES", got = got, all = all);
            }
            sub += tr!("GUIDING");
            y += pen.write(cv, TEXT_X, y, width, &sub, 11, false, DIM, 1) + 4;
            if let Some((got, all)) = progress {
                meter(cv, TEXT_X, y, width, got as f32 / all as f32, c);
                y += 3 + 8;
            } else {
                y += 4;
            }
            if !near {
                let why = match q.kind {
                    Kind::Main(_) => tr!("THIS_QUEST_HAS_NO_GOAL_IN"),
                    _ => tr!("THIS_ENTRY_HAS_NO_GOAL_IN"),
                };
                y += callout(cv, pen, y, width, why, WAIT) + 4;
            }
            if !needs.is_empty() {
                y += callout(cv, pen, y, width, needs, ACCENT) + 4;
            }
            if near && stuck {
                y += callout(cv, pen, y, width, tr!("PAST_A_CLOSED_DOOR_OR_PUZZLE"), WAIT) + 4;
            }
            if !q.detail.is_empty() {
                y += pen.write(cv, TEXT_X, y, width, &q.detail, 12, false, TEXT, 3) + 4;
            }
            for (lead, text) in q.leads.iter().take(3) {
                let line = match text {
                    Some(t) => format!("{lead}: {t}"),
                    None => lead.clone(),
                };
                let h = pen.write(cv, TEXT_X + 10, y, width - 10, &line, 11, false, DIM, 2);
                // A small dash in front, as a list.
                cv.rect(TEXT_X + 1, y + 8, TEXT_X + 5, y + 9, DIM);
                y += h + 2;
            }
            if list.len() > 1 {
                // A hairline between the followed quest and the rest.
                y += 6;
                cv.rect(PAD, y, W - PAD, y + 1, EDGE);
                y += 1 + 8;
            }
        }
        if y > H - 40 {
            break;
        }
    }
    let used = (y + PAD - 4).min(H);
    let card = (0, 0, W, used);
    under(cv, card, 8.0, CARD);
    edge(cv, card, 8.0, EDGE);
    used
}

/// A thin meter: `share` of `width` filled in `c` over a dim track, 3 px tall.
fn meter(cv: &mut Canvas, x: i32, y: i32, width: i32, share: f32, c: Rgba) {
    let (x, y, w) = (x as f32, y as f32, width as f32);
    fill(cv, (x, y, x + w, y + 3.0), 1.5, Rgba(0x3A, 0x44, 0x52, 255));
    let filled = (w * share.clamp(0.0, 1.0)).round();
    if filled > 0.0 {
        fill(cv, (x, y, x + filled.max(3.0), y + 3.0), 1.5, c);
    }
}

/// A note in a callout: the text on a faint wash of `c`, a bar of `c` at its left.
/// Returns the height it took.
fn callout(cv: &mut Canvas, pen: &mut Pen, y: i32, width: i32, text: &str, c: Rgba) -> i32 {
    let h = pen.write(cv, TEXT_X + 2, y + 4, width - 8, text, 12, false, TEXT, 3) + 8;
    let b = (TEXT_X - 6, y, TEXT_X + width, y + h);
    under(cv, b, 4.0, Rgba(c.0, c.1, c.2, 40));
    cv.rect(b.0, y + 2, b.0 + 2, y + h - 2, c);
    h
}

/// The signed distance from (px, py) to the edge of the box `b` with corners of
/// radius `r`: negative inside.
fn box_distance(px: f32, py: f32, b: (f32, f32, f32, f32), r: f32) -> f32 {
    let (x0, y0, x1, y1) = b;
    let r = r.min((x1 - x0) / 2.0).min((y1 - y0) / 2.0);
    let qx = (px - (x0 + x1) / 2.0).abs() - ((x1 - x0) / 2.0 - r);
    let qy = (py - (y0 + y1) / 2.0).abs() - ((y1 - y0) / 2.0 - r);
    qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - r
}

/// Each pixel of the box `b`, handed how far inside its edge the pixel's centre is.
fn each_in(cv: &mut Canvas, b: (f32, f32, f32, f32), r: f32, mut f: impl FnMut(&mut Canvas, usize, f32)) {
    let (lx, hx) = ((b.0.floor() as i32).max(0), (b.2.ceil() as i32).min(cv.w as i32));
    let (ly, hy) = ((b.1.floor() as i32).max(0), (b.3.ceil() as i32).min(cv.h as i32));
    for y in ly..hy {
        for x in lx..hx {
            let d = box_distance(x as f32 + 0.5, y as f32 + 0.5, b, r);
            f(cv, y as usize * cv.w + x as usize, -d);
        }
    }
}

fn float((x0, y0, x1, y1): (i32, i32, i32, i32)) -> (f32, f32, f32, f32) {
    (x0 as f32, y0 as f32, x1 as f32, y1 as f32)
}

/// A filled box with round corners, over what is drawn.
fn fill(cv: &mut Canvas, b: (f32, f32, f32, f32), r: f32, c: Rgba) {
    let w = cv.w;
    each_in(cv, b, r, |cv, i, inside| cv.blend((i % w) as i32, (i / w) as i32, c, (inside + 0.5).clamp(0.0, 1.0)));
}

/// A one-pixel line just inside the edge of the box `b` with round corners.
fn edge(cv: &mut Canvas, b: (i32, i32, i32, i32), r: f32, c: Rgba) {
    let w = cv.w;
    each_in(cv, float(b), r, |cv, i, inside| cv.blend((i % w) as i32, (i / w) as i32, c, 1.0 - (inside - 0.5).abs()));
}

/// Put `bg` under what is drawn, over the box `b` with round corners — so a box is as
/// tall as its content without drawing the content twice.
fn under(cv: &mut Canvas, b: (i32, i32, i32, i32), r: f32, bg: Rgba) {
    each_in(cv, float(b), r, |cv, i, inside| {
        let cover = (inside + 0.5).clamp(0.0, 1.0);
        let a = (bg.3 as f32 * cover + 0.5) as u32;
        if a == 0 {
            return;
        }
        let (r, g, b) = (bg.0 as u32 * a / 255, bg.1 as u32 * a / 255, bg.2 as u32 * a / 255);
        let p = &mut cv.px[i];
        let k = 255 - (*p >> 24);
        let ch = |shift: u32, v: u32| ((((*p >> shift) & 0xFF) + v * k / 255).min(255)) << shift;
        *p = ch(24, a) | ch(16, r) | ch(8, g) | ch(0, b);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_box_goes_under_the_text() {
        let mut cv = Canvas::new(2, 1);
        cv.px[0] = 0xFF_FF_FF_FF;
        under(&mut cv, (0, 0, 2, 1), 0.0, Rgba(0, 0, 0, 128));
        assert_eq!(cv.px[0], 0xFF_FF_FF_FF, "opaque text stays");
        assert_eq!(cv.px[1] >> 24, 128, "empty pixels get the box");
    }

    #[test]
    fn the_card_rounds_its_corners() {
        let mut cv = Canvas::new(20, 20);
        under(&mut cv, (0, 0, 20, 20), 8.0, Rgba(0, 0, 0, 200));
        assert_eq!(cv.px[0] >> 24, 0, "the corner stays clear");
        assert_eq!(cv.px[10 * 20 + 10] >> 24, 200, "the middle is the card");
    }
}
