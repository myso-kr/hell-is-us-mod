//! The compass strip: the eight winds, the goals and pins as marks that shrink and fade
//! with distance, the target's distance and height difference — and the floor cues
//! (faded marks, up/down arrows) the map uses too.

use super::canvas::{Canvas, Rgba};

/// A place on the compass: which way (world yaw, degrees), how it looks, how far.
#[derive(Clone, Copy, Debug)]
pub struct Pin {
    pub bearing: f32,
    pub rgb: [u8; 3],
    /// The guide's target: drawn larger, with its distance.
    pub target: bool,
    pub distance_m: f32,
    /// How far above (+) or below (−) the hero it is (m).
    pub dz_m: f32,
}

/// A pin's size and opacity by distance: full and a little large near, smaller and
/// fainter far — on a log scale, 10 m to 200 m. The target never fades below 70%.
pub fn pin_look(distance_m: f32, target: bool) -> (f32, f32) {
    let t = ((distance_m.max(1.0).ln() - 10f32.ln()) / (200f32.ln() - 10f32.ln())).clamp(0.0, 1.0);
    let scale = 1.15 - 0.45 * t;
    let alpha = if target { 1.0 - 0.3 * t } else { 1.0 - 0.65 * t };
    (scale, alpha)
}

/// A height difference worth showing (m): another floor, not a slope.
pub const FLOOR_DZ: f32 = 3.0;

/// Bearings closer than this to straight ahead are on the strip; the rest pin to its ends.
pub const COMPASS_SPAN: f32 = 90.0;

fn wrap(a: f32) -> f32 {
    (a + 540.0).rem_euclid(360.0) - 180.0
}

/// The panel's palette (ui/theme.rs) as the raster's colours: the strip's wash, its
/// letters and ticks, and Lymbic blue for north and the guided target.
const WASH: Rgba = Rgba(0x0E, 0x12, 0x17, 168);
const TITLE: Rgba = Rgba(0xE6, 0xEE, 0xF7, 255);
const DIM: Rgba = Rgba(0x9A, 0xA6, 0xB3, 255);
const ACCENT: Rgba = Rgba(0x5A, 0x9C, 0xE6, 255);
/// Up a floor in sky blue, down in amber, wherever a height is shown.
const UP: Rgba = Rgba(120, 200, 255, 245);
const DOWN: Rgba = Rgba(255, 180, 80, 245);

/// `c` with its opacity scaled by `a` (0..=1).
fn faded(c: Rgba, a: f32) -> Rgba {
    Rgba(c.0, c.1, c.2, (c.3 as f32 * a.clamp(0.0, 1.0)) as u8)
}

/// A box (x0, y0, x1, y1).
type Bounds = (f32, f32, f32, f32);

/// The signed distance from (px, py) to the edge of the box `b` with corners of
/// radius `r`: negative inside.
fn box_distance(px: f32, py: f32, b: Bounds, r: f32) -> f32 {
    let (x0, y0, x1, y1) = b;
    let r = r.min((x1 - x0) / 2.0).min((y1 - y0) / 2.0);
    let qx = (px - (x0 + x1) / 2.0).abs() - ((x1 - x0) / 2.0 - r);
    let qy = (py - (y0 + y1) / 2.0).abs() - ((y1 - y0) / 2.0 - r);
    qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - r
}

/// Each pixel of the box `b` on the canvas, handed its centre's distance to the edge.
fn each_in(cv: &mut Canvas, b: Bounds, r: f32, mut f: impl FnMut(&mut Canvas, i32, i32, f32, f32)) {
    let (lx, hx) = ((b.0.floor() as i32).max(0), (b.2.ceil() as i32).min(cv.w as i32));
    let (ly, hy) = ((b.1.floor() as i32).max(0), (b.3.ceil() as i32).min(cv.h as i32));
    for y in ly..hy {
        for x in lx..hx {
            let px = x as f32 + 0.5;
            f(cv, x, y, px, box_distance(px, y as f32 + 0.5, b, r));
        }
    }
}

/// A filled box with round corners, `fade` scaling each column's opacity by its x.
fn round_box(cv: &mut Canvas, b: Bounds, r: f32, c: Rgba, fade: impl Fn(f32) -> f32) {
    each_in(cv, b, r, |cv, x, y, px, d| cv.blend(x, y, c, (0.5 - d).clamp(0.0, 1.0) * fade(px)));
}

/// A one-pixel line just inside a round box's edge.
fn round_edge(cv: &mut Canvas, b: Bounds, r: f32, c: Rgba) {
    each_in(cv, b, r, |cv, x, y, _, d| cv.blend(x, y, c, 1.0 - (d + 0.5).abs()));
}

