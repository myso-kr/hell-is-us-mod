//! A few shapes drawn into a premultiplied-alpha pixel buffer — just what the
//! minimap needs, so its window can be a plain layered Win32 window that keeps
//! drawing while the panel (and eframe with it) is hidden.
//!
//! Pixels are `0xAARRGGBB`, premultiplied, top row first: what `UpdateLayeredWindow`
//! takes from a 32-bit top-down DIB. Edges are anti-aliased by coverage over one pixel.

use crate::actors::{Kind, Thing};
use crate::geometry::Footprint;
use crate::goals::Goal;
use crate::icons::Icons;
use crate::minimap::{MapState, ReliefMode, View};
use crate::pathfind::Path;
use crate::relief::Relief;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgba(pub u8, pub u8, pub u8, pub u8);

pub struct Canvas {
    pub w: usize,
    pub h: usize,
    pub px: Vec<u32>,
}

impl Canvas {
    pub fn new(w: usize, h: usize) -> Canvas {
        Canvas { w, h, px: vec![0; w * h] }
    }

    pub fn clear(&mut self) {
        self.px.fill(0);
    }

    /// A filled rectangle, pixel-aligned.
    pub fn rect(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, c: Rgba) {
        for y in y0.max(0)..y1.min(self.h as i32) {
            for x in x0.max(0)..x1.min(self.w as i32) {
                self.blend(x, y, c, 1.0);
            }
        }
    }

    /// Source-over, `c` at `coverage` (0..=1).
    pub fn blend(&mut self, x: i32, y: i32, c: Rgba, coverage: f32) {
        if x < 0 || y < 0 || x as usize >= self.w || y as usize >= self.h || coverage <= 0.0 {
            return;
        }
        let a = (c.3 as f32 * coverage.min(1.0) + 0.5) as u32;
        if a == 0 {
            return;
        }
        let i = y as usize * self.w + x as usize;
        self.px[i] = over(self.px[i], c, a);
    }

    /// Every pixel within `reach` of the box around (x0, y0)–(x1, y1), handed its
    /// centre.
    fn each(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, mut f: impl FnMut(&mut Self, i32, i32, f32, f32)) {
        let (lx, hx) = ((x0.min(x1).floor() as i32 - 1).max(0), (x0.max(x1).ceil() as i32 + 1).min(self.w as i32 - 1));
        let (ly, hy) = ((y0.min(y1).floor() as i32 - 1).max(0), (y0.max(y1).ceil() as i32 + 1).min(self.h as i32 - 1));
        for y in ly..=hy {
            for x in lx..=hx {
                f(self, x, y, x as f32 + 0.5, y as f32 + 0.5);
            }
        }
    }

    /// Each row from `y0` to `y1`, over only the columns `span` gives for the row's
    /// centre (any number of ranges), each pixel handed its centre — so a thin or round
    /// shape costs its own pixels, not its bounding box's.
    fn rows(
        &mut self,
        y0: f32,
        y1: f32,
        span: impl Fn(f32) -> [Option<(f32, f32)>; 2],
        mut f: impl FnMut(&mut Self, i32, i32, f32, f32),
    ) {
        let (ly, hy) = ((y0.floor() as i32 - 1).max(0), (y1.ceil() as i32 + 1).min(self.h as i32 - 1));
        for y in ly..=hy {
            let py = y as f32 + 0.5;
            for (a, b) in span(py).into_iter().flatten() {
                let (lx, hx) = ((a.floor() as i32 - 1).max(0), (b.ceil() as i32 + 1).min(self.w as i32 - 1));
                for x in lx..=hx {
                    f(self, x, y, x as f32 + 0.5, py);
                }
            }
        }
    }

    pub fn disc(&mut self, cx: f32, cy: f32, r: f32, c: Rgba) {
        let o = r + 1.0;
        let span = |py: f32| {
            let dy = py - cy;
            let w = (o * o - dy * dy).max(0.0).sqrt();
            [(w > 0.0).then_some((cx - w, cx + w)), None]
        };
        self.rows(cy - o, cy + o, span, |s, x, y, px, py| {
            let d = ((px - cx).powi(2) + (py - cy).powi(2)).sqrt();
            s.blend(x, y, c, r - d + 0.5);
        });
    }

    pub fn ring(&mut self, cx: f32, cy: f32, r: f32, width: f32, c: Rgba) {
        let (o, i) = (r + width / 2.0 + 1.0, (r - width / 2.0 - 1.0).max(0.0));
        let span = |py: f32| {
            let dy = py - cy;
            let wo = (o * o - dy * dy).max(0.0).sqrt();
            if wo <= 0.0 {
                return [None, None];
            }
            if dy.abs() < i {
                let wi = (i * i - dy * dy).sqrt();
                [Some((cx - wo, cx - wi)), Some((cx + wi, cx + wo))]
            } else {
                [Some((cx - wo, cx + wo)), None]
            }
        };
        self.rows(cy - o, cy + o, span, |s, x, y, px, py| {
            let d = ((px - cx).powi(2) + (py - cy).powi(2)).sqrt();
            s.blend(x, y, c, width / 2.0 - (d - r).abs() + 0.5);
        });
    }

    pub fn line(&mut self, a: (f32, f32), b: (f32, f32), width: f32, c: Rgba) {
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let len2 = (dx * dx + dy * dy).max(f32::EPSILON);
        let h = width / 2.0;
        let (bx0, bx1) = (a.0.min(b.0) - h - 1.0, a.0.max(b.0) + h + 1.0);
        // Along a row, the pixels near the line lie within this of where it crosses.
        let reach = (h + 1.5) * len2.sqrt() / dy.abs().max(f32::EPSILON);
        let span = |py: f32| {
            if dy.abs() < 1e-3 || reach > bx1 - bx0 {
                return [Some((bx0, bx1)), None];
            }
            let x = a.0 + (py - a.1) * dx / dy;
            let (lo, hi) = ((x - reach).max(bx0), (x + reach).min(bx1));
            [(lo <= hi).then_some((lo, hi)), None]
        };
        self.rows(a.1.min(b.1) - h, a.1.max(b.1) + h, span, |s, x, y, px, py| {
            let t = (((px - a.0) * dx + (py - a.1) * dy) / len2).clamp(0.0, 1.0);
            let d = ((px - a.0 - t * dx).powi(2) + (py - a.1 - t * dy).powi(2)).sqrt();
            s.blend(x, y, c, h - d + 0.5);
        });
    }

