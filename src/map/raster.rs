//! One frame of the minimap: the landscape relief, the walls and floors by height band,
//! the trail, the pins, the things and the goals with the route, the hero — drawn on a
//! `Canvas` (canvas.rs). The canvas and the compass are re-exported from here.

use crate::actors::{Kind, Thing};
use crate::geometry::Footprint;
use crate::goals::Goal;
use crate::icons::Icons;
use crate::minimap::{MapState, ReliefMode, View};
use crate::pathfind::Path;
use crate::relief::Relief;

use super::canvas::over;
pub use super::canvas::*;
use super::compass::floor_arrow;
pub use super::compass::*;

const BACKGROUND: Rgba = Rgba(16, 18, 22, 170);
const EDGE: Rgba = Rgba(200, 200, 190, 200);
const TRAIL: Rgba = Rgba(150, 200, 255, 210);
/// A Haze's link to a Hollow Walker it keeps alive: the Lymbic violet.
const HAZE_LINK: Rgba = Rgba(196, 128, 255, 220);
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
            Band::Above => tr!("FLOORS_ABOVE_FADED"),
            Band::Below => tr!("FLOORS_BELOW_FADED"),
            Band::Deep => tr!("DEEP"),
            Band::Lower => tr!("LOW"),
            Band::Level => tr!("SAME_LEVEL"),
            Band::Raised => tr!("HIGH"),
            Band::High => tr!("CLIFFS_AND_ROCKS"),
            Band::Wall => tr!("WALLS"),
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

/// A colour at a layer's opacity (percent).
fn faded(c: Rgba, pct: u8) -> Rgba {
    Rgba(c.0, c.1, c.2, (c.3 as u32 * pct as u32 / 100) as u8)
}

/// Deadly water's shore, as an outline.
const SHORE: Rgba = Rgba(90, 170, 255, 230);
/// Contour spacing (cm): thin lines, and every so many a strong one.
const CONTOUR: f32 = 200.0;
const CONTOUR_MAJOR: f32 = 1000.0;

/// The landscape under the map's disc of radius `r` (pixels): its baked colours
/// (shaded, tinted, water) and/or contour lines.
fn draw_relief(cv: &mut Canvas, mode: ReliefMode, view: &View, rel: &Relief, r: f32, outline: bool, opacity: [u8; 3]) {
    let [ground, lines, _] = opacity;
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
    // ground height and wetness (bilinear, for contours and the shore) and nearest
    // texel (colour)…
    let workers = std::thread::available_parallelism().map_or(4, |n| n.get()).clamp(1, 8);
    let band = h.div_ceil(workers);
    let mut z = vec![f32::NAN; w * h];
    let mut wet = vec![f32::NAN; w * h];
    let mut near = vec![u32::MAX; w * h];
    std::thread::scope(|scope| {
        let chunks = z.chunks_mut(band * w).zip(wet.chunks_mut(band * w)).zip(near.chunks_mut(band * w));
        for (k, ((zs, ws), ns)) in chunks.enumerate() {
            scope.spawn(move || {
                for (row, ((zr, wr), nr)) in zs.chunks_mut(w).zip(ws.chunks_mut(w)).zip(ns.chunks_mut(w)).enumerate() {
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
                            if outline {
                                // Wet or not per texel, blurred over a texel (the mean of
                                // four samples half a texel apart) so the shore's line
                                // rounds the texels' corners instead of tracing them.
                                let mut sum = 0.0;
                                for (ox, oy) in [(-0.5, -0.5), (0.5, -0.5), (-0.5, 0.5), (0.5, 0.5)] {
                                    let (sx, sy) =
                                        ((tx + ox).clamp(0.0, last - 0.001), (ty + oy).clamp(0.0, last - 0.001));
                                    let (jx, jy) = (sx as usize, sy as usize);
                                    let (gx, gy) = (sx - jx as f32, sy - jy as f32);
                                    let j = jy * n + jx;
                                    let f = |j: usize| rel.wet[j] as u8 as f32;
                                    let (a, b, c, d) = (f(j), f(j + 1), f(j + n), f(j + n + 1));
                                    sum += (a + (b - a) * gx) * (1.0 - gy) + (c + (d - c) * gx) * gy;
                                }
                                wr[px] = sum / 4.0;
                            }
                            nr[px] = ((ty + 0.5) as usize * n + (tx + 0.5) as usize) as u32;
                        }
                        tx += ex[0];
                        ty += ex[1];
                    }
                }
            });
        }
    });
    // …then colour and contours, again a band of rows each. A line is drawn where a
    // field crosses a level: its coverage falls off with the pixel's distance from the
    // crossing, the field's distance to the level over its slope per pixel, so lines
    // come out smooth whatever their direction.
    let (z, wet, near) = (&z, &wet, &near);
    std::thread::scope(|scope| {
        for (k, out) in cv.px.chunks_mut(band * w).enumerate() {
            scope.spawn(move || {
                for (row, out) in out.chunks_mut(w).enumerate() {
                    let py = k * band + row;
                    let dy = py as f32 + 0.5 - cy;
                    for (px, o) in out.iter_mut().enumerate() {
                        let i = py * w + px;
                        if near[i] == u32::MAX {
                            continue;
                        }
                        // Lines fade out over the disc's last pixel rather than stop.
                        let dx = px as f32 + 0.5 - cx;
                        let d2 = dx * dx + dy * dy;
                        let inside =
                            if d2 < (r - 0.5) * (r - 0.5) { 1.0 } else { (r + 0.5 - d2.sqrt()).clamp(0.0, 1.0) };
                        let t = near[i] as usize;
                        if outline {
                            // The shore: where the wetness crosses one half.
                            let v = wet[i];
                            let cover = match v.is_nan() {
                                true => 0.0,
                                false => line_cover(v, slope2(wet, w, h, px, py), 0.5, 1.0, SHORE_HALF) * inside,
                            };
                            if cover > 0.0 {
                                let c = faded(SHORE, lines);
                                *o = over(*o, c, (c.3 as f32 * cover + 0.5) as u32);
                            }
                        } else if rel.wet[t] || mode.shade() {
                            let c = faded(rel.colour[t], ground);
                            if c.3 > 0 {
                                *o = over(*o, c, c.3 as u32);
                            }
                        }
                        if !mode.contour() || z[i].is_nan() {
                            continue;
                        }
                        let g = slope2(z, w, h, px, py);
                        let major_cover = line_cover(z[i], g, 0.0, CONTOUR_MAJOR, MAJOR_HALF) * inside;
                        let minor_cover = line_cover(z[i], g, 0.0, CONTOUR, MINOR_HALF) * inside;
                        let (major, minor) = if outline { (190, 90) } else { (150, 70) };
                        let (major, minor) = (major * lines as u32 / 100, minor * lines as u32 / 100);
                        // The thin line under the strong one, where they meet, does not show.
                        let minor_cover = minor_cover * (1.0 - major_cover);
                        let thin =
                            if outline { Rgba(200, 196, 180, minor as u8) } else { Rgba(210, 205, 185, minor as u8) };
                        let a = (minor as f32 * minor_cover + 0.5) as u32;
                        if a > 0 {
                            *o = over(*o, thin, a);
                        }
                        let a = (major as f32 * major_cover + 0.5) as u32;
                        if a > 0 {
                            *o = over(*o, Rgba(236, 222, 180, major as u8), a);
                        }
                    }
                }
            });
        }
    });
}