/// One frame of the compass strip, for a camera facing `yaw` — measured from the
/// game's north, clockwise (the caller subtracts the world yaw of north).
///
/// A calm dark band that dissolves at its ends; thin ticks along its foot; the four
/// winds in the title colour (north in the accent), the four between them small and
/// dim; the pins as small dots; the guided target as a diamond ringed in the accent,
/// its distance in a pill under the band.
pub fn draw_compass(cv: &mut Canvas, yaw: f32, pins: &[Pin]) {
    cv.clear();
    let w = cv.w as f32;
    let (cx, bar_top, bar_bottom) = (w / 2.0, 4.0, 34.0);
    let ppd = (w / 2.0 - 22.0) / COMPASS_SPAN;
    // The band's ends fade out over 56 px instead of stopping at an edge.
    let feather = |x: f32| {
        let t = ((x - 8.0).min(w - 8.0 - x) / 56.0).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    };
    round_box(cv, (8.0, bar_top, w - 8.0, bar_bottom), 10.0, WASH, feather);
    let fade = |x: f32| (1.0 - ((x - cx).abs() / (w / 2.0 - 10.0)).powi(3)).clamp(0.0, 1.0);
    // Ticks every 15°, letters on the eight winds.
    for step in 0..24 {
        let bearing = step as f32 * 15.0;
        let d = wrap(bearing - yaw);
        if d.abs() > COMPASS_SPAN + 1.0 {
            continue;
        }
        let x = cx + d * ppd;
        let a = fade(x);
        let cardinal = step % 6 == 0;
        let ordinal = step % 3 == 0 && !cardinal;
        let (len, strength) = if cardinal {
            (5.0, 0.8)
        } else if ordinal {
            (4.0, 0.6)
        } else {
            (3.0, 0.45)
        };
        cv.line((x, bar_bottom - 3.0 - len), (x, bar_bottom - 3.0), 1.0, faded(DIM, strength * a));
        let label = match step {
            0 => "N",
            3 => "NE",
            6 => "E",
            9 => "SE",
            12 => "S",
            15 => "SW",
            18 => "W",
            21 => "NW",
            _ => "",
        };
        if cardinal {
            let colour = if step == 0 { ACCENT } else { TITLE };
            cv.text(x, bar_top + 11.0, 10.0, label, faded(colour, a));
        } else if ordinal {
            cv.text(x, bar_top + 11.0, 7.0, label, faded(DIM, 0.9 * a));
        }
    }
    // Straight ahead: a small caret on the band's foot.
    cv.triangle(
        [(cx, bar_bottom - 4.0), (cx - 4.0, bar_bottom + 1.0), (cx + 4.0, bar_bottom + 1.0)],
        faded(TITLE, 0.9),
    );
    // The pins: others first, the target last and on top.
    let mut order: Vec<&Pin> = pins.iter().collect();
    order.sort_by_key(|p| p.target);
    for p in order {
        let d = wrap(p.bearing - yaw);
        let [r, g, b] = p.rgb;
        let y = bar_bottom - 13.0;
        if d.abs() <= COMPASS_SPAN {
            let x = cx + d * ppd;
            let (k, a) = pin_look(p.distance_m, p.target);
            let colour = Rgba(r, g, b, (255.0 * a) as u8);
            if p.target {
                // A diamond in the goal's colour, ringed in the accent, on a dark halo.
                let s = 6.5 * k;
                let diamond = |s: f32| [(x, y - s), (x + s, y), (x, y + s), (x - s, y)];
                cv.polygon(&diamond(s + 3.2), faded(WASH, 1.2 * a));
                cv.polygon(&diamond(s + 1.8), faded(ACCENT, a));
                cv.polygon(&diamond(s), colour);
                if p.dz_m.abs() >= FLOOR_DZ {
                    floor_tick(cv, x + s + 7.0, y, p.dz_m, a);
                }
                // A thread from the band down to the pill.
                cv.line((x, bar_bottom + 1.0), (x, bar_bottom + 5.0), 1.0, faded(ACCENT, 0.8 * a));
                target_label(cv, x, bar_bottom + 12.0, p);
            } else {
                // A dot, quieter with distance, on a faint dark halo for bright scenes.
                let s = 3.0 * k;
                cv.disc(x, y, s + 1.5, faded(WASH, 0.8 * a));
                cv.disc(x, y, s, faded(colour, 0.9));
                if p.dz_m.abs() >= FLOOR_DZ {
                    floor_tick(cv, x + s + 4.0, y, p.dz_m, 0.8 * a);
                }
            }
        } else if p.target {
            // Behind or beside: an accent chevron at the end it is nearer to, with the distance.
            let side = d.signum();
            let x = cx + side * (w / 2.0 - 14.0);
            let chevron = |s: f32| [(x + side * s, y), (x - side * s * 0.4, y - s), (x - side * s * 0.4, y + s)];
            cv.triangle(chevron(8.5), faded(WASH, 1.2));
            cv.triangle(chevron(6.5), ACCENT);
            target_label(cv, x - side * 4.0, bar_bottom + 12.0, p);
        }
    }
}