    pub fn triangle(&mut self, p: [(f32, f32); 3], c: Rgba) {
        let edge = |a: (f32, f32), b: (f32, f32), x: f32, y: f32| {
            let (ex, ey) = (b.0 - a.0, b.1 - a.1);
            ((x - a.0) * ey - (y - a.1) * ex) / (ex * ex + ey * ey).sqrt().max(f32::EPSILON)
        };
        // Wind consistently, so "inside" is the same sign for every edge.
        let area = edge(p[0], p[1], p[2].0, p[2].1);
        let p = if area < 0.0 { [p[0], p[2], p[1]] } else { p };
        let (x0, x1) = (p.iter().map(|q| q.0).fold(f32::MAX, f32::min), p.iter().map(|q| q.0).fold(f32::MIN, f32::max));
        let (y0, y1) = (p.iter().map(|q| q.1).fold(f32::MAX, f32::min), p.iter().map(|q| q.1).fold(f32::MIN, f32::max));
        self.each(x0, y0, x1, y1, |s, x, y, px, py| {
            let d = edge(p[0], p[1], px, py).min(edge(p[1], p[2], px, py)).min(edge(p[2], p[0], px, py));
            s.blend(x, y, c, d + 0.5);
        });
    }

    /// A premultiplied bitmap, centred on (cx, cy), drawn over what is there.
    pub fn blit(&mut self, cx: f32, cy: f32, size: usize, px: &[u32]) {
        self.blit_alpha(cx, cy, size, px, 255);
    }

    /// `blit`, at `alpha` (0–255) of the image's own opacity.
    pub fn blit_alpha(&mut self, cx: f32, cy: f32, size: usize, px: &[u32], alpha: u32) {
        let (x0, y0) = ((cx - size as f32 / 2.0).round() as i32, (cy - size as f32 / 2.0).round() as i32);
        for (i, &s) in px.iter().enumerate() {
            // Premultiplied: every channel scales with the opacity.
            let s = if alpha >= 255 {
                s
            } else {
                let k = |shift: u32| (((s >> shift) & 0xFF) * alpha / 255) << shift;
                k(24) | k(16) | k(8) | k(0)
            };
            let sa = s >> 24;
            if sa == 0 {
                continue;
            }
            let (x, y) = (x0 + (i % size) as i32, y0 + (i / size) as i32);
            if x < 0 || y < 0 || x as usize >= self.w || y as usize >= self.h {
                continue;
            }
            let at = y as usize * self.w + x as usize;
            let d = self.px[at];
            let keep = 255 - sa;
            let ch = |shift: u32| ((((d >> shift) & 0xFF) * keep / 255 + ((s >> shift) & 0xFF)).min(255)) << shift;
            self.px[at] = ch(24) | ch(16) | ch(8) | ch(0);
        }
    }

    /// A convex polygon, its points in either winding.
    pub fn polygon(&mut self, p: &[(f32, f32)], c: Rgba) {
        if p.len() < 3 {
            return;
        }
        let edge = |a: (f32, f32), b: (f32, f32), x: f32, y: f32| {
            let (ex, ey) = (b.0 - a.0, b.1 - a.1);
            ((x - a.0) * ey - (y - a.1) * ex) / (ex * ex + ey * ey).sqrt().max(f32::EPSILON)
        };
        // Signed area decides which side is inside.
        let area: f32 = (0..p.len()).map(|i| p[i].0 * p[(i + 1) % p.len()].1 - p[(i + 1) % p.len()].0 * p[i].1).sum();
        let sign = if area < 0.0 { 1.0 } else { -1.0 };
        let (x0, x1) = (p.iter().map(|q| q.0).fold(f32::MAX, f32::min), p.iter().map(|q| q.0).fold(f32::MIN, f32::max));
        let (y0, y1) = (p.iter().map(|q| q.1).fold(f32::MAX, f32::min), p.iter().map(|q| q.1).fold(f32::MIN, f32::max));
        self.each(x0, y0, x1, y1, |s, x, y, px, py| {
            let d = (0..p.len()).map(|i| sign * edge(p[i], p[(i + 1) % p.len()], px, py)).fold(f32::MAX, f32::min);
            s.blend(x, y, c, d + 0.5);
        });
    }

    /// Text in a small stroke font (N E S W, digits, `m`, `.`, `-`), `size` px tall,
    /// centred on (cx, cy). Characters it does not know are spaces.
    pub fn text(&mut self, cx: f32, cy: f32, size: f32, s: &str, c: Rgba) {
        let unit = size / 6.0;
        let advance = 5.5 * unit;
        let width = advance * s.chars().count() as f32 - 1.5 * unit;
        let (x0, y0) = (cx - width / 2.0, cy - size / 2.0);
        let w = (unit * 0.9).max(1.3);
        for (i, ch) in s.chars().enumerate() {
            let ox = x0 + i as f32 * advance;
            for &((ax, ay), (bx, by)) in glyph(ch) {
                self.line((ox + ax * unit, y0 + ay * unit), (ox + bx * unit, y0 + by * unit), w, c);
            }
        }
    }

    /// The letter N, `size` px tall, centred on (cx, cy) — the only text the map needs.
    pub fn letter_n(&mut self, cx: f32, cy: f32, size: f32, c: Rgba) {
        let (hw, hh) = (size * 0.32, size / 2.0);
        let (l, r, t, b) = (cx - hw, cx + hw, cy - hh, cy + hh);
        self.line((l, b), (l, t), 2.0, c);
        self.line((l, t), (r, b), 2.0, c);
        self.line((r, b), (r, t), 2.0, c);
    }
}

type Seg = ((f32, f32), (f32, f32));