/// Half the width (px) of the thin contour lines, the strong ones and the shore.
const MINOR_HALF: f32 = 0.85;
const MAJOR_HALF: f32 = 1.0;
const SHORE_HALF: f32 = 1.0;

/// How steep the field `f` (one value per pixel, NaN where unknown) is at the pixel
/// (`x`, `y`), squared: its change per pixel, from the neighbouring pixels.
#[inline]
fn slope2(f: &[f32], w: usize, h: usize, x: usize, y: usize) -> f32 {
    let i = y * w + x;
    let here = f[i];
    let get = |ok: bool, j: usize| if ok { f[j] } else { f32::NAN };
    let diff = |a: f32, b: f32| match (a.is_nan(), b.is_nan()) {
        (false, false) => (b - a) / 2.0,
        (true, false) => b - here,
        (false, true) => here - a,
        (true, true) => 0.0,
    };
    let gx = diff(get(x > 0, i.wrapping_sub(1)), get(x + 1 < w, i + 1));
    let gy = diff(get(y > 0, i.wrapping_sub(w)), get(y + 1 < h, i + w));
    gx * gx + gy * gy
}

/// How much of a pixel whose field is `v`, changing by √`slope2` a pixel, a line
/// along the levels `base + k·spacing` covers: 1 on the line, falling to 0 at `half`
/// pixels from it.
#[inline]
fn line_cover(v: f32, slope2: f32, base: f32, spacing: f32, half: f32) -> f32 {
    let t = (v - base) / spacing;
    let off = (t - t.round()).abs() * spacing;
    // Most pixels are far from any line: no root for them.
    if off * off >= slope2 * half * half {
        return 0.0;
    }
    1.0 - off / (slope2.sqrt() * half)
}

/// The ground as dots: a square dot in each cell of a grid, the rest of the cell cleared,
/// so three quarters of the map is gap and the game shows through. A kept dot is drawn
/// stronger, so the ground still reads at a quarter of the ink. The dot grows with the
/// screen (`screen_h`: 1 px on a small one, 2 px on a 1440 px tall one) so it reads as a
/// dot, not as a fine mesh. Run on the ground (disc, relief, terrain), before anything
/// that must stay solid.
pub fn dots(cv: &mut Canvas, screen_h: usize) {
    const BOOST: u32 = 170; // percent
                            // Smaller than any screen (the Map page's preview, shown smaller still): the dots
                            // would be under a pixel there, so draw what they average to instead, a quarter of
                            // the ink boosted. A 1 px mask here, scaled down in the panel, beat with its pixels.
    if screen_h < 600 {
        const TONE: u32 = BOOST / 4; // percent
        for px in cv.px.iter_mut().filter(|p| **p != 0) {
            let c = |shift: u32| (((*px >> shift) & 0xFF) * TONE / 100) << shift;
            *px = c(24) | c(16) | c(8) | c(0);
        }
        return;
    }
    let dot = (screen_h / 600).clamp(1, 3);
    let pitch = dot * 2;
    // By rows: a gap row is cleared whole; on a dot row, runs of `dot` kept, `dot` cleared.
    for (y, row) in cv.px.chunks_mut(cv.w).enumerate() {
        if y % pitch >= dot {
            row.fill(0);
            continue;
        }
        for (x, px) in row.iter_mut().enumerate() {
            if *px == 0 {
                continue;
            }
            if x % pitch >= dot {
                *px = 0;
                continue;
            }
            // Premultiplied: alpha and colour scale together; colour never above alpha.
            let a = ((*px >> 24) * BOOST / 100).min(255);
            let c = |shift: u32| ((((*px >> shift) & 0xFF) * BOOST / 100).min(a)) << shift;
            *px = (a << 24) | c(16) | c(8) | c(0);
        }
    }
}

