//! The lie of the land for the map: the landscape's heights around the hero, baked
//! into a square of texels with their shading and water, so a frame only looks them
//! up (.spec/GUIDE.md §9).
//!
//! Baking reads the heightfields (terrain.rs) once per texel — too slow for every
//! frame, quick enough off the overlay thread whenever the hero has moved far or the
//! scene has changed. Drawing then looks up each pixel's height (bilinear), so
//! contour lines stay one pixel wide at any zoom.

use crate::obstacles::Scene;
use crate::raster::Rgba;

/// Texels a side, at most; texels grow beyond that instead.
pub const MAX_SIDE: usize = 1024;
/// Texels are at least this big (cm): the heightfields' own spacing.
const MIN_RES: f32 = 100.0;
/// The light comes from the north-west, 45° up. North is −Y and west +X (Hell Is Us).
const LIGHT: [f32; 3] = [0.5, -0.5, 0.707];
/// How far above or below the hero the height tint is at its strongest (cm).
const TINT_SPAN: f32 = 1500.0;
/// The tint's colours: level with the hero, below, above; and water.
const MID: (f32, f32, f32) = (72.0, 82.0, 70.0);
const LOW: (f32, f32, f32) = (48.0, 70.0, 98.0);
const HIGH: (f32, f32, f32) = (128.0, 106.0, 72.0);
const WATER: Rgba = Rgba(40, 95, 175, 170);
/// Bake again when the hero's feet are this far from the height the tint was for (cm).
pub const TINT_MOVED: f32 = 300.0;

#[derive(Clone, Debug, Default)]
pub struct Relief {
    pub origin: [f32; 2],
    /// Texel size (cm).
    pub res: f32,
    pub n: usize,
    /// Ground height per texel (cm); NaN where the landscape is not loaded.
    pub z: Vec<f32>,
    /// How lit the ground is, against flat ground (−1 = facing away, 0 = flat).
    pub shade: Vec<f32>,
    /// Deadly water.
    pub wet: Vec<bool>,
    /// The shaded, tinted colour per texel (transparent where unknown), and the feet
    /// height it was tinted against (cm).
    pub colour: Vec<Rgba>,
    pub feet: f32,
}

/// One texel's colour: water, or the ground shaded and tinted by its height against
/// the hero's feet.
fn colour(z: f32, lit: f32, wet: bool, feet: f32) -> Rgba {
    if wet {
        return WATER;
    }
    if z.is_nan() {
        return Rgba(0, 0, 0, 0);
    }
    let lerp = |a: (f32, f32, f32), b: (f32, f32, f32), t: f32| {
        (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t, a.2 + (b.2 - a.2) * t)
    };
    let t = ((z - feet) / TINT_SPAN).clamp(-1.0, 1.0);
    let base = if t < 0.0 { lerp(MID, LOW, -t) } else { lerp(MID, HIGH, t) };
    let k = (1.0 + lit * 2.2).clamp(0.35, 1.8);
    let c = |v: f32| (v * k).clamp(0.0, 255.0) as u8;
    Rgba(c(base.0), c(base.1), c(base.2), 150)
}

impl Relief {
    /// The square `half` (cm) each way around `center`, tinted against `feet` (cm).
    pub fn bake(scene: &Scene, center: [f32; 2], half: f32, feet: f32) -> Relief {
        let res = (2.0 * half / MAX_SIDE as f32).max(MIN_RES);
        let n = ((2.0 * half / res).ceil() as usize).clamp(2, MAX_SIDE);
        let origin = [center[0] - half, center[1] - half];
        let mid = |i: usize| (i as f32 + 0.5) * res;
        let mut z = vec![f32::NAN; n * n];
        for y in 0..n {
            for x in 0..n {
                if let Some(h) = scene.terrain.height(origin[0] + mid(x), origin[1] + mid(y)) {
                    z[y * n + x] = h;
                }
            }
        }
        let flat = LIGHT[2] / (LIGHT[0] * LIGHT[0] + LIGHT[1] * LIGHT[1] + LIGHT[2] * LIGHT[2]).sqrt();
        let mut shade = vec![0.0; n * n];
        for y in 0..n {
            for x in 0..n {
                let at = |x: usize, y: usize| z[y * n + x];
                let (x0, x1) = (x.saturating_sub(1), (x + 1).min(n - 1));
                let (y0, y1) = (y.saturating_sub(1), (y + 1).min(n - 1));
                let dx = (at(x1, y) - at(x0, y)) / ((x1 - x0).max(1) as f32 * res);
                let dy = (at(x, y1) - at(x, y0)) / ((y1 - y0).max(1) as f32 * res);
                if dx.is_nan() || dy.is_nan() {
                    continue;
                }
                let len = (dx * dx + dy * dy + 1.0).sqrt();
                let lit = (-dx * LIGHT[0] - dy * LIGHT[1] + LIGHT[2]) / len;
                let norm = (LIGHT[0] * LIGHT[0] + LIGHT[1] * LIGHT[1] + LIGHT[2] * LIGHT[2]).sqrt();
                shade[y * n + x] = lit / norm - flat;
            }
        }
        let mut wet = vec![false; n * n];
        for o in scene.obstacles.iter().filter(|o| o.water) {
            let lo = [
                o.hull.iter().map(|p| p[0]).fold(f32::MAX, f32::min),
                o.hull.iter().map(|p| p[1]).fold(f32::MAX, f32::min),
            ];
            let hi = [
                o.hull.iter().map(|p| p[0]).fold(f32::MIN, f32::max),
                o.hull.iter().map(|p| p[1]).fold(f32::MIN, f32::max),
            ];
            let cell = |v: f32, o: f32| ((v - o) / res - 0.5).ceil();
            let (x0, x1) = (cell(lo[0], origin[0]).max(0.0) as usize, cell(hi[0], origin[0]).min(n as f32) as usize);
            let (y0, y1) = (cell(lo[1], origin[1]).max(0.0) as usize, cell(hi[1], origin[1]).min(n as f32) as usize);
            for y in y0..y1 {
                for x in x0..x1 {
                    wet[y * n + x] = true;
                }
            }
        }
        let colour = (0..n * n).map(|i| colour(z[i], shade[i], wet[i], feet)).collect();
        Relief { origin, res, n, z, shade, wet, colour, feet }
    }