/// Strokes on a 4 × 6 grid, y down.
fn glyph(c: char) -> &'static [Seg] {
    const BOX: [Seg; 4] = [((0., 0.), (4., 0.)), ((4., 0.), (4., 6.)), ((4., 6.), (0., 6.)), ((0., 6.), (0., 0.))];
    match c {
        'N' => &[((0., 6.), (0., 0.)), ((0., 0.), (4., 6.)), ((4., 6.), (4., 0.))],
        'E' => &[((4., 0.), (0., 0.)), ((0., 0.), (0., 6.)), ((0., 6.), (4., 6.)), ((0., 3.), (3., 3.))],
        'S' => &[
            ((4., 0.), (0., 0.)),
            ((0., 0.), (0., 3.)),
            ((0., 3.), (4., 3.)),
            ((4., 3.), (4., 6.)),
            ((4., 6.), (0., 6.)),
        ],
        'W' => &[((0., 0.), (1., 6.)), ((1., 6.), (2., 2.)), ((2., 2.), (3., 6.)), ((3., 6.), (4., 0.))],
        '0' => &BOX,
        '1' => &[((2., 0.), (2., 6.)), ((1., 1.), (2., 0.))],
        '2' => &[
            ((0., 0.), (4., 0.)),
            ((4., 0.), (4., 3.)),
            ((4., 3.), (0., 3.)),
            ((0., 3.), (0., 6.)),
            ((0., 6.), (4., 6.)),
        ],
        '3' => &[((0., 0.), (4., 0.)), ((4., 0.), (4., 6.)), ((4., 6.), (0., 6.)), ((0., 3.), (4., 3.))],
        '4' => &[((0., 0.), (0., 3.)), ((0., 3.), (4., 3.)), ((4., 0.), (4., 6.))],
        '5' => &[
            ((4., 0.), (0., 0.)),
            ((0., 0.), (0., 3.)),
            ((0., 3.), (4., 3.)),
            ((4., 3.), (4., 6.)),
            ((4., 6.), (0., 6.)),
        ],
        '6' => &[
            ((4., 0.), (0., 0.)),
            ((0., 0.), (0., 6.)),
            ((0., 6.), (4., 6.)),
            ((4., 6.), (4., 3.)),
            ((4., 3.), (0., 3.)),
        ],
        '7' => &[((0., 0.), (4., 0.)), ((4., 0.), (4., 6.))],
        '8' => &[
            ((0., 0.), (4., 0.)),
            ((4., 0.), (4., 6.)),
            ((4., 6.), (0., 6.)),
            ((0., 6.), (0., 0.)),
            ((0., 3.), (4., 3.)),
        ],
        '9' => &[
            ((4., 3.), (0., 3.)),
            ((0., 3.), (0., 0.)),
            ((0., 0.), (4., 0.)),
            ((4., 0.), (4., 6.)),
            ((4., 6.), (0., 6.)),
        ],
        'm' => &[((0., 6.), (0., 3.)), ((0., 3.), (4., 3.)), ((4., 3.), (4., 6.)), ((2., 3.), (2., 6.))],
        'k' => &[((0., 0.), (0., 6.)), ((0., 4.), (4., 2.)), ((1., 3.5), (4., 6.))],
        '.' => &[((1.5, 5.6), (2.5, 5.6))],
        '-' => &[((0.5, 3.), (3.5, 3.))],
        _ => &[],
    }
}

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
            cv.polygon(&[(x, y - s - 1.5), (x + s + 1.5, y), (x, y + s + 1.5), (x - s - 1.5, y)], Rgba(0, 0, 0, (200.0 * a) as u8));
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
fn floor_arrow(cv: &mut Canvas, x: f32, y: f32, dz_m: f32) {
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

const BACKGROUND: Rgba = Rgba(16, 18, 22, 170);
const EDGE: Rgba = Rgba(200, 200, 190, 200);
const TRAIL: Rgba = Rgba(150, 200, 255, 210);
const MARKER: Rgba = Rgba(255, 200, 60, 240);
const HERO: Rgba = Rgba(255, 255, 255, 255);
const NORTH: Rgba = Rgba(255, 110, 90, 255);
const OUTLINE: Rgba = Rgba(0, 0, 0, 200);
fn colour(k: Kind) -> Rgba {
    let [r, g, b] = k.rgb();
    Rgba(r, g, b, 255)
}

/// Lower than this (cm) is a floor; higher stands up.
const FLAT: f32 = 80.0;
/// Something standing up and wider than this (cm) is a cliff or a big rock, whose box
/// is far larger than its shape: drawn as faint high ground, not as a wall.
const MAX_STANDING: f32 = 2_500.0;

/// How a footprint is drawn, by its height against the hero's feet. Each band is
/// laid down as one shape with an edge in its own colour, so where the ground steps
/// up or down a line runs — the map's contour lines. Drawn in this order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Band {
    /// Walls and rooms of a floor above the hero's (3–12 m up): a ghost.
    Above,
    /// Walls and rooms of a floor below the hero's (top 1.5–15 m down): a ghost.
    Below,
    /// Surfaces more than 4 m below the feet.
    Deep,
    /// Surfaces from 4 m to 1.2 m below.
    Lower,
    /// The hero's own level.
    Level,
    /// Surfaces up to 3 m above: steps, platforms, ledges.
    Raised,
    /// Cliffs and big rocks: boxes too big to be walls.
    High,
    /// What stands up at the hero's level: walls, pillars, doors.
    Wall,
}

impl Band {
    pub const ALL: [Band; 8] =
        [Band::Above, Band::Below, Band::Deep, Band::Lower, Band::Level, Band::Raised, Band::High, Band::Wall];

    /// (fill, edge).
    pub fn colours(self) -> (Rgba, Rgba) {
        match self {
            Band::Above => (Rgba(150, 190, 230, 14), Rgba(150, 200, 240, 70)),
            Band::Below => (Rgba(230, 170, 90, 14), Rgba(240, 180, 100, 75)),
            Band::Deep => (Rgba(40, 70, 120, 45), Rgba(70, 110, 170, 90)),
            Band::Lower => (Rgba(70, 110, 150, 55), Rgba(110, 160, 205, 140)),
            Band::Level => (Rgba(120, 130, 140, 55), Rgba(165, 175, 185, 140)),
            Band::Raised => (Rgba(190, 160, 105, 70), Rgba(235, 200, 130, 190)),
            Band::High => (Rgba(140, 100, 70, 45), Rgba(185, 140, 95, 120)),
            Band::Wall => (Rgba(185, 190, 196, 85), Rgba(240, 242, 244, 215)),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Band::Above => "위층 (흐리게)",
            Band::Below => "아래층·지하 (흐리게)",
            Band::Deep => "깊은 곳",
            Band::Lower => "낮은 곳",
            Band::Level => "같은 높이",
            Band::Raised => "높은 곳",
            Band::High => "절벽·바위",
            Band::Wall => "벽",
        }
    }
}