/// Samples per pixel each way for the terrain bands, whose edges are antialiased by
/// averaging them.
const SS: usize = 3;
// The bands' sample rows are read three at a time.
const _: () = assert!(SS == 3);

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

/// A ground layer kept for reuse: its canvas size, what it was drawn from, its pixels.
type Ground = ((usize, usize), u64, Vec<u32>);

thread_local! {
    /// The ground (disc and relief) last drawn at each size, by what it was drawn from.
    static GROUND: std::cell::RefCell<Vec<Ground>> = const { std::cell::RefCell::new(Vec::new()) };
    /// The map's empty disc at the last size drawn, per thread.
    static BASE: std::cell::RefCell<Option<(u8, Canvas)>> = const { std::cell::RefCell::new(None) };
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
    let r = draw_ground(cv, state, view, relief, footprints);
    if state.dots {
        dots(cv, cv.h);
    }
    draw_above(cv, state, world, view, things, icons, goals, route, r);
}

/// The map's radius on a canvas (px): a full-screen map's circle leaves `FULL_FILL` of the
/// short side's half, so the screen keeps a margin above and below it.
pub fn map_radius(cv: &Canvas, full: bool) -> f32 {
    let (cx, cy) = (cv.w as f32 / 2.0, cv.h as f32 / 2.0);
    if full {
        cx.min(cy) * FULL_FILL
    } else {
        cx.min(cy) - 14.0
    }
}

/// How much of the short side's half a full-screen map's circle takes.
pub const FULL_FILL: f32 = 0.86;

/// The ground (disc, relief, terrain) into `cv` out to `r` px from its centre, uncached:
/// the big map draws it larger than the screen and scrolls it (overlay `Scroll`).
pub fn paint_ground(
    cv: &mut Canvas,
    state: &MapState,
    view: &View,
    relief: Option<&Relief>,
    footprints: &[Footprint],
    r: f32,
) {
    let (cx, cy) = (cv.w as f32 / 2.0, cv.h as f32 / 2.0);
    let ground = state.opacity[0];
    // The empty disc is the same every frame at a size: drawn once, then copied. As
    // outlines, there is no disc: the background stays clear.
    if view.outline {
        cv.clear();
    } else if view.full {
        let c = faded(BACKGROUND, ground);
        let px = if c.3 == 0 {
            0
        } else {
            let a = c.3 as u32;
            (a << 24) | ((c.0 as u32 * a / 255) << 16) | ((c.1 as u32 * a / 255) << 8) | (c.2 as u32 * a / 255)
        };
        cv.px.fill(px);
    } else {
        BASE.with(|base| {
            let mut base = base.borrow_mut();
            if base.as_ref().is_none_or(|(g, b): &(u8, Canvas)| (b.w, b.h, *g) != (cv.w, cv.h, ground)) {
                let mut b = Canvas::new(cv.w, cv.h);
                b.disc(cx, cy, r, faded(BACKGROUND, ground));
                *base = Some((ground, b));
            }
            cv.px.copy_from_slice(&base.as_ref().unwrap().1.px);
        });
    }
    if let Some(rel) = relief.filter(|_| state.relief != ReliefMode::Off) {
        draw_relief(cv, state.relief, view, rel, r, view.outline, state.opacity);
    }
    if state.terrain {
        draw_terrain(cv, state, view, footprints, r);
    }
}

/// The ground under everything (the disc and the relief) into `cv`; the map's radius.
pub fn draw_ground(
    cv: &mut Canvas,
    state: &MapState,
    view: &View,
    relief: Option<&Relief>,
    footprints: &[Footprint],
) -> f32 {
    let r = map_radius(cv, view.full);
    let [ground, lines, _] = state.opacity;
    // The ground (disc, relief, terrain) depends only on where the map stands and how it
    // is drawn: while the hero stands still (or moves under a pixel) it is copied from
    // the last frame of that size instead of drawn again: it is most of a frame.
    let relief = relief.filter(|_| state.relief != ReliefMode::Off);
    let key = (relief.is_some() || state.terrain).then(|| {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        let (px, py) = view.project([0.0, 0.0, 0.0]);
        ((px.round() as i64, py.round() as i64), view.scale.to_bits(), view.north_deg.to_bits()).hash(&mut h);
        (view.heading_up, view.heading_up.then(|| view.yaw_deg.to_bits()), view.outline, view.full).hash(&mut h);
        (state.relief.key(), ground, lines, relief.map(|rel| (rel.z.as_ptr() as usize, rel.feet.to_bits())))
            .hash(&mut h);
        // The terrain's bands are by height against the feet: to 20 cm.
        (state.terrain, footprints.as_ptr() as usize, footprints.len(), (view.center[2] / 20.0).round() as i64)
            .hash(&mut h);
        h.finish()
    });
    if let Some(k) = key {
        let hit = GROUND.with(|g| {
            let g = g.borrow();
            g.iter().find(|(size, kk, _)| *size == (cv.w, cv.h) && *kk == k).map(|(_, _, px)| cv.px.copy_from_slice(px))
        });
        if hit.is_some() {
            return r;
        }
    }
    paint_ground(cv, state, view, relief, footprints, r);
    if let Some(k) = key {
        GROUND.with(|g| {
            let mut g = g.borrow_mut();
            g.retain(|(size, _, _)| *size != (cv.w, cv.h));
            g.push(((cv.w, cv.h), k, cv.px.clone()));
            // A few sizes are drawn on this thread: the minimap, the big map, previews.
            if g.len() > 4 {
                g.remove(0);
            }
        });
    }
    r
}

