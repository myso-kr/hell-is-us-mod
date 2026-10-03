//! A few shapes drawn into a premultiplied-alpha pixel buffer — what the minimap,
//! the compass and the tracker draw into, so their windows can be plain layered Win32
//! windows that keep drawing while the panel (and eframe with it) is hidden.
//!
//! Pixels are `0xAARRGGBB`, premultiplied, top row first: what `UpdateLayeredWindow`
//! takes from a 32-bit top-down DIB. Edges are anti-aliased by coverage over one pixel.

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

/// Integer source-over of `c` at alpha `a` (0..=255) onto the pixel `d`: each
/// channel (s·a + d·(255 − a)) / 255.
#[inline]
pub(crate) fn over(d: u32, c: Rgba, a: u32) -> u32 {
    let keep = 255 - a;
    let mix = |shift: u32, s: u32| {
        let v = s * a + ((d >> shift) & 0xFF) * keep + 127;
        (((v + (v >> 8)) >> 8).min(255)) << shift
    };
    mix(24, 255) | mix(16, c.0 as u32) | mix(8, c.1 as u32) | mix(0, c.2 as u32)
}