/// Which band a footprint falls in for a hero whose feet are at `feet` (cm), or
/// `None` when it is not drawn: a ceiling, or too far up or down to matter.
pub fn band(f: &Footprint, feet: f32) -> Option<Band> {
    let (bottom, top) = (f.zmin - feet, f.zmax - feet);
    let standing = f.height() >= FLAT && f.size() <= MAX_STANDING;
    // Other floors' walls, as ghosts: a floor up, a floor or a cellar down.
    if standing && bottom > 300.0 && bottom <= 1200.0 && top > bottom {
        return Some(Band::Above);
    }
    if standing && (-1500.0..-150.0).contains(&top) {
        return Some(Band::Below);
    }
    if bottom > 300.0 || top < -800.0 {
        return None; // overhead (ceilings, upper floors) or far below
    }
    if f.height() >= FLAT {
        if f.size() > MAX_STANDING {
            return Some(Band::High);
        }
        if bottom <= 250.0 && top >= -150.0 {
            return Some(Band::Wall);
        }
    }
    Some(match top {
        t if t < -400.0 => Band::Deep,
        t if t < -120.0 => Band::Lower,
        t if t <= 60.0 => Band::Level,
        t if t <= 300.0 => Band::Raised,
        _ => return None,
    })
}

/// Deadly water's shore, as an outline.
const SHORE: Rgba = Rgba(90, 170, 255, 230);
/// Contour spacing (cm): thin lines, and every so many a strong one.
const CONTOUR: f32 = 200.0;
const CONTOUR_MAJOR: f32 = 1000.0;

/// The landscape under the map's disc of radius `r` (pixels): its baked colours
/// (shaded, tinted, water) and/or contour lines.
fn draw_relief(cv: &mut Canvas, mode: ReliefMode, view: &View, rel: &Relief, r: f32, outline: bool) {
    let (w, h) = (cv.w, cv.h);
    let (cx, cy) = (w as f32 / 2.0, h as f32 / 2.0);
    // The map is a rotation and a scale of the world, so a pixel's place among the
    // texels is the first pixel's plus whole steps along a row and down a column.
    let texel = |p: [f32; 2]| [(p[0] - rel.origin[0]) / rel.res - 0.5, (p[1] - rel.origin[1]) / rel.res - 0.5];
    let first = texel(view.unproject(0.5 - cx, 0.5 - cy));
    let across = texel(view.unproject(1.5 - cx, 0.5 - cy));
    let down = texel(view.unproject(0.5 - cx, 1.5 - cy));
    let (ex, ey) = ([across[0] - first[0], across[1] - first[1]], [down[0] - first[0], down[1] - first[1]]);
    let (n, last) = (rel.n, (rel.n - 1) as f32);
    let rim = r * r;
    // Rows are independent: each worker takes a band of them. First every pixel's
    // ground height (bilinear, for contours) and nearest texel (colour)…
    let workers = std::thread::available_parallelism().map_or(4, |n| n.get()).clamp(1, 8);
    let band = h.div_ceil(workers);
    let mut z = vec![f32::NAN; w * h];
    let mut near = vec![u32::MAX; w * h];
    std::thread::scope(|scope| {
        for (k, (zs, ns)) in z.chunks_mut(band * w).zip(near.chunks_mut(band * w)).enumerate() {
            scope.spawn(move || {
                for (row, (zr, nr)) in zs.chunks_mut(w).zip(ns.chunks_mut(w)).enumerate() {
                    let py = k * band + row;
                    let dy = py as f32 + 0.5 - cy;
                    if dy * dy > rim {
                        continue;
                    }
                    let half = (rim - dy * dy).sqrt();
                    let (x0, x1) = (((cx - half).floor().max(0.0)) as usize, ((cx + half).ceil() as usize).min(w));
                    let mut tx = first[0] + ex[0] * x0 as f32 + ey[0] * py as f32;
                    let mut ty = first[1] + ex[1] * x0 as f32 + ey[1] * py as f32;
                    for px in x0..x1 {
                        if (0.0..last).contains(&tx) && (0.0..last).contains(&ty) {
                            let (ix, iy) = (tx as usize, ty as usize);
                            let (fx, fy) = (tx - ix as f32, ty - iy as f32);
                            let i = iy * n + ix;
                            let (a, b, c, d) = (rel.z[i], rel.z[i + 1], rel.z[i + n], rel.z[i + n + 1]);
                            zr[px] = (a + (b - a) * fx) * (1.0 - fy) + (c + (d - c) * fx) * fy;
                            nr[px] = ((ty + 0.5) as usize * n + (tx + 0.5) as usize) as u32;
                        }
                        tx += ex[0];
                        ty += ex[1];
                    }
                }
            });
        }
    });
    // …then colour and contours, again a band of rows each.
    let (z, near) = (&z, &near);
    std::thread::scope(|scope| {
        for (k, out) in cv.px.chunks_mut(band * w).enumerate() {
            scope.spawn(move || {
                for (row, out) in out.chunks_mut(w).enumerate() {
                    let py = k * band + row;
                    for (px, o) in out.iter_mut().enumerate() {
                        let i = py * w + px;
                        if near[i] == u32::MAX {
                            continue;
                        }
                        let t = near[i] as usize;
                        if outline {
                            // The shore: a wet pixel next to a dry one.
                            let shore = rel.wet[t]
                                && [(px + 1, py), (px.wrapping_sub(1), py), (px, py + 1), (px, py.wrapping_sub(1))]
                                    .iter()
                                    .any(|&(nx, ny)| {
                                        nx < w
                                            && ny < h
                                            && near[ny * w + nx] != u32::MAX
                                            && !rel.wet[near[ny * w + nx] as usize]
                                    });
                            if shore {
                                *o = over(*o, SHORE, SHORE.3 as u32);
                            }
                        } else if rel.wet[t] || mode.shade() {
                            let c = rel.colour[t];
                            if c.3 > 0 {
                                *o = over(*o, c, c.3 as u32);
                            }
                        }
                        let here = z[i];
                        if !mode.contour() || here.is_nan() {
                            continue;
                        }
                        let step = |v: f32, s: f32| (v / s).floor();
                        let mut edge = (false, false);
                        for (nx, ny) in [(px + 1, py), (px, py + 1)] {
                            if nx >= w || ny >= h {
                                continue;
                            }
                            let there = z[ny * w + nx];
                            if there.is_nan() {
                                continue;
                            }
                            edge.0 |= step(here, CONTOUR) != step(there, CONTOUR);
                            edge.1 |= step(here, CONTOUR_MAJOR) != step(there, CONTOUR_MAJOR);
                        }
                        let (major, minor) = if outline { (190, 90) } else { (150, 70) };
                        if edge.1 {
                            *o = over(*o, Rgba(236, 222, 180, major), major as u32);
                        } else if edge.0 && !outline {
                            *o = over(*o, Rgba(210, 205, 185, minor), minor as u32);
                        } else if edge.0 {
                            *o = over(*o, Rgba(200, 196, 180, minor), minor as u32);
                        }
                    }
                }
            });
        }
    });
}