/// The terrain's walls and floors (`state.terrain`) as bands by height against the feet,
/// over the ground. Supersampled, so the heaviest layer: it is part of the ground, cached
/// and drawn at half size for the big map.
fn draw_terrain(cv: &mut Canvas, state: &MapState, view: &View, footprints: &[Footprint], r: f32) {
    let (cx, cy) = (cv.w as f32 / 2.0, cv.h as f32 / 2.0);
    let [ground, lines, _] = state.opacity;
    // Feet are about 90 cm below the capsule's centre.
    let feet = view.center[2] - 90.0;
    let reach = r / view.scale.max(f32::EPSILON);
    // One class per sample (0 = none, else 1 + the band's place in `Band::ALL`; the
    // later band wins where they overlap), SS × SS samples a pixel, then coloured
    // once with an edge where a band ends. Overlaps do not pile up.
    let (w, h) = (cv.w, cv.h);
    let (sw, sh) = (w * SS, h * SS);
    let mut class = vec![0u8; sw * sh];
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
                ((cx + x) * SS as f32, (cy + y) * SS as f32)
            })
            .collect();
        fill_convex(&mut class, sw, sh, &pts, k);
    }
    let rim = r - 1.0;
    let outline = view.outline;
    // As outlines only walls and raised floors are drawn: the cliff and rock boxes
    // are bigger than what they hold (the contours show the real ground), and the
    // lower bands' blue would read as water.
    let kept: Vec<bool> = Band::ALL
        .iter()
        .map(|b| !outline || matches!(b, Band::Wall | Band::Raised | Band::Above | Band::Below))
        .collect();
    // Each class's (fill, edge) at the layers' opacity; as outlines, no fill and a
    // stronger edge. Index 0 is the dark rim just outside each shape, as outlines,
    // which keeps its line readable over any background.
    let none = Rgba(0, 0, 0, 0);
    let mut paint = vec![(none, faded(Rgba(0, 0, 0, if outline { 120 } else { 0 }), lines))];
    paint.extend(Band::ALL.iter().map(|b| {
        let (fill, edge) = b.colours();
        match outline {
            true => (none, faded(Rgba(edge.0, edge.1, edge.2, 235), lines)),
            false => (faded(fill, ground), faded(edge, lines)),
        }
    }));
    // A band left out is drawn as nothing, still over what it covers.
    let mut map = [0u8; 9];
    for (k, m) in map.iter_mut().enumerate().skip(1) {
        *m = if kept[k - 1] { k as u8 } else { 0 };
    }
    let workers = std::thread::available_parallelism().map_or(4, |n| n.get()).clamp(1, 8);
    let rows = h.div_ceil(workers);
    // Each pixel's band where all its samples are of one, else MIXED: where a
    // pixel and the four beside it are all of one band, it is a plain fill (or
    // nothing), with no need to look at the samples one by one.
    const MIXED: u8 = u8::MAX;
    let mut span = vec![0u8; w * h];
    std::thread::scope(|scope| {
        for (k, out) in span.chunks_mut(rows * w).enumerate() {
            let (class, map) = (&class, &map);
            scope.spawn(move || {
                for (row, out) in out.chunks_mut(w).enumerate() {
                    let y = k * rows + row;
                    let sub = |j: usize| class[(y * SS + j) * sw..(y * SS + j + 1) * sw].chunks_exact(SS);
                    for (o, ((a, b), c)) in out.iter_mut().zip(sub(0).zip(sub(1)).zip(sub(2))) {
                        let k = a[0];
                        *o = match a.iter().chain(b).chain(c).all(|&v| v == k) {
                            true => map[k as usize],
                            false => MIXED,
                        };
                    }
                }
            });
        }
    });
    let (class, paint, map, span) = (&class, &paint, &map, &span);
    let class_at = move |i: usize| map[class[i] as usize];
    std::thread::scope(|scope| {
        for (k, out) in cv.px.chunks_mut(rows * w).enumerate() {
            scope.spawn(move || {
                for (row, out) in out.chunks_mut(w).enumerate() {
                    let y = k * rows + row;
                    let dy = y as f32 + 0.5 - cy;
                    if y == 0 || y + 1 >= h || dy.abs() > rim + 0.5 {
                        continue;
                    }
                    let half = ((rim + 0.5).powi(2) - dy * dy).max(0.0).sqrt();
                    let (x0, x1) = (((cx - half).floor() as usize).max(1), ((cx + half).ceil() as usize).min(w - 1));
                    // The bands fade out over the disc's last pixel.
                    let fade = |x: usize| {
                        let dx = x as f32 + 0.5 - cx;
                        let d2 = dx * dx + dy * dy;
                        match d2 < (rim - 0.5) * (rim - 0.5) {
                            true => 1.0,
                            false => (rim + 0.5 - d2.sqrt()).clamp(0.0, 1.0),
                        }
                    };
                    for x in x0..x1 {
                        let p = y * w + x;
                        let k = span[p];
                        if k != MIXED {
                            let near = [span[p - 1], span[p + 1], span[p - w], span[p + w]];
                            if k == 0 && (!outline || near.iter().all(|&n| n == 0)) {
                                continue;
                            }
                            if k > 0 && near.iter().all(|&n| n != MIXED && n >= k) {
                                let c = paint[k as usize].0;
                                let a = (c.3 as f32 * fade(x) + 0.5) as u32;
                                if a > 0 {
                                    out[x] = over(out[x], c, a);
                                }
                                continue;
                            }
                        }
                        // Each sample is a pixel's worth of the plain test — a band's
                        // edge where a sample one pixel away is of a lower band — at
                        // its own offset: their mean is the edge, antialiased.
                        let (mut sa, mut sr, mut sg, mut sb) = (0u32, 0u32, 0u32, 0u32);
                        for sy in y * SS..(y + 1) * SS {
                            let at = sy * sw;
                            let cells = &class[at + x * SS..at + (x + 1) * SS];
                            for (j, &raw) in cells.iter().enumerate() {
                                let i = at + x * SS + j;
                                let k = map[raw as usize];
                                let beside =
                                    [class_at(i - SS), class_at(i + SS), class_at(i - SS * sw), class_at(i + SS * sw)];
                                let c = if k == 0 {
                                    if !outline || beside.iter().all(|&n| n == 0) {
                                        continue;
                                    }
                                    paint[0].1
                                } else if beside.iter().any(|&n| n < k) {
                                    paint[k as usize].1
                                } else {
                                    paint[k as usize].0
                                };
                                let a = c.3 as u32;
                                sa += a;
                                sr += c.0 as u32 * a;
                                sg += c.1 as u32 * a;
                                sb += c.2 as u32 * a;
                            }
                        }
                        if sa == 0 {
                            continue;
                        }
                        let a = (sa as f32 * fade(x) / (SS * SS) as f32 + 0.5) as u32;
                        if a > 0 {
                            let c = Rgba((sr / sa) as u8, (sg / sa) as u8, (sb / sa) as u8, 255);
                            out[x] = over(out[x], c, a);
                        }
                    }
                }
            });
        }
    });
}

