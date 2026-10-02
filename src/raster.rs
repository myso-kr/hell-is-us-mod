//! A few shapes drawn into a premultiplied-alpha pixel buffer — just what the
//! minimap needs, so its window can be a plain layered Win32 window that keeps
//! drawing while the panel (and eframe with it) is hidden.
//!
//! Pixels are `0xAARRGGBB`, premultiplied, top row first: what `UpdateLayeredWindow`
//! takes from a 32-bit top-down DIB. Edges are anti-aliased by coverage over one pixel.

use crate::actors::{Kind, Thing};
use crate::geometry::Footprint;
use crate::icons::Icons;
use crate::minimap::{MapState, View};

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

    /// Source-over, `c` at `coverage` (0..=1).
    fn blend(&mut self, x: i32, y: i32, c: Rgba, coverage: f32) {
        if x < 0 || y < 0 || x as usize >= self.w || y as usize >= self.h || coverage <= 0.0 {
            return;
        }
        let a = (c.3 as f32 / 255.0) * coverage.min(1.0);
        let i = y as usize * self.w + x as usize;
        let d = self.px[i];
        let ch = |shift: u32, s: u8| {
            let dst = ((d >> shift) & 0xFF) as f32;
            ((s as f32 * a + dst * (1.0 - a)).round() as u32).min(255) << shift
        };
        let da = ((d >> 24) & 0xFF) as f32;
        let out_a = ((255.0 * a + da * (1.0 - a)).round() as u32).min(255) << 24;
        self.px[i] = out_a | ch(16, c.0) | ch(8, c.1) | ch(0, c.2);
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

    pub fn disc(&mut self, cx: f32, cy: f32, r: f32, c: Rgba) {
        self.each(cx - r, cy - r, cx + r, cy + r, |s, x, y, px, py| {
            let d = ((px - cx).powi(2) + (py - cy).powi(2)).sqrt();
            s.blend(x, y, c, r - d + 0.5);
        });
    }

    pub fn ring(&mut self, cx: f32, cy: f32, r: f32, width: f32, c: Rgba) {
        let o = r + width;
        self.each(cx - o, cy - o, cx + o, cy + o, |s, x, y, px, py| {
            let d = ((px - cx).powi(2) + (py - cy).powi(2)).sqrt();
            s.blend(x, y, c, width / 2.0 - (d - r).abs() + 0.5);
        });
    }

    pub fn line(&mut self, a: (f32, f32), b: (f32, f32), width: f32, c: Rgba) {
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let len2 = (dx * dx + dy * dy).max(f32::EPSILON);
        let h = width / 2.0;
        self.each(a.0.min(b.0) - h, a.1.min(b.1) - h, a.0.max(b.0) + h, a.1.max(b.1) + h, |s, x, y, px, py| {
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
        let (x0, y0) = ((cx - size as f32 / 2.0).round() as i32, (cy - size as f32 / 2.0).round() as i32);
        for (i, &s) in px.iter().enumerate() {
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

    /// The letter N, `size` px tall, centred on (cx, cy) — the only text the map needs.
    pub fn letter_n(&mut self, cx: f32, cy: f32, size: f32, c: Rgba) {
        let (hw, hh) = (size * 0.32, size / 2.0);
        let (l, r, t, b) = (cx - hw, cx + hw, cy - hh, cy + hh);
        self.line((l, b), (l, t), 2.0, c);
        self.line((l, t), (r, b), 2.0, c);
        self.line((r, b), (r, t), 2.0, c);
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
    pub const ALL: [Band; 6] = [Band::Deep, Band::Lower, Band::Level, Band::Raised, Band::High, Band::Wall];

    /// (fill, edge).
    pub fn colours(self) -> (Rgba, Rgba) {
        match self {
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

/// One frame of the minimap: a disc of radius `r` px centred in the canvas.
/// With `icons`, things are drawn as icons; without, as coloured dots.
pub fn draw_map(
    cv: &mut Canvas,
    state: &MapState,
    world: &str,
    view: &View,
    things: &[Thing],
    icons: Option<&Icons>,
    footprints: &[Footprint],
) {
    cv.clear();
    let (cx, cy) = (cv.w as f32 / 2.0, cv.h as f32 / 2.0);
    let r = cx.min(cy) - 14.0;
    let inside = |p: (f32, f32)| p.0 * p.0 + p.1 * p.1 <= r * r;
    cv.disc(cx, cy, r, BACKGROUND);

    if state.terrain {
        // Feet are about 90 cm below the capsule's centre.
        let feet = view.center[2] - 90.0;
        let reach = r / view.scale.max(f32::EPSILON);
        let mut masks: Vec<Canvas> = Band::ALL.iter().map(|_| Canvas::new(cv.w, cv.h)).collect();
        for f in footprints {
            let c = f.center();
            let d = ((c[0] - view.center[0]).powi(2) + (c[1] - view.center[1]).powi(2)).sqrt();
            if d - f.reach() > reach {
                continue;
            }
            let Some(b) = band(f, feet) else { continue };
            let pts: Vec<(f32, f32)> = f
                .corners
                .iter()
                .map(|c| {
                    let (x, y) = view.project([c[0], c[1], 0.0]);
                    (cx + x, cy + y)
                })
                .collect();
            masks[Band::ALL.iter().position(|x| *x == b).unwrap()].polygon(&pts, Rgba(255, 255, 255, 255));
        }
        // Each band is one shape — the union of its footprints — laid over the map once,
        // with an edge where it ends. Overlaps do not pile up.
        let rim = (r - 1.0) * (r - 1.0);
        for (mask, band) in masks.iter().zip(Band::ALL) {
            let (fill, edge) = band.colours();
            let cover = |x: usize, y: usize| (mask.px[y * mask.w + x] >> 24) as f32 / 255.0;
            for y in 1..cv.h - 1 {
                for x in 1..cv.w - 1 {
                    let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
                    if dx * dx + dy * dy > rim {
                        continue;
                    }
                    let a = cover(x, y);
                    if a <= 0.0 {
                        continue;
                    }
                    let edged = a >= 0.5
                        && [(x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)].iter().any(|&(nx, ny)| cover(nx, ny) < 0.5);
                    if edged {
                        cv.blend(x as i32, y as i32, edge, 1.0);
                    } else {
                        cv.blend(x as i32, y as i32, fill, a);
                    }
                }
            }
        }
    }

    if let Some(trail) = state.trails.get(world) {
        for pair in trail.windows(2) {
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
                    cv.line((cx + pa.0, cy + pa.1), (cx + pb.0, cy + pb.1), 2.0, TRAIL);
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
        match icons {
            Some(icons) => {
                let i = icons.get(*k);
                cv.blit(cx + p.0, cy + p.1, i.size, &i.px);
            }
            None => {
                let size = if *k == Kind::Enemy { 4.5 } else { 3.5 };
                cv.disc(cx + p.0, cy + p.1, size + 1.2, OUTLINE);
                cv.disc(cx + p.0, cy + p.1, size, colour(*k));
            }
        }
    }

    cv.ring(cx, cy, r, 2.0, EDGE);
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
        let v = View { center: [0.0, 0.0, 100.0], yaw_deg: 0.0, heading_up: false, scale: 0.05 };
        // A wall 10 m north of the hero, standing on their floor; a ceiling slab over them.
        let wall =
            crate::geometry::footprint([1000.0, 0.0, 100.0], 0.0, [1.0; 3], [0.0; 3], [100.0, 300.0, 150.0]).unwrap();
        let ceiling =
            crate::geometry::footprint([0.0, 0.0, 900.0], 0.0, [1.0; 3], [0.0; 3], [400.0, 400.0, 20.0]).unwrap();
        let mut cv = Canvas::new(200, 200);
        draw_map(&mut cv, &s, "W", &v, &[], None, &[wall, ceiling]);
        // The wall at 10 m north, 0.05 px/cm: 50 px above the centre.
        assert!((cv.px[50 * 200 + 100] >> 16) & 0xFF > 60, "wall drawn");
        // Beside the hero (10 px left), where only the ceiling would be.
        let bg = cv.px[100 * 200 + 85];
        let mut plain = Canvas::new(200, 200);
        draw_map(&mut plain, &s, "W", &v, &[], None, &[]);
        assert_eq!(bg, plain.px[100 * 200 + 85], "ceiling not drawn");
    }

    #[test]
    fn icons_blit_opaque_over_the_map() {
        let icons = Icons::new(16).unwrap();
        let mut cv = Canvas::new(200, 200);
        let s = MapState::default();
        let v = View { center: [0.0; 3], yaw_deg: 0.0, heading_up: true, scale: 0.01 };
        draw_map(
            &mut cv,
            &s,
            "W",
            &v,
            &[Thing { sub: crate::actors::Sub::Medicine, at: [3000.0, 0.0, 0.0] }],
            Some(&icons),
            &[],
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
        let v = View { center: [0.0; 3], yaw_deg: 0.0, heading_up: true, scale: 0.01 };
        draw_map(&mut cv, &s, "W", &v, &[Thing { sub: crate::actors::Sub::Feral, at: [3000.0, 0.0, 0.0] }], None, &[]);
        assert_eq!(cv.px[100 * 200 + 100] >> 24, 255, "hero arrow");
        // 30 m ahead at 0.01 px/cm is 30 px above the centre, in enemy red.
        assert_eq!((cv.px[70 * 200 + 100] >> 16) & 0xFF, 235);
        s.layers = 0;
        draw_map(&mut cv, &s, "W", &v, &[Thing { sub: crate::actors::Sub::Feral, at: [3000.0, 0.0, 0.0] }], None, &[]);
        assert_ne!((cv.px[70 * 200 + 100] >> 16) & 0xFF, 235, "layer off, not drawn");
        assert_eq!(cv.px[0], 0, "outside the disc stays clear");
    }
}