/// A convex polygon's pixels (centres inside), set to at least `k` — no antialiasing.
fn fill_convex(class: &mut [u8], w: usize, h: usize, p: &[(f32, f32)], k: u8) {
    let (y0, y1) = (p.iter().map(|q| q.1).fold(f32::MAX, f32::min), p.iter().map(|q| q.1).fold(f32::MIN, f32::max));
    let (ly, hy) = ((y0 - 0.5).ceil().max(0.0) as usize, ((y1 - 0.5).floor().min(h as f32 - 1.0)) as i64);
    if hy < 0 {
        return;
    }
    for y in ly..=hy as usize {
        let py = y as f32 + 0.5;
        let (mut lo, mut hi) = (f32::MAX, f32::MIN);
        for i in 0..p.len() {
            let (a, b) = (p[i], p[(i + 1) % p.len()]);
            if (a.1 <= py && b.1 > py) || (b.1 <= py && a.1 > py) {
                let x = a.0 + (py - a.1) * (b.0 - a.0) / (b.1 - a.1);
                lo = lo.min(x);
                hi = hi.max(x);
            }
        }
        if lo > hi {
            continue;
        }
        let (lx, hx) = ((lo - 0.5).ceil().max(0.0) as usize, ((hi - 0.5).floor().min(w as f32 - 1.0)) as i64);
        if hx < lx as i64 {
            continue;
        }
        for c in &mut class[y * w + lx..=y * w + hx as usize] {
            *c = (*c).max(k);
        }
    }
}

/// Integer source-over of `c` at alpha `a` (0..=255) onto the pixel `d`: each
/// channel (s·a + d·(255 − a)) / 255.
#[inline]
fn over(d: u32, c: Rgba, a: u32) -> u32 {
    let keep = 255 - a;
    let mix = |shift: u32, s: u32| {
        let v = s * a + ((d >> shift) & 0xFF) * keep + 127;
        (((v + (v >> 8)) >> 8).min(255)) << shift
    };
    mix(24, 255) | mix(16, c.0 as u32) | mix(8, c.1 as u32) | mix(0, c.2 as u32)
}

thread_local! {
    /// The map's empty disc at the last size drawn, per thread.
    static BASE: std::cell::RefCell<Option<Canvas>> = const { std::cell::RefCell::new(None) };
}