/// Everything over the ground (and its dots): the trail, pins, things,
/// goals and the route, the rim, north and the hero.
#[allow(clippy::too_many_arguments)]
pub fn draw_above(
    cv: &mut Canvas,
    state: &MapState,
    world: &str,
    view: &View,
    things: &[Thing],
    icons: Option<&Icons>,
    goals: &[Goal],
    route: &Path,
    r: f32,
) {
    let (cx, cy) = (cv.w as f32 / 2.0, cv.h as f32 / 2.0);
    let inside = |p: (f32, f32)| p.0 * p.0 + p.1 * p.1 <= r * r;
    let [ground, lines, marks] = state.opacity;

    if let Some(trail) = state.trails.get(world) {
        // Newest last: the recent way bright, the old way fading out, so a long walk
        // does not cover the map in lines.
        // Only the latest stretch: older points would be all but transparent, yet each
        // was still a line to draw. The trail itself keeps them all (it is small).
        const DRAWN: usize = 2000;
        let trail = &trail[trail.len().saturating_sub(DRAWN)..];
        let n = trail.len().max(2) as f32;
        // A segment is drawn from the last point drawn to one at least `STEP` px on (or
        // the last before a gap): on a wide map a walk's points are a pixel or two apart,
        // thousands of lines where a few hundred look the same.
        const STEP: f32 = 4.0;
        let mut from: Option<(f32, f32)> = None;
        for (i, point) in trail.iter().enumerate() {
            let Some(point) = point else {
                from = None;
                continue;
            };
            let pb = view.project(*point);
            let Some(pa) = from else {
                from = Some(pb);
                continue;
            };
            let end = trail.get(i + 1).is_none_or(|next| next.is_none());
            if !end && (pb.0 - pa.0).hypot(pb.1 - pa.1) < STEP {
                continue;
            }
            from = Some(pb);
            let age = 1.0 - i as f32 / (n - 1.0);
            let fresh = (1.0 - age).powf(1.6);
            // Fading to nothing at the slice's start, so it has no edge.
            let colour = faded(Rgba(TRAIL.0, TRAIL.1, TRAIL.2, (TRAIL.3 as f32 * fresh) as u8), lines);
            if colour.3 == 0 {
                continue;
            }
            let width = 1.2 + 1.0 * fresh;
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

    if let Some(markers) = state.markers.get(world) {
        for m in markers {
            let p = view.project(m.at);
            let d = (p.0 * p.0 + p.1 * p.1).sqrt();
            let [pr, pg, pb] = m.kind.rgb();
            let colour = faded(Rgba(pr, pg, pb, 255), marks);
            if d <= r - 5.0 {
                match icons {
                    // Its kind's icon, the pin's point on the spot.
                    Some(icons) => {
                        let i = icons.pin(m.kind);
                        let a = 255 * marks as u32 / 100;
                        cv.blit_alpha(cx + p.0, cy + p.1 - i.size as f32 * 0.42, i.size, &i.px, a);
                    }
                    None => {
                        cv.disc(cx + p.0, cy + p.1, 6.5, faded(OUTLINE, marks));
                        cv.disc(cx + p.0, cy + p.1, 5.0, colour);
                    }
                }
            } else {
                // Off the map: a small arrow on the rim, pointing at it.
                let (ux, uy) = (p.0 / d, p.1 / d);
                let tip = (cx + ux * (r - 2.0), cy + uy * (r - 2.0));
                let base = (cx + ux * (r - 12.0), cy + uy * (r - 12.0));
                let (nx, ny) = (-uy * 5.0, ux * 5.0);
                cv.triangle([tip, (base.0 + nx, base.1 + ny), (base.0 - nx, base.1 - ny)], colour);
            }
        }
    }

    // A thin line from each Haze to every Hollow Walker it keeps alive, so the one to kill
    // first shows (JOURNEY §3.7): with the enemies, under their icons.
    if state.layers & Kind::Enemy.bit() != 0 {
        let colour = faded(HAZE_LINK, lines);
        for (haze, walker) in &state.haze_links {
            let (a, b) = (view.project(*haze), view.project(*walker));
            if inside(a) || inside(b) {
                cv.line((cx + a.0, cy + a.1), (cx + b.0, cy + b.1), 1.4, colour);
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
        let alpha = floor_alpha(dz) * marks as u32 / 100;
        match icons {
            Some(icons) => {
                let i = icons.get(t.sub);
                cv.blit_alpha(cx + p.0, cy + p.1, i.size, &i.px, alpha);
                floor_arrow(cv, cx + p.0 + i.size as f32 / 2.0, cy + p.1 - i.size as f32 / 2.0 + 3.0, dz);
            }
            None => {
                let size = if *k == Kind::Enemy { 4.5 } else { 3.5 };
                let c = colour(*k);
                cv.disc(
                    cx + p.0,
                    cy + p.1,
                    size + 1.2,
                    Rgba(OUTLINE.0, OUTLINE.1, OUTLINE.2, (OUTLINE.3 as u32 * alpha / 255) as u8),
                );
                cv.disc(cx + p.0, cy + p.1, size, Rgba(c.0, c.1, c.2, (c.3 as u32 * alpha / 255) as u8));
                floor_arrow(cv, cx + p.0 + size + 2.0, cy + p.1 - size, dz);
            }
        }
    }

    // Goals: diamonds in their tier's colour; the guide's target larger, with a line
    // from the hero to it — or an arrow on the rim when it is off the map.
    for g in goals.iter().filter(|g| state.goal_tiers & (1 << g.tier as u8) != 0 || Some(g.id) == state.target) {
        // A map pin guided to: its route is drawn here, the pin itself above.
        let pin = crate::minimap::is_pin(g.id);
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
                        cv.line(p0, p1, 4.5, faded(Rgba(0, 0, 0, 160), lines));
                        cv.line(p0, p1, 2.5, faded(Rgba(255, 220, 60, 235), lines));
                        t += 9.0;
                    }
                } else {
                    cv.line((cx + a.0, cy + a.1), (cx + b.0, cy + b.1), 4.5, faded(Rgba(0, 0, 0, 160), lines));
                    cv.line((cx + a.0, cy + a.1), (cx + b.0, cy + b.1), 2.5, faded(Rgba(cr, cg, cb, 235), lines));
                }
            }
        } else if target {
            let reach = d.min(r - 8.0);
            let (ux, uy) = (p.0 / d.max(f32::EPSILON), p.1 / d.max(f32::EPSILON));
            // Dashed: 6 px on, 4 off.
            let mut t = 10.0;
            while t < reach {
                let e = (t + 6.0).min(reach);
                cv.line(
                    (cx + ux * t, cy + uy * t),
                    (cx + ux * e, cy + uy * e),
                    2.0,
                    faded(Rgba(cr, cg, cb, 200), lines),
                );
                t += 10.0;
            }
        }
        if d <= r - 6.0 && !pin {
            let s = if target { 7.0 } else { 4.5 };
            let (x, y) = (cx + p.0, cy + p.1);
            // Another floor: faint (the target less so), with an arrow up or down.
            let dz = (g.at[2] - view.center[2]) / 100.0;
            let a = if target { floor_alpha(dz).max(190) } else { floor_alpha(dz) } * marks as u32 / 100;
            let fade = |c: Rgba| Rgba(c.0, c.1, c.2, (c.3 as u32 * a / 255) as u8);
            cv.polygon(&[(x, y - s - 1.5), (x + s + 1.5, y), (x, y + s + 1.5), (x - s - 1.5, y)], fade(OUTLINE));
            cv.polygon(&[(x, y - s), (x + s, y), (x, y + s), (x - s, y)], fade(colour));
            floor_arrow(cv, x + s + 3.0, y - s + 1.0, dz);
        } else if target {
            let (ux, uy) = (p.0 / d, p.1 / d);
            let tip = (cx + ux * (r - 1.0), cy + uy * (r - 1.0));
            let base = (cx + ux * (r - 13.0), cy + uy * (r - 13.0));
            let (nx, ny) = (-uy * 6.0, ux * 6.0);
            cv.triangle([tip, (base.0 + nx, base.1 + ny), (base.0 - nx, base.1 - ny)], faded(colour, marks));
        }
    }

    if !view.outline && !view.full {
        cv.ring(cx, cy, r, 2.0, faded(EDGE, ground.max(lines)));
    }
    if !view.full {
        let (nx, ny) = view.north();
        cv.disc(cx + nx * r, cy + ny * r, 9.0, faded(BACKGROUND, marks));
        cv.letter_n(cx + nx * r, cy + ny * r, 11.0, faded(NORTH, marks));
    }

    let (hx, hy) = view.heading();
    let (px, py) = (-hy, hx);
    let tip = (cx + hx * 11.0, cy + hy * 11.0);
    let left = (cx - hx * 7.0 + px * 7.0, cy - hy * 7.0 + py * 7.0);
    let right = (cx - hx * 7.0 - px * 7.0, cy - hy * 7.0 - py * 7.0);
    cv.triangle([tip, left, right], faded(HERO, marks));
}