    /// The square's centre and half-width (cm).
    pub fn extent(&self) -> ([f32; 2], f32) {
        let half = self.n as f32 * self.res / 2.0;
        ([self.origin[0] + half, self.origin[1] + half], half)
    }

    /// Where a world point falls, in texels from the first texel's centre.
    fn texel(&self, x: f32, y: f32) -> Option<(f32, f32)> {
        let (tx, ty) = ((x - self.origin[0]) / self.res - 0.5, (y - self.origin[1]) / self.res - 0.5);
        let last = (self.n - 1) as f32;
        ((0.0..=last).contains(&tx) && (0.0..=last).contains(&ty)).then_some((tx, ty))
    }

    /// The ground's height at a world point (bilinear), where it is known.
    pub fn height(&self, x: f32, y: f32) -> Option<f32> {
        let (tx, ty) = self.texel(x, y)?;
        let (x0, y0) = ((tx as usize).min(self.n - 2), (ty as usize).min(self.n - 2));
        let (fx, fy) = (tx - x0 as f32, ty - y0 as f32);
        let v = |x: usize, y: usize| self.z[y * self.n + x];
        let top = v(x0, y0) * (1.0 - fx) + v(x0 + 1, y0) * fx;
        let bot = v(x0, y0 + 1) * (1.0 - fx) + v(x0 + 1, y0 + 1) * fx;
        let h = top * (1.0 - fy) + bot * fy;
        (!h.is_nan()).then_some(h)
    }

    /// Shading and water at the nearest texel.
    pub fn look(&self, x: f32, y: f32) -> Option<(f32, bool)> {
        let (tx, ty) = self.texel(x, y)?;
        let i = ty.round() as usize * self.n + tx.round() as usize;
        Some((self.shade[i], self.wet[i]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::obstacles::Obstacle;
    use crate::terrain::{Heightfield, Terrain};

    /// 100 m of ground rising 1 m per metre eastward (−X is east, so rising toward −X),
    /// with water over its first 10 m along X.
    fn scene() -> Scene {
        let n = 101;
        let z = (0..n * n).map(|i| (n - 1 - i % n) as f32 * 100.0).collect();
        let field = Heightfield { origin: [0.0, 0.0, 0.0], spacing: [100.0, 100.0], n, z };
        let water = Obstacle {
            hull: vec![[0.0, 0.0], [1000.0, 0.0], [1000.0, 10_000.0], [0.0, 10_000.0]],
            zmin: f32::MIN,
            zmax: f32::MAX,
            water: true,
        };
        Scene { obstacles: vec![water], terrain: Terrain::new(vec![field]) }
    }

    #[test]
    fn heights_follow_the_ground() {
        let r = Relief::bake(&scene(), [5000.0, 5000.0], 3000.0, 0.0);
        assert_eq!(r.res, 100.0);
        let h = r.height(5000.0, 5000.0).unwrap();
        assert!((h - 5000.0).abs() < 1.0, "{h}");
        assert_eq!(r.height(-100.0, 5000.0), None);
        assert_eq!(r.extent().0, [5000.0, 5000.0]);
    }

    #[test]
    fn a_slope_facing_the_light_is_lit_and_water_is_marked() {
        // The ground falls toward +X, which is west: toward the light. Lit.
        let r = Relief::bake(&scene(), [5000.0, 5000.0], 4900.0, 0.0);
        let (lit, wet) = r.look(5000.0, 5000.0).unwrap();
        assert!(lit > 0.1, "{lit}");
        assert!(!wet);
        assert!(r.look(500.0, 5000.0).unwrap().1, "water");
    }

    #[test]
    fn big_squares_grow_their_texels() {
        let r = Relief::bake(&scene(), [0.0, 0.0], 200_000.0, 0.0);
        assert_eq!(r.n, MAX_SIDE);
        assert!(r.res > 300.0);
    }
}
