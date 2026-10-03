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

/// One frame of the compass strip, for a camera facing `yaw` — measured from the
/// game's north, clockwise (the caller subtracts the world yaw of north).
pub fn draw_compass(cv: &mut Canvas, yaw: f32, pins: &[Pin]) {
    cv.clear();
    let w = cv.w as f32;
    let (cx, bar_top, bar_bottom) = (w / 2.0, 4.0, 34.0);
    let ppd = (w / 2.0 - 22.0) / COMPASS_SPAN;
    cv.polygon(&[(8.0, bar_top), (w - 8.0, bar_top), (w - 8.0, bar_bottom), (8.0, bar_bottom)], Rgba(14, 16, 20, 150));
    cv.line((8.0, bar_bottom), (w - 8.0, bar_bottom), 1.0, Rgba(200, 200, 190, 120));
    let fade = |x: f32| (1.0 - ((x - cx).abs() / (w / 2.0 - 10.0)).powi(3)).clamp(0.0, 1.0);
    // Ticks every 15°, labels on the eight winds.
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
        let len = if cardinal {
            9.0
        } else if ordinal {
            6.0
        } else {
            3.5
        };
        cv.line((x, bar_bottom - len), (x, bar_bottom), 1.4, Rgba(220, 220, 210, (200.0 * a) as u8));
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
        if !label.is_empty() {
            let colour =
                if step == 0 { Rgba(255, 110, 90, (255.0 * a) as u8) } else { Rgba(235, 235, 225, (230.0 * a) as u8) };
            cv.text(x, bar_top + 10.0, if cardinal { 11.0 } else { 8.0 }, label, colour);
        }
    }
    // Straight ahead.
    cv.triangle(
        [(cx, bar_bottom - 4.0), (cx - 5.0, bar_bottom + 4.0), (cx + 5.0, bar_bottom + 4.0)],
        Rgba(255, 255, 255, 230),
    );
    // The pins: others first, the target last and on top.
    let mut order: Vec<&Pin> = pins.iter().collect();
    order.sort_by_key(|p| p.target);
    for p in order {
        let d = wrap(p.bearing - yaw);
        let [r, g, b] = p.rgb;
        let colour = Rgba(r, g, b, 255);
        let y = bar_bottom - 13.0;
        if d.abs() <= COMPASS_SPAN {
            let x = cx + d * ppd;
            let (k, a) = pin_look(p.distance_m, p.target);
            let s = if p.target { 6.5 } else { 4.0 } * k;
            let colour = Rgba(r, g, b, (255.0 * a) as u8);
            cv.polygon(
                &[(x, y - s - 1.5), (x + s + 1.5, y), (x, y + s + 1.5), (x - s - 1.5, y)],
                Rgba(0, 0, 0, (200.0 * a) as u8),
            );
            cv.polygon(&[(x, y - s), (x + s, y), (x, y + s), (x - s, y)], colour);
            // Another floor: a small arrow beside it, up or down.
            if p.dz_m.abs() >= FLOOR_DZ {
                let (ax, up) = (x + s + 5.0, p.dz_m > 0.0);
                let (tip, base) = if up { (y - 5.0, y + 2.0) } else { (y + 5.0, y - 2.0) };
                cv.triangle([(ax, tip), (ax - 3.5, base), (ax + 3.5, base)], Rgba(255, 255, 255, (230.0 * a) as u8));
            }
            if p.target {
                target_label(cv, x, bar_bottom + 12.0, p);
            }
        } else if p.target {
            // Behind or beside: an arrow at the end it is nearer to, with the distance.
            let side = d.signum();
            let x = cx + side * (w / 2.0 - 14.0);
            cv.triangle([(x + side * 7.0, y), (x - side * 3.0, y - 7.0), (x - side * 3.0, y + 7.0)], colour);
            target_label(cv, x - side * 4.0, bar_bottom + 12.0, p);
        }
    }
}

/// Under the target: its distance, and on another floor an arrow up or down with the
/// height difference — `85m ▼12m` — centred on `x`, kept on the strip.
fn target_label(cv: &mut Canvas, x: f32, y: f32, p: &Pin) {
    const SIZE: f32 = 10.0;
    let width = |s: &str| 5.5 * SIZE / 6.0 * s.chars().count() as f32 - 1.5 * SIZE / 6.0;
    let dist = distance(p.distance_m);
    let height = (p.dz_m.abs() >= FLOOR_DZ).then(|| format!("{:.0}m", p.dz_m.abs()));
    let (arrow, gap) = (8.0, 6.0);
    let total = width(&dist) + height.as_ref().map_or(0.0, |h| gap + arrow + 2.0 + width(h));
    let x0 = (x - total / 2.0).clamp(4.0, cv.w as f32 - 4.0 - total);
    cv.text(x0 + width(&dist) / 2.0, y, SIZE, &dist, Rgba(255, 255, 255, 240));
    if let Some(h) = height {
        let up = p.dz_m > 0.0;
        // Up in sky blue, down in amber.
        let c = if up { Rgba(120, 200, 255, 245) } else { Rgba(255, 180, 80, 245) };
        let ax = x0 + width(&dist) + gap + arrow / 2.0;
        let (tip, base) = if up { (y - 5.0, y + 4.0) } else { (y + 5.0, y - 4.0) };
        cv.triangle([(ax, tip), (ax - arrow / 2.0, base), (ax + arrow / 2.0, base)], c);
        cv.text(ax + arrow / 2.0 + 2.0 + width(&h) / 2.0, y, SIZE, &h, c);
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