/// How a legend entry is drawn: a short line, a dashed one, or a filled square.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Swatch {
    Line,
    Dashed,
    Fill,
}

/// What the map draws with the settings as they are, for the Map page's legend: each
/// line or area's swatch, colour (the same constants it is drawn with) and name.
pub fn legend(state: &MapState, outline: bool) -> Vec<(Swatch, Rgba, &'static str)> {
    let mut out = Vec::new();
    if state.terrain {
        for b in Band::ALL {
            let kept = !outline || matches!(b, Band::Wall | Band::Raised | Band::Above | Band::Below);
            if kept {
                out.push((Swatch::Line, b.colours().1, b.label()));
            }
        }
    }
    if state.relief.contour() {
        out.push((Swatch::Line, Rgba(236, 222, 180, 230), tr!("LEGEND_CONTOUR")));
    }
    if outline {
        out.push((Swatch::Line, SHORE, tr!("LEGEND_SHORE")));
    } else if state.relief != ReliefMode::Off {
        out.push((Swatch::Fill, Rgba(40, 95, 175, 230), tr!("LEGEND_WATER")));
    }
    out.push((Swatch::Line, TRAIL, tr!("LEGEND_TRAIL")));
    if state.layers & Kind::Enemy.bit() != 0 {
        out.push((Swatch::Line, HAZE_LINK, tr!("LEGEND_HAZE_LINK")));
    }
    out.push((Swatch::Line, Rgba(245, 120, 200, 235), tr!("LEGEND_ROUTE")));
    out.push((Swatch::Dashed, Rgba(255, 220, 60, 235), tr!("LEGEND_BLOCKED")));
    out
}

