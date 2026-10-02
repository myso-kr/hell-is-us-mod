//! The ground's height, from the landscape's physics heightfields (.spec/GUIDE.md §8).
//!
//! Landscape heights are not reflected, but Chaos keeps each collision component's
//! heightfield on the CPU for the physics: `LandscapeHeightfieldCollisionComponent`
//! → `HeightfieldRef` (the native member right after the reflected
//! `CookedPhysicalMaterials`) → `FHeightfieldGeometryRef::HeightfieldGeometry` (+0x30)
//! → `Chaos::FHeightField::GeomData`. A vertex's height, in the component's space, is
//! `(MinValue + Heights[row·cols + col] · HeightPerUnit) · Scale.z`, rows along Y —
//! checked on build 24045435 against 1,345 grass and foliage instances (median 28 cm
//! above, most within 30 cm).

use crate::mem::{self, Memory};
use crate::names::Names;
use std::collections::HashMap;

/// FHeightfieldGeometryRef::HeightfieldGeometry.
const GEOMETRY: u64 = 0x30;
/// Chaos::FHeightField::GeomData members.
const HEIGHTS: usize = 0x20;
const SCALE: usize = 0x50;
const MIN_VALUE: usize = 0x80;
const MAX_VALUE: usize = 0x88;
const ROWS: usize = 0x90;
const HEIGHT_PER_UNIT: usize = 0xA0;

/// One component's heights: a square of `n`×`n` vertices, `spacing` apart, from
/// `origin` (world, cm).
#[derive(Clone, Debug, PartialEq)]
pub struct Heightfield {
    pub origin: [f64; 3],
    pub spacing: [f64; 2],
    pub n: usize,
    /// World heights (cm), row-major, rows along Y.
    pub z: Vec<f32>,
}

/// One field's steepness at each vertex (rise per run), from its neighbours.
fn steepness(f: &Heightfield) -> Vec<f32> {
    let n = f.n;
    let at = |r: usize, c: usize| f.z[r * n + c];
    (0..n * n)
        .map(|i| {
            let (r, c) = (i / n, i % n);
            let (c0, c1) = (c.saturating_sub(1), (c + 1).min(n - 1));
            let (r0, r1) = (r.saturating_sub(1), (r + 1).min(n - 1));
            let dx = (at(r, c1) - at(r, c0)) / ((c1 - c0).max(1) as f64 * f.spacing[0]) as f32;
            let dy = (at(r1, c) - at(r0, c)) / ((r1 - r0).max(1) as f64 * f.spacing[1]) as f32;
            (dx * dx + dy * dy).sqrt()
        })
        .collect()
}

impl Heightfield {
    fn span(&self) -> [f64; 2] {
        [(self.n - 1) as f64 * self.spacing[0], (self.n - 1) as f64 * self.spacing[1]]
    }

    /// The bilinear height at a world point inside the field.
    fn at(&self, x: f64, y: f64) -> Option<f32> {
        let (c, r) = ((x - self.origin[0]) / self.spacing[0], (y - self.origin[1]) / self.spacing[1]);
        let last = (self.n - 1) as f64;
        if !(0.0..=last).contains(&c) || !(0.0..=last).contains(&r) {
            return None;
        }
        let (c0, r0) = ((c as usize).min(self.n - 2), (r as usize).min(self.n - 2));
        let (fc, fr) = ((c - c0 as f64) as f32, (r - r0 as f64) as f32);
        let v = |r: usize, c: usize| self.z[r * self.n + c];
        let top = v(r0, c0) * (1.0 - fc) + v(r0, c0 + 1) * fc;
        let bot = v(r0 + 1, c0) * (1.0 - fc) + v(r0 + 1, c0 + 1) * fc;
        Some(top * (1.0 - fr) + bot * fr)
    }
}

/// All the loaded landscape, found by the square each component covers.
#[derive(Clone, Debug, Default)]
pub struct Terrain {
    fields: Vec<Heightfield>,
    /// Each field's steepness per vertex.
    steep: Vec<Vec<f32>>,
    /// Components by their square: ((x − ox) / span, (y − oy) / span).
    by_square: HashMap<(i64, i64), usize>,
    origin: [f64; 2],
    span: [f64; 2],
}