/// A small floor arrow beside a pin on the strip: up in sky blue, down in amber.
fn floor_tick(cv: &mut Canvas, x: f32, y: f32, dz_m: f32, a: f32) {
    let up = dz_m > 0.0;
    let (tip, base) = if up { (y - 4.0, y + 2.0) } else { (y + 4.0, y - 2.0) };
    cv.triangle([(x, tip), (x - 3.0, base), (x + 3.0, base)], faded(if up { UP } else { DOWN }, a));
}

/// Under the target: a small dark pill with its distance, and on another floor an
/// arrow up or down with the height difference (`85m ▼12m`), centred on `x`, kept on
/// the strip.
fn target_label(cv: &mut Canvas, x: f32, y: f32, p: &Pin) {
    const SIZE: f32 = 9.0;
    let width = |s: &str| 5.5 * SIZE / 6.0 * s.chars().count() as f32 - 1.5 * SIZE / 6.0;
    let dist = distance(p.distance_m);
    let height = (p.dz_m.abs() >= FLOOR_DZ).then(|| format!("{:.0}m", p.dz_m.abs()));
    let (arrow, gap, pad) = (7.0, 6.0, 8.0);
    let total = width(&dist) + height.as_ref().map_or(0.0, |h| gap + arrow + 3.0 + width(h));
    let x0 = (x - total / 2.0).clamp(4.0 + pad, cv.w as f32 - 4.0 - pad - total);
    let pill = (x0 - pad, y - 8.0, x0 + total + pad, y + 8.0);
    round_box(cv, pill, 8.0, Rgba(0x0E, 0x12, 0x17, 220), |_| 1.0);
    round_edge(cv, pill, 8.0, faded(ACCENT, 0.55));
    cv.text(x0 + width(&dist) / 2.0, y, SIZE, &dist, TITLE);
    if let Some(h) = height {
        let up = p.dz_m > 0.0;
        let c = if up { UP } else { DOWN };
        let ax = x0 + width(&dist) + gap + arrow / 2.0;
        let (tip, base) = if up { (y - 4.0, y + 3.5) } else { (y + 4.0, y - 3.5) };
        cv.triangle([(ax, tip), (ax - arrow / 2.0, base), (ax + arrow / 2.0, base)], c);
        cv.text(ax + arrow / 2.0 + 3.0 + width(&h) / 2.0, y, SIZE, &h, c);
    }
}

/// How opaque something on another floor is drawn (0–255): its own floor in full,
/// fading over 3–6 m of height difference to about a third.
pub fn floor_alpha(dz_m: f32) -> u32 {
    let t = ((dz_m.abs() - FLOOR_DZ) / FLOOR_DZ).clamp(0.0, 1.0);
    (255.0 - 170.0 * t) as u32
}

/// A small arrow at (x, y) when `dz_m` is another floor: up in sky blue, down in amber.
pub(crate) fn floor_arrow(cv: &mut Canvas, x: f32, y: f32, dz_m: f32) {
    if dz_m.abs() < FLOOR_DZ {
        return;
    }
    let up = dz_m > 0.0;
    let c = if up { Rgba(120, 200, 255, 240) } else { Rgba(255, 180, 80, 240) };
    let (tip, base) = if up { (y - 4.0, y + 3.0) } else { (y + 4.0, y - 3.0) };
    cv.triangle([(x, tip + 1.0), (x - 4.5, base + 1.0), (x + 4.5, base + 1.0)], Rgba(0, 0, 0, 170));
    cv.triangle([(x, tip), (x - 3.5, base), (x + 3.5, base)], c);
}

/// `85m`, `1.2km`.
pub fn distance(m: f32) -> String {
    if m < 1000.0 {
        format!("{:.0}m", m.max(0.0))
    } else {
        format!("{:.1}km", m / 1000.0)
    }
}

/// How far a place is from the hero: across, and up or down when that is three metres
/// or more — a lock under a monument is "3m" away across but nine below. Positions in
/// centimetres, as the game keeps them.
pub fn span(from: [f32; 3], to: [f32; 3]) -> String {
    let across = distance((to[0] - from[0]).hypot(to[1] - from[1]) / 100.0);
    match (to[2] - from[2]) / 100.0 {
        up if up >= 3.0 => format!("{across} ↑{up:.0}m"),
        up if up <= -3.0 => format!("{across} ↓{:.0}m", -up),
        _ => across,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_span_says_up_or_down_only_when_it_counts() {
        assert_eq!(span([0.0, 0.0, 0.0], [300.0, 400.0, 100.0]), "5m");
        assert_eq!(span([0.0, 0.0, 0.0], [300.0, 0.0, -900.0]), "3m ↓9m");
        assert_eq!(span([0.0, 0.0, 0.0], [0.0, 200_000.0, 1_200.0]), "2.0km ↑12m");
    }

    #[test]
    fn bearings_wrap_to_the_short_way_round() {
        assert_eq!(wrap(350.0 - 10.0), -20.0);
        assert_eq!(wrap(10.0 - 350.0), 20.0);
    }
}