/// The big map's soft edge: everything keeps its full strength out to `INNER` of the
/// way to the edge of a circle as wide as the canvas's short side, then fades to nothing
/// there, as Diablo's and Path of Exile's overlay maps do. A circle, not an ellipse of
/// the screen's shape: on a wide screen the sides would reach past what the game has
/// loaded and be cut off. Worked out once per size (per thread), multiplied each frame.
pub fn fade_edges(cv: &mut Canvas) {
    const INNER: f32 = 0.45;
    thread_local! {
        static MASK: std::cell::RefCell<(usize, usize, Vec<u16>)> = const { std::cell::RefCell::new((0, 0, Vec::new())) };
    }
    let (w, h) = (cv.w, cv.h);
    MASK.with(|m| {
        let mut m = m.borrow_mut();
        if (m.0, m.1) != (w, h) {
            let (cx, cy) = (w as f32 / 2.0, h as f32 / 2.0);
            let rad = cx.min(cy) * FULL_FILL;
            let mut k = vec![256u16; w * h];
            for y in 0..h {
                let dy = (y as f32 + 0.5 - cy) / rad;
                for x in 0..w {
                    let dx = (x as f32 + 0.5 - cx) / rad;
                    let t = ((dx * dx + dy * dy).sqrt() - INNER) / (1.0 - INNER);
                    k[y * w + x] = if t <= 0.0 {
                        256
                    } else if t >= 1.0 {
                        0
                    } else {
                        ((1.0 - t * t * (3.0 - 2.0 * t)) * 256.0) as u16
                    };
                }
            }
            *m = (w, h, k);
        }
        let mask = &m.2;
        let workers = std::thread::available_parallelism().map_or(4, |n| n.get()).clamp(1, 8);
        let rows = h.div_ceil(workers).max(1);
        std::thread::scope(|scope| {
            for (band, (out, ks)) in cv.px.chunks_mut(rows * w).zip(mask.chunks(rows * w)).enumerate() {
                let _ = band;
                scope.spawn(move || {
                    for (px, &k) in out.iter_mut().zip(ks) {
                        if *px == 0 || k == 256 {
                            continue;
                        }
                        let k = k as u32;
                        let c = |shift: u32| ((((*px >> shift) & 0xFF) * k) >> 8) << shift;
                        *px = c(24) | c(16) | c(8) | c(0);
                    }
                });
            }
        });
    });
}

