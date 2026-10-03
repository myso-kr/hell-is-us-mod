//! The minimap's background: the level's static meshes seen from above.
//!
//! The game ships no map of the area being explored (.spec/MAP.md), so the map is
//! drawn from the world itself. Every loaded static mesh actor — floor tiles, walls,
//! pillars, rocks — has a root `StaticMeshComponent` with `RelativeLocation`,
//! `RelativeRotation` and `RelativeScale3D`, and a `StaticMesh` whose
//! `ExtendedBounds` is its box in its own space. Turned by the yaw and scaled, that
//! box is the mesh's footprint: a quadrilateral on the ground, with a height range.
//!
//! Footprints do not move, so each actor's is read once and kept; a scan every few
//! seconds only adds actors streamed in and drops those streamed out.

use crate::mem::{self, Memory};
use crate::names::{Names, CLASS, OUTER};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// A mesh seen from above: four corners (cm, world X/Y) and how far up it reaches.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Footprint {
    pub corners: [[f32; 2]; 4],
    pub zmin: f32,
    pub zmax: f32,
}

impl Footprint {
    pub fn height(&self) -> f32 {
        self.zmax - self.zmin
    }

    pub fn center(&self) -> [f32; 2] {
        let c = self.corners;
        [(c[0][0] + c[2][0]) / 2.0, (c[0][1] + c[2][1]) / 2.0]
    }

    /// Its longer side.
    pub fn size(&self) -> f32 {
        let c = self.corners;
        let side = |a: [f32; 2], b: [f32; 2]| ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt();
        side(c[0], c[1]).max(side(c[1], c[2]))
    }

    /// Half the diagonal: how far from its centre any part of it can be.
    pub fn reach(&self) -> f32 {
        let c = self.corners;
        ((c[0][0] - c[2][0]).powi(2) + (c[0][1] - c[2][1]).powi(2)).sqrt() / 2.0
    }
}

/// Footprints smaller than this across (cm) are clutter, larger ones are sky boxes,
/// terrain blockers and the like.
const MIN_SIZE: f32 = 30.0;
const MAX_SIZE: f32 = 15_000.0;

/// The box of a mesh in its own space, scaled, turned by `yaw` degrees and moved to
/// `loc`. `None` for anything not finite or outside the size limits.
pub fn footprint(loc: [f64; 3], yaw: f64, scale: [f64; 3], origin: [f64; 3], extent: [f64; 3]) -> Option<Footprint> {
    let all = loc.iter().chain(&scale).chain(&origin).chain(&extent).chain([&yaw]);
    if !all.into_iter().all(|v| v.is_finite()) {
        return None;
    }
    let (s, c) = yaw.to_radians().sin_cos();
    let (cx, cy) = (origin[0] * scale[0], origin[1] * scale[1]);
    let (ex, ey) = (extent[0] * scale[0].abs(), extent[1] * scale[1].abs());
    let size = 2.0 * ex.max(ey) as f32;
    if !(MIN_SIZE..=MAX_SIZE).contains(&size) {
        return None;
    }
    let corner = |sx: f64, sy: f64| {
        let (x, y) = (cx + sx * ex, cy + sy * ey);
        [(loc[0] + x * c - y * s) as f32, (loc[1] + x * s + y * c) as f32]
    };
    let (oz, ez) = (origin[2] * scale[2], extent[2] * scale[2].abs());
    Some(Footprint {
        corners: [corner(-1.0, -1.0), corner(1.0, -1.0), corner(1.0, 1.0), corner(-1.0, 1.0)],
        zmin: (loc[2] + oz - ez) as f32,
        zmax: (loc[2] + oz + ez) as f32,
    })
}

/// Where a static mesh component keeps what the footprint needs, by its class.
#[derive(Clone, Copy)]
struct Fields {
    rotation: u64,
    scale: u64,
    mesh: u64,
    parent: u64,
}

const RESCAN: Duration = Duration::from_secs(3);
const MAX_LEVELS: u32 = 4096;
const MAX_ACTORS: u32 = 500_000;

#[derive(Default)]
pub struct Geometry {
    levels: Option<u64>,
    fields: HashMap<u64, Option<Fields>>,
    /// The mesh's `ExtendedBounds` offset, once known (every UStaticMesh shares it).
    bounds: Option<u64>,
    /// Per actor: its class and its footprint (None: not drawn).
    seen: HashMap<u64, (u64, Option<Footprint>)>,
    /// What the map draws; replaced, never changed, so a snapshot can share it.
    pub footprints: Arc<Vec<Footprint>>,
    scanned: Option<Instant>,
}

fn f64s<const N: usize>(m: &dyn Memory, at: u64) -> Option<[f64; N]> {
    let mut b = vec![0u8; N * 8];
    m.read(at, &mut b).then(|| std::array::from_fn(|i| f64::from_le_bytes(b[i * 8..i * 8 + 8].try_into().unwrap())))
}

impl Geometry {
    fn fields(&mut self, m: &dyn Memory, n: &Names, comp: u64, class: u64) -> Option<Fields> {
        *self.fields.entry(class).or_insert_with(|| {
            let f = |name: &str| n.field(m, comp, name).map(|p| p.offset as u64);
            Some(Fields {
                rotation: f("RelativeRotation")?,
                scale: f("RelativeScale3D")?,
                mesh: f("StaticMesh")?,
                parent: f("AttachParent")?,
            })
        })
    }