/// One frame of the minimap: a disc of radius `r` px centred in the canvas.
/// With `icons`, things are drawn as icons; without, as coloured dots.
#[allow(clippy::too_many_arguments)]
pub fn draw_map(
    cv: &mut Canvas,
    state: &MapState,
    world: &str,
    view: &View,
    things: &[Thing],
    icons: Option<&Icons>,
    footprints: &[Footprint],
    goals: &[Goal],
    route: &Path,
    relief: Option<&Relief>,
) {
    let (cx, cy) = (cv.w as f32 / 2.0, cv.h as f32 / 2.0);
    let r = cx.min(cy) - 14.0;
    let inside = |p: (f32, f32)| p.0 * p.0 + p.1 * p.1 <= r * r;
    // The empty disc is the same every frame at a size: drawn once, then copied. As
    // outlines, there is no disc: the background stays clear.
    if view.outline {
        cv.clear();
    } else {
        BASE.with(|base| {
            let mut base = base.borrow_mut();
            if base.as_ref().is_none_or(|b: &Canvas| (b.w, b.h) != (cv.w, cv.h)) {
                let mut b = Canvas::new(cv.w, cv.h);
                b.disc(cx, cy, r, BACKGROUND);
                *base = Some(b);
            }
            cv.px.copy_from_slice(&base.as_ref().unwrap().px);
        });
    }
    if let Some(rel) = relief.filter(|_| state.relief != ReliefMode::Off) {
        draw_relief(cv, state.relief, view, rel, r, view.outline);
    }

    if state.terrain {
        // Feet are about 90 cm below the capsule's centre.
        let feet = view.center[2] - 90.0;
        let reach = r / view.scale.max(f32::EPSILON);
        // One class per pixel (0 = none, else 1 + the band's place in `Band::ALL`; the
        // later band wins where they overlap), filled without antialiasing, then
        // coloured once with an edge where a band ends. Overlaps do not pile up.
        let (w, h) = (cv.w, cv.h);
        let mut class = vec![0u8; w * h];
        for f in footprints {
            let c = f.center();
            let d = ((c[0] - view.center[0]).powi(2) + (c[1] - view.center[1]).powi(2)).sqrt();
            if d - f.reach() > reach {
                continue;
            }
            let Some(b) = band(f, feet) else { continue };
            let k = 1 + Band::ALL.iter().position(|x| *x == b).unwrap() as u8;
            let pts: Vec<(f32, f32)> = f
                .corners
                .iter()
                .map(|c| {
                    let (x, y) = view.project([c[0], c[1], 0.0]);
                    (cx + x, cy + y)
                })
                .collect();
            fill_convex(&mut class, w, h, &pts, k);
        }
        let rim = (r - 1.0) * (r - 1.0);
        let colours: Vec<(Rgba, Rgba)> = Band::ALL.iter().map(|b| b.colours()).collect();
        let (class, colours, outline) = (&class, &colours, view.outline);
        // As outlines only walls and raised floors are drawn: the cliff and rock boxes
        // are bigger than what they hold (the contours show the real ground), and the
        // lower bands' blue would read as water.
        let kept: Vec<bool> =
            Band::ALL.iter().map(|b| !outline || matches!(b, Band::Wall | Band::Raised | Band::Above | Band::Below)).collect();
        let kept = &kept;
        let class_at = move |i: usize| {
            let k = class[i];
            if k > 0 && kept[k as usize - 1] {
                k
            } else {
                0
            }
        };
        let workers = std::thread::available_parallelism().map_or(4, |n| n.get()).clamp(1, 8);
        let rows = h.div_ceil(workers);
        std::thread::scope(|scope| {
            for (k, out) in cv.px.chunks_mut(rows * w).enumerate() {
                scope.spawn(move || {
                    for (row, out) in out.chunks_mut(w).enumerate() {
                        let y = k * rows + row;
                        let dy = y as f32 + 0.5 - cy;
                        if y == 0 || y + 1 >= h || dy * dy > rim {
                            continue;
                        }
                        let half = (rim - dy * dy).sqrt();
                        let (x0, x1) =
                            (((cx - half).floor() as usize).max(1), ((cx + half).ceil() as usize).min(w - 1));
                        #[allow(clippy::needless_range_loop)]
                        #[allow(clippy::needless_range_loop)]
                        for x in x0..x1 {
                            let k = class_at(y * w + x);
                            if k == 0 {
                                // As outlines, a dark rim just outside each shape keeps its
                                // line readable over any background.
                                let beside = [
                                    class_at(y * w + x - 1),
                                    class_at(y * w + x + 1),
                                    class_at((y - 1) * w + x),
                                    class_at((y + 1) * w + x),
                                ];
                                if outline && beside.iter().any(|&n| n > 0) {
                                    out[x] = over(out[x], Rgba(0, 0, 0, 120), 120);
                                }
                                continue;
                            }
                            let (fill, edge) = colours[k as usize - 1];
                            let edged = [
                                class_at(y * w + x - 1),
                                class_at(y * w + x + 1),
                                class_at((y - 1) * w + x),
                                class_at((y + 1) * w + x),
                            ]
                            .iter()
                            .any(|&n| n < k);
                            if outline {
                                if edged {
                                    let c = Rgba(edge.0, edge.1, edge.2, 235);
                                    out[x] = over(out[x], c, 235);
                                }
                                continue;
                            }
                            let c = if edged { edge } else { fill };
                            out[x] = over(out[x], c, c.3 as u32);
                        }
                    }
                });
            }
        });
    }

    if let Some(trail) = state.trails.get(world) {
        // Newest last: the recent way bright, the old way fading out, so a long walk
        // does not cover the map in lines.
        let n = trail.len().max(2) as f32;
        for (i, pair) in trail.windows(2).enumerate() {
            let age = 1.0 - (i as f32 + 1.0) / (n - 1.0);
            let fresh = (1.0 - age).powf(1.6);
            let colour = Rgba(TRAIL.0, TRAIL.1, TRAIL.2, (TRAIL.3 as f32 * (0.08 + 0.92 * fresh)) as u8);
            let width = 1.2 + 1.0 * fresh;
            if let [Some(a), Some(b)] = pair {
                let (pa, pb) = (view.project(*a), view.project(*b));
                if inside(pa) || inside(pb) {
                    // Clip by shortening to the rim: good enough at walking scale.
                    let clip = |p: (f32, f32)| {
                        let d = (p.0 * p.0 + p.1 * p.1).sqrt();
                        if d > r {
                            (p.0 * r / d, p.1 * r / d)
                        } else {
                            p
                        }
                    };
                    let (pa, pb) = (clip(pa), clip(pb));
                    cv.line((cx + pa.0, cy + pa.1), (cx + pb.0, cy + pb.1), width, colour);
                }
            }
        }
    }

    if let Some(markers) = state.markers.get(world) {
        for m in markers {
            let p = view.project(*m);
            let d = (p.0 * p.0 + p.1 * p.1).sqrt();
            if d <= r - 5.0 {
                cv.disc(cx + p.0, cy + p.1, 5.0, MARKER);
            } else {
                // Off the map: a small arrow on the rim, pointing at it.
                let (ux, uy) = (p.0 / d, p.1 / d);
                let tip = (cx + ux * (r - 2.0), cy + uy * (r - 2.0));
                let base = (cx + ux * (r - 12.0), cy + uy * (r - 12.0));
                let (nx, ny) = (-uy * 5.0, ux * 5.0);
                cv.triangle([tip, (base.0 + nx, base.1 + ny), (base.0 - nx, base.1 - ny)], MARKER);
            }
        }
    }

    // Enemies last, so they sit on top of the rest.
    let mut sorted: Vec<&Thing> = things.iter().filter(|t| state.shows(t.sub)).collect();
    sorted.sort_by_key(|t| std::cmp::Reverse(t.kind()));
    for t in sorted {
        let (k, at) = (&t.kind(), &t.at);
        let p = view.project(*at);
        if p.0 * p.0 + p.1 * p.1 > (r - 6.0) * (r - 6.0) {
            continue;
        }
        // Another floor: faint, with an arrow up or down.
        let dz = (at[2] - view.center[2]) / 100.0;
        let alpha = floor_alpha(dz);
        match icons {
            Some(icons) => {
                let i = icons.get(t.sub);
                cv.blit_alpha(cx + p.0, cy + p.1, i.size, &i.px, alpha);
                floor_arrow(cv, cx + p.0 + i.size as f32 / 2.0, cy + p.1 - i.size as f32 / 2.0 + 3.0, dz);
            }
            None => {
                let size = if *k == Kind::Enemy { 4.5 } else { 3.5 };
                let c = colour(*k);
                cv.disc(cx + p.0, cy + p.1, size + 1.2, Rgba(OUTLINE.0, OUTLINE.1, OUTLINE.2, (OUTLINE.3 as u32 * alpha / 255) as u8));
                cv.disc(cx + p.0, cy + p.1, size, Rgba(c.0, c.1, c.2, (c.3 as u32 * alpha / 255) as u8));
                floor_arrow(cv, cx + p.0 + size + 2.0, cy + p.1 - size, dz);
            }
        }
    }

    // Goals: diamonds in their tier's colour; the guide's target larger, with a line
    // from the hero to it — or an arrow on the rim when it is off the map.
    for g in goals.iter().filter(|g| state.goal_tiers & (1 << g.tier as u8) != 0 || Some(g.id) == state.target) {
        let target = Some(g.id) == state.target;
        let [cr, cg, cb] = g.tier.rgb();
        let colour = Rgba(cr, cg, cb, 255);
        let p = view.project(g.at);
        let d = (p.0 * p.0 + p.1 * p.1).sqrt();
        if target && route.points.len() >= 2 {
            // The walking route: a line through its points, cut at the rim. Legs that go
            // through an obstacle — no way in was found — are dashed in yellow.
            let pts: Vec<(f32, f32)> = route.points.iter().map(|q| view.project([q[0], q[1], 0.0])).collect();
            for (k, s) in pts.windows(2).enumerate() {
                let through = route.through.get(k).copied().unwrap_or(false);
                let (a, b) = (s[0], s[1]);
                let inside = |p: (f32, f32)| p.0 * p.0 + p.1 * p.1 <= (r - 2.0) * (r - 2.0);
                if !inside(a) && !inside(b) {
                    continue;
                }
                let clip = |p: (f32, f32), q: (f32, f32)| {
                    if inside(p) {
                        return p;
                    }
                    // Walk from q towards p until the rim.
                    let (mut lo, mut hi) = (0.0f32, 1.0f32);
                    for _ in 0..12 {
                        let mid = (lo + hi) / 2.0;
                        let m = (q.0 + (p.0 - q.0) * mid, q.1 + (p.1 - q.1) * mid);
                        if inside(m) {
                            lo = mid;
                        } else {
                            hi = mid;
                        }
                    }
                    (q.0 + (p.0 - q.0) * lo, q.1 + (p.1 - q.1) * lo)
                };
                let (a, b) = (clip(a, b), clip(b, a));
                if through {
                    let len = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt().max(f32::EPSILON);
                    let (ux, uy) = ((b.0 - a.0) / len, (b.1 - a.1) / len);
                    let mut t = 0.0;
                    while t < len {
                        let e = (t + 5.0).min(len);
                        let (p0, p1) = ((cx + a.0 + ux * t, cy + a.1 + uy * t), (cx + a.0 + ux * e, cy + a.1 + uy * e));
                        cv.line(p0, p1, 4.5, Rgba(0, 0, 0, 160));
                        cv.line(p0, p1, 2.5, Rgba(255, 220, 60, 235));
                        t += 9.0;
                    }
                } else {
                    cv.line((cx + a.0, cy + a.1), (cx + b.0, cy + b.1), 4.5, Rgba(0, 0, 0, 160));
                    cv.line((cx + a.0, cy + a.1), (cx + b.0, cy + b.1), 2.5, Rgba(cr, cg, cb, 235));
                }
            }
        } else if target {
            let reach = d.min(r - 8.0);
            let (ux, uy) = (p.0 / d.max(f32::EPSILON), p.1 / d.max(f32::EPSILON));
            // Dashed: 6 px on, 4 off.
            let mut t = 10.0;
            while t < reach {
                let e = (t + 6.0).min(reach);
                cv.line((cx + ux * t, cy + uy * t), (cx + ux * e, cy + uy * e), 2.0, Rgba(cr, cg, cb, 200));
                t += 10.0;
            }
        }
        if d <= r - 6.0 {
            let s = if target { 7.0 } else { 4.5 };
            let (x, y) = (cx + p.0, cy + p.1);
            // Another floor: faint (the target less so), with an arrow up or down.
            let dz = (g.at[2] - view.center[2]) / 100.0;
            let a = if target { floor_alpha(dz).max(190) } else { floor_alpha(dz) };
            let fade = |c: Rgba| Rgba(c.0, c.1, c.2, (c.3 as u32 * a / 255) as u8);
            cv.polygon(&[(x, y - s - 1.5), (x + s + 1.5, y), (x, y + s + 1.5), (x - s - 1.5, y)], fade(OUTLINE));
            cv.polygon(&[(x, y - s), (x + s, y), (x, y + s), (x - s, y)], fade(colour));
            floor_arrow(cv, x + s + 3.0, y - s + 1.0, dz);
        } else if target {
            let (ux, uy) = (p.0 / d, p.1 / d);
            let tip = (cx + ux * (r - 1.0), cy + uy * (r - 1.0));
            let base = (cx + ux * (r - 13.0), cy + uy * (r - 13.0));
            let (nx, ny) = (-uy * 6.0, ux * 6.0);
            cv.triangle([tip, (base.0 + nx, base.1 + ny), (base.0 - nx, base.1 - ny)], colour);
        }
    }

    if !view.outline {
        cv.ring(cx, cy, r, 2.0, EDGE);
    }
    let (nx, ny) = view.north();
    cv.disc(cx + nx * r, cy + ny * r, 9.0, BACKGROUND);
    cv.letter_n(cx + nx * r, cy + ny * r, 11.0, NORTH);

    let (hx, hy) = view.heading();
    let (px, py) = (-hy, hx);
    let tip = (cx + hx * 11.0, cy + hy * 11.0);
    let left = (cx - hx * 7.0 + px * 7.0, cy - hy * 7.0 + py * 7.0);
    let right = (cx - hx * 7.0 - px * 7.0, cy - hy * 7.0 - py * 7.0);
    cv.triangle([tip, left, right], HERO);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alpha(cv: &Canvas, x: usize, y: usize) -> u32 {
        cv.px[y * cv.w + x] >> 24
    }

    #[test]
    fn a_disc_covers_its_middle_and_not_its_corners() {
        let mut cv = Canvas::new(20, 20);
        cv.disc(10.0, 10.0, 6.2, Rgba(255, 0, 0, 255));
        assert_eq!(cv.px[10 * 20 + 10], 0xFFFF_0000);
        assert_eq!(cv.px[0], 0);
        let edge = alpha(&cv, 16, 10);
        assert!(edge > 0 && edge < 255, "anti-aliased edge, got {edge}");
    }

    #[test]
    fn blending_is_premultiplied_source_over() {
        let mut cv = Canvas::new(1, 1);
        cv.blend(0, 0, Rgba(255, 255, 255, 128), 1.0);
        let p = cv.px[0];
        assert_eq!(p >> 24, 128);
        assert_eq!((p >> 16) & 0xFF, 128, "colour is scaled by alpha");
    }

    #[test]
    fn a_triangle_fills_either_winding() {
        for pts in [[(2.0, 2.0), (18.0, 2.0), (10.0, 18.0)], [(2.0, 2.0), (10.0, 18.0), (18.0, 2.0)]] {
            let mut cv = Canvas::new(20, 20);
            cv.triangle(pts, Rgba(0, 255, 0, 255));
            assert_eq!(alpha(&cv, 10, 6), 255);
            assert_eq!(alpha(&cv, 1, 18), 0);
        }
    }

    #[test]
    fn other_floors_are_ghosts_and_fade() {
        let wall = |z0: f32, z1: f32| Footprint { corners: [[0.0, 0.0], [100.0, 0.0], [100.0, 20.0], [0.0, 20.0]], zmin: z0, zmax: z1 };
        assert_eq!(band(&wall(0.0, 300.0), 0.0), Some(Band::Wall));
        assert_eq!(band(&wall(-1300.0, -1000.0), 0.0), Some(Band::Below), "a cellar's wall");
        assert_eq!(band(&wall(400.0, 700.0), 0.0), Some(Band::Above), "an upper floor's wall");
        assert_eq!(floor_alpha(0.0), 255);
        assert_eq!(floor_alpha(-12.0), 85);
    }

    #[test]
    fn the_compass_puts_north_ahead_and_a_target_where_it_lies() {
        let mut cv = Canvas::new(400, 60);
        // Facing north (yaw 0); a target due east (bearing 90) sits at the right end.
        draw_compass(&mut cv, 0.0, &[Pin { bearing: 45.0, rgb: [255, 0, 255], target: true, distance_m: 8.0, dz_m: -12.0 }]);
        let ppd = (400.0 / 2.0 - 22.0) / COMPASS_SPAN;
        let x = (200.0 + 45.0 * ppd) as usize;
        let px = cv.px[(34 - 13) * 400 + x];
        assert_eq!(((px >> 16) & 0xFF, px & 0xFF), (255, 255), "the target's diamond, 45° right");
        assert_eq!(wrap(350.0 - 10.0), -20.0);
        assert_eq!(wrap(10.0 - 350.0), 20.0);
    }

    #[test]
    fn distances_read_short() {
        assert_eq!(distance(84.6), "85m");
        assert_eq!(distance(1234.0), "1.2km");
    }

    #[test]
    fn a_square_polygon_fills_inside_only() {
        let mut cv = Canvas::new(20, 20);
        cv.polygon(&[(4.0, 4.0), (16.0, 4.0), (16.0, 16.0), (4.0, 16.0)], Rgba(255, 255, 255, 255));
        assert_eq!(cv.px[10 * 20 + 10] >> 24, 255);
        assert_eq!(cv.px[20 + 1] >> 24, 0);
        let mut cv = Canvas::new(20, 20);
        cv.polygon(&[(4.0, 4.0), (4.0, 16.0), (16.0, 16.0), (16.0, 4.0)], Rgba(255, 255, 255, 255));
        assert_eq!(cv.px[10 * 20 + 10] >> 24, 255, "either winding");
    }

    #[test]
    fn footprints_fall_in_bands_by_height_against_the_feet() {
        let fp = |z: f64, half_height: f64, half_width: f64| {
            crate::geometry::footprint([0.0, 0.0, z], 0.0, [1.0; 3], [0.0; 3], [half_width, half_width, half_height])
                .unwrap()
        };
        let feet = 0.0;
        assert_eq!(band(&fp(0.0, 10.0, 300.0), feet), Some(Band::Level), "the floor underfoot");
        assert_eq!(band(&fp(-250.0, 10.0, 300.0), feet), Some(Band::Lower));
        assert_eq!(band(&fp(-600.0, 10.0, 300.0), feet), Some(Band::Deep));
        assert_eq!(band(&fp(150.0, 10.0, 300.0), feet), Some(Band::Raised), "a platform");
        assert_eq!(band(&fp(150.0, 150.0, 100.0), feet), Some(Band::Wall));
        assert_eq!(band(&fp(500.0, 2000.0, 3000.0), feet), Some(Band::High), "a cliff");
        assert_eq!(band(&fp(450.0, 10.0, 300.0), feet), None, "a ceiling");
        assert_eq!(band(&fp(-2000.0, 10.0, 300.0), feet), None, "far below");
    }

    #[test]
    fn walls_on_the_heros_floor_are_drawn_and_ceilings_are_not() {
        let s = MapState { terrain: true, ..MapState::default() };
        let v = View {
            center: [0.0, 0.0, 100.0],
            yaw_deg: 0.0,
            heading_up: false,
            scale: 0.05,
            north_deg: 0.0,
            outline: false,
        };
        // A wall 10 m north of the hero, standing on their floor; a ceiling slab over them.
        let wall =
            crate::geometry::footprint([1000.0, 0.0, 100.0], 0.0, [1.0; 3], [0.0; 3], [100.0, 300.0, 150.0]).unwrap();
        let ceiling =
            crate::geometry::footprint([0.0, 0.0, 900.0], 0.0, [1.0; 3], [0.0; 3], [400.0, 400.0, 20.0]).unwrap();
        let mut cv = Canvas::new(200, 200);
        draw_map(&mut cv, &s, "W", &v, &[], None, &[wall, ceiling], &[], &Path::default(), None);
        // The wall at 10 m north, 0.05 px/cm: 50 px above the centre.
        assert!((cv.px[50 * 200 + 100] >> 16) & 0xFF > 60, "wall drawn");
        // Beside the hero (10 px left), where only the ceiling would be.
        let bg = cv.px[100 * 200 + 85];
        let mut plain = Canvas::new(200, 200);
        draw_map(&mut plain, &s, "W", &v, &[], None, &[], &[], &Path::default(), None);
        assert_eq!(bg, plain.px[100 * 200 + 85], "ceiling not drawn");
    }

    #[test]
    fn icons_blit_opaque_over_the_map() {
        let icons = Icons::new(16).unwrap();
        let mut cv = Canvas::new(200, 200);
        let s = MapState::default();
        let v = View { center: [0.0; 3], yaw_deg: 0.0, heading_up: true, scale: 0.01, north_deg: 0.0, outline: false };
        draw_map(
            &mut cv,
            &s,
            "W",
            &v,
            &[Thing { sub: crate::actors::Sub::Medicine, at: [3000.0, 0.0, 0.0] }],
            Some(&icons),
            &[],
            &[],
            &Path::default(),
            None,
        );
        assert!(cv.px[70 * 200 + 100] >> 24 > 200, "item icon 30 px above the centre");
    }

    #[test]
    fn a_map_frame_draws_the_hero_at_the_centre() {
        let mut cv = Canvas::new(200, 200);
        let mut s = MapState::default();
        s.observe("W", [0.0, 0.0, 0.0]);
        s.observe("W", [1000.0, 0.0, 0.0]);
        s.toggle_marker("W", [100_000.0, 0.0, 0.0]);
        let v = View { center: [0.0; 3], yaw_deg: 0.0, heading_up: true, scale: 0.01, north_deg: 0.0, outline: false };
        draw_map(
            &mut cv,
            &s,
            "W",
            &v,
            &[Thing { sub: crate::actors::Sub::Feral, at: [3000.0, 0.0, 0.0] }],
            None,
            &[],
            &[],
            &Path::default(),
            None,
        );
        assert_eq!(cv.px[100 * 200 + 100] >> 24, 255, "hero arrow");
        // 30 m ahead at 0.01 px/cm is 30 px above the centre, in enemy red.
        assert_eq!((cv.px[70 * 200 + 100] >> 16) & 0xFF, 235);
        s.layers = 0;
        draw_map(
            &mut cv,
            &s,
            "W",
            &v,
            &[Thing { sub: crate::actors::Sub::Feral, at: [3000.0, 0.0, 0.0] }],
            None,
            &[],
            &[],
            &Path::default(),
            None,
        );
        assert_ne!((cv.px[70 * 200 + 100] >> 16) & 0xFF, 235, "layer off, not drawn");
        assert_eq!(cv.px[0], 0, "outside the disc stays clear");
    }
}