/// `src` twice as big each way into `dst` (premultiplied stays premultiplied): the big
/// map is drawn at half size and shown at full, a quarter of the work. Doubling puts
/// each output pixel a quarter of a source pixel from its nearest source centre, so
/// bilinear weights are always 3/4 and 1/4: integer arithmetic, rows in parallel.
pub fn upscale2(src: &Canvas, dst: &mut Canvas) {
    let (sw, sh, dw) = (src.w, src.h, dst.w);
    let workers = std::thread::available_parallelism().map_or(4, |n| n.get()).clamp(1, 8);
    let rows = dst.h.div_ceil(workers).max(1);
    std::thread::scope(|scope| {
        for (k, out) in dst.px.chunks_mut(rows * dw).enumerate() {
            scope.spawn(move || {
                for (row, line) in out.chunks_mut(dw).enumerate() {
                    let y = k * rows + row;
                    // The nearer source row (weight 3) and the farther (weight 1).
                    let near_y = (y / 2).min(sh - 1);
                    let far_y = if y % 2 == 0 { near_y.saturating_sub(1) } else { (near_y + 1).min(sh - 1) };
                    for (x, o) in line.iter_mut().enumerate() {
                        let near_x = (x / 2).min(sw - 1);
                        let far_x = if x % 2 == 0 { near_x.saturating_sub(1) } else { (near_x + 1).min(sw - 1) };
                        let (a, b) = (src.px[near_y * sw + near_x], src.px[near_y * sw + far_x]);
                        let (c, d) = (src.px[far_y * sw + near_x], src.px[far_y * sw + far_x]);
                        if (a | b | c | d) == 0 {
                            *o = 0;
                            continue;
                        }
                        // Weights 9, 3, 3, 1 (of 16).
                        let mix = |s: u32| {
                            let ch = |p: u32| (p >> s) & 0xFF;
                            ((ch(a) * 9 + ch(b) * 3 + ch(c) * 3 + ch(d) + 8) >> 4) << s
                        };
                        *o = mix(24) | mix(16) | mix(8) | mix(0);
                    }
                }
            });
        }
    });
}

/// `src` made `w` × `h` (smaller), each pixel the mean of the source pixels it covers.
pub fn downscale(src: &Canvas, w: usize, h: usize) -> Canvas {
    let mut out = Canvas::new(w, h);
    let (sx, sy) = (src.w as f32 / w as f32, src.h as f32 / h as f32);
    for y in 0..h {
        let (y0, y1) = ((y as f32 * sy) as usize, (((y + 1) as f32 * sy) as usize).clamp(1, src.h));
        for x in 0..w {
            let (x0, x1) = ((x as f32 * sx) as usize, (((x + 1) as f32 * sx) as usize).clamp(1, src.w));
            let mut sum = [0u32; 4];
            let mut n = 0u32;
            for yy in y0..y1.max(y0 + 1) {
                for xx in x0..x1.max(x0 + 1) {
                    let p = src.px[yy.min(src.h - 1) * src.w + xx.min(src.w - 1)];
                    for (k, s) in sum.iter_mut().enumerate() {
                        *s += (p >> (24 - 8 * k as u32)) & 0xFF;
                    }
                    n += 1;
                }
            }
            let c = |k: usize| (sum[k] / n.max(1)) << (24 - 8 * k as u32);
            out.px[y * w + x] = c(0) | c(1) | c(2) | c(3);
        }
    }
    out
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
        let wall = |z0: f32, z1: f32| Footprint {
            corners: [[0.0, 0.0], [100.0, 0.0], [100.0, 20.0], [0.0, 20.0]],
            zmin: z0,
            zmax: z1,
        };
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
        draw_compass(
            &mut cv,
            0.0,
            &[Pin { bearing: 45.0, rgb: [255, 0, 255], target: true, distance_m: 8.0, dz_m: -12.0 }],
        );
        let ppd = (400.0 / 2.0 - 22.0) / COMPASS_SPAN;
        let x = (200.0 + 45.0 * ppd) as usize;
        let px = cv.px[(34 - 13) * 400 + x];
        assert_eq!(((px >> 16) & 0xFF, px & 0xFF), (255, 255), "the target's diamond, 45° right");
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
        let s = MapState { terrain: true, dots: false, ..MapState::default() };
        let v = View {
            center: [0.0, 0.0, 100.0],
            yaw_deg: 0.0,
            heading_up: false,
            scale: 0.05,
            north_deg: 0.0,
            outline: false,
            full: false,
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
        let v = View {
            center: [0.0; 3],
            yaw_deg: 0.0,
            heading_up: true,
            scale: 0.01,
            north_deg: 0.0,
            outline: false,
            full: false,
        };
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
        let v = View {
            center: [0.0; 3],
            yaw_deg: 0.0,
            heading_up: true,
            scale: 0.01,
            north_deg: 0.0,
            outline: false,
            full: false,
        };
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