    fn read(&mut self, m: &dyn Memory, n: &Names, actor: u64, root: u64, location: u64) -> Option<Footprint> {
        let comp = mem::read_u64(m, actor + root).filter(|&p| mem::plausible(p))?;
        let class = mem::read_u64(m, comp + CLASS).filter(|&p| mem::plausible(p))?;
        let f = self.fields(m, n, comp, class)?;
        // Attached components keep a location relative to their parent, not the world.
        if mem::read_u64(m, comp + f.parent).is_some_and(|p| p != 0) {
            return None;
        }
        let mesh = mem::read_u64(m, comp + f.mesh).filter(|&p| mem::plausible(p))?;
        if self.bounds.is_none() {
            self.bounds = Some(n.field(m, mesh, "ExtendedBounds").filter(|p| p.size == 56)?.offset as u64);
        }
        let b: [f64; 6] = f64s(m, mesh + self.bounds?)?;
        let loc: [f64; 3] = f64s(m, comp + location)?;
        let rot: [f64; 3] = f64s(m, comp + f.rotation)?;
        let scale: [f64; 3] = f64s(m, comp + f.scale)?;
        footprint(loc, rot[1], scale, [b[0], b[1], b[2]], [b[3], b[4], b[5]])
    }

    /// Bring the footprints up to date with the loaded levels, when a scan is due.
    /// `root` and `location` are the hero chain's RootComponent and RelativeLocation
    /// offsets; `actors` the level's actor array offset (actors.rs).
    pub fn refresh(&mut self, m: &dyn Memory, n: &Names, hero: u64, root: u64, location: u64, actors: u64) {
        if self.scanned.is_some_and(|t| t.elapsed() < RESCAN) {
            return;
        }
        self.scanned = Some(Instant::now());
        let Some(world) = mem::read_u64(m, hero + OUTER)
            .filter(|&p| mem::plausible(p))
            .and_then(|level| mem::read_u64(m, level + OUTER))
            .filter(|&p| mem::plausible(p))
        else {
            return;
        };
        if self.levels.is_none() {
            self.levels = n.field(m, world, "Levels").map(|p| p.offset as u64);
        }
        let Some(levels) = self.levels else { return };

        let mut now: HashMap<u64, (u64, Option<Footprint>)> = HashMap::with_capacity(self.seen.len());
        for lv in crate::actors::array(m, world + levels, MAX_LEVELS) {
            for actor in crate::actors::array(m, lv + actors, MAX_ACTORS) {
                let Some(class) = mem::read_u64(m, actor + CLASS).filter(|&c| mem::plausible(c)) else { continue };
                let entry = match self.seen.get(&actor) {
                    Some(&(c, fp)) if c == class => (c, fp),
                    _ => (class, self.read(m, n, actor, root, location)),
                };
                now.insert(actor, entry);
            }
        }
        let changed = now.len() != self.seen.len() || now.keys().any(|k| !self.seen.contains_key(k));
        self.seen = now;
        if changed {
            self.footprints = Arc::new(self.seen.values().filter_map(|(_, fp)| *fp).collect());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: [f32; 2], b: [f32; 2]) -> bool {
        (a[0] - b[0]).abs() < 0.01 && (a[1] - b[1]).abs() < 0.01
    }

    #[test]
    fn a_box_turned_and_scaled_lands_where_it_should() {
        // A 4 m × 2 m slab centred on its origin, doubled in X, turned 90°, at (1000, 0).
        let f = footprint([1000.0, 0.0, 50.0], 90.0, [2.0, 1.0, 1.0], [0.0; 3], [200.0, 100.0, 10.0]).unwrap();
        // X now points along +Y: the long side (800) runs north–south... along Y.
        assert!(close(f.corners[0], [1100.0, -400.0]), "{:?}", f.corners);
        assert!(close(f.corners[2], [900.0, 400.0]), "{:?}", f.corners);
        assert_eq!((f.zmin, f.zmax), (40.0, 60.0));
        assert!(close(f.center(), [1000.0, 0.0]));
        assert!((f.reach() - (400f32 * 400.0 + 100.0 * 100.0).sqrt()).abs() < 0.1);
    }

    #[test]
    fn an_offset_mesh_origin_moves_with_the_turn() {
        let f = footprint([0.0; 3], 90.0, [1.0; 3], [100.0, 0.0, 0.0], [50.0, 50.0, 50.0]).unwrap();
        assert!(close(f.center(), [0.0, 100.0]), "{:?}", f.center());
    }

    #[test]
    fn clutter_and_sky_boxes_are_left_out() {
        assert!(footprint([0.0; 3], 0.0, [1.0; 3], [0.0; 3], [5.0, 5.0, 5.0]).is_none());
        assert!(footprint([0.0; 3], 0.0, [1.0; 3], [0.0; 3], [20_000.0, 10.0, 10.0]).is_none());
        assert!(footprint([f64::NAN, 0.0, 0.0], 0.0, [1.0; 3], [0.0; 3], [100.0; 3]).is_none());
    }
}