impl Terrain {
    pub fn new(fields: Vec<Heightfield>) -> Terrain {
        let Some(first) = fields.first() else { return Terrain::default() };
        let (origin, span) = ([first.origin[0], first.origin[1]], first.span());
        let mut by_square = HashMap::new();
        for (i, f) in fields.iter().enumerate() {
            let key = (
                ((f.origin[0] - origin[0]) / span[0]).round() as i64,
                ((f.origin[1] - origin[1]) / span[1]).round() as i64,
            );
            by_square.insert(key, i);
        }
        let steep = fields.iter().map(steepness).collect();
        Terrain { fields, steep, by_square, origin, span }
    }

    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }

    pub fn len(&self) -> usize {
        self.fields.len()
    }

    fn field(&self, x: f64, y: f64) -> Option<usize> {
        if self.fields.is_empty() {
            return None;
        }
        let key = (
            ((x - self.origin[0]) / self.span[0]).floor() as i64,
            ((y - self.origin[1]) / self.span[1]).floor() as i64,
        );
        self.by_square.get(&key).copied()
    }

    /// The ground's height at a world point (cm), where the landscape is loaded.
    pub fn height(&self, x: f32, y: f32) -> Option<f32> {
        let (x, y) = (x as f64, y as f64);
        self.field(x, y).and_then(|i| self.fields[i].at(x, y))
    }

    /// How steep the ground is at a point — rise per unit run, at the nearest vertex.
    pub fn slope(&self, x: f32, y: f32) -> Option<f32> {
        let (x, y) = (x as f64, y as f64);
        let i = self.field(x, y)?;
        let f = &self.fields[i];
        let c = ((x - f.origin[0]) / f.spacing[0]).round();
        let r = ((y - f.origin[1]) / f.spacing[1]).round();
        let last = (f.n - 1) as f64;
        ((0.0..=last).contains(&c) && (0.0..=last).contains(&r)).then(|| self.steep[i][r as usize * f.n + c as usize])
    }
}

/// Where in a heightfield collision component class the heightfield reference is:
/// right after the reflected `CookedPhysicalMaterials` array.
pub fn reference_offset(n: &Names, m: &dyn Memory, comp: u64) -> Option<u64> {
    n.field(m, comp, "CookedPhysicalMaterials").map(|p| p.offset as u64 + 16)
}

/// One component's heightfield, placed by the component's world transform (translation
/// only: landscapes are not rotated, and the heightfield's scale already holds the
/// component's).
pub fn read(m: &dyn Memory, comp: u64, reference: u64, translation: [f64; 3]) -> Option<Heightfield> {
    let r = mem::read_u64(m, comp + reference).filter(|&p| mem::plausible(p))?;
    let geo = mem::read_u64(m, r + GEOMETRY).filter(|&p| mem::plausible(p))?;
    let mut b = [0u8; 0xA8];
    m.read(geo, &mut b).then_some(())?;
    let d = |o: usize| f64::from_le_bytes(b[o..o + 8].try_into().unwrap());
    let rows = u16::from_le_bytes([b[ROWS], b[ROWS + 1]]) as usize;
    let cols = u16::from_le_bytes([b[ROWS + 2], b[ROWS + 3]]) as usize;
    let data = u64::from_le_bytes(b[HEIGHTS..HEIGHTS + 8].try_into().unwrap());
    let num = u32::from_le_bytes(b[HEIGHTS + 8..HEIGHTS + 12].try_into().unwrap()) as usize;
    let (min, max, hpu) = (d(MIN_VALUE), d(MAX_VALUE), d(HEIGHT_PER_UNIT));
    let scale = [d(SCALE), d(SCALE + 8), d(SCALE + 16)];
    // The header must hang together: square, as many heights as vertices, and the
    // height step spanning min..max over the u16 range.
    let sane = rows == cols
        && (2..=1025).contains(&rows)
        && num == rows * cols
        && mem::plausible(data)
        && scale.iter().all(|s| s.is_finite() && *s > 0.0)
        && max >= min
        && (hpu * 65535.0 - (max - min)).abs() <= 1e-3 * (max - min).abs().max(1.0);
    sane.then_some(())?;
    let mut hb = vec![0u8; num * 2];
    m.read(data, &mut hb).then_some(())?;
    let z = hb
        .chunks_exact(2)
        .map(|c| ((min + u16::from_le_bytes([c[0], c[1]]) as f64 * hpu) * scale[2] + translation[2]) as f32)
        .collect();
    Some(Heightfield { origin: translation, spacing: [scale[0], scale[1]], n: rows, z })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 3×3-vertex field 100 cm apart at `origin`, rising 10 cm per column.
    fn ramp(origin: [f64; 3]) -> Heightfield {
        let z = (0..9).map(|i| origin[2] as f32 + (i % 3) as f32 * 10.0).collect();
        Heightfield { origin, spacing: [100.0, 100.0], n: 3, z }
    }

    #[test]
    fn heights_are_bilinear_and_found_by_square() {
        let t = Terrain::new(vec![ramp([0.0, 0.0, 0.0]), ramp([200.0, 0.0, 50.0])]);
        assert_eq!(t.height(50.0, 50.0), Some(5.0));
        assert_eq!(t.height(150.0, 199.0), Some(15.0));
        assert_eq!(t.height(250.0, 10.0), Some(55.0), "the second component");
        assert_eq!(t.height(-1.0, 10.0), None);
        assert_eq!(t.height(10.0, 450.0), None);
    }

    #[test]
    fn slope_is_rise_over_run() {
        let t = Terrain::new(vec![ramp([0.0, 0.0, 0.0])]);
        let s = t.slope(100.0, 100.0).unwrap();
        assert!((s - 0.1).abs() < 1e-4, "{s}");
    }
}
