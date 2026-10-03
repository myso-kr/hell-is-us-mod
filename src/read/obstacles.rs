//! What actually stands in the hero's way: the collision shapes of every static mesh
//! — plain, instanced, hierarchical, foliage — in world space, seen from above
//! (.spec/GUIDE.md §7).
//!
//! The game keeps no ground navmesh in memory here (§7), so the route finder needs
//! the obstacles themselves. Render bounds will not do — a tree's box is its canopy —
//! so each mesh's `BodySetup.AggGeom` is read instead: boxes, spheres, capsules and
//! convex hulls, as points in the mesh's own space (cached per mesh). Each placement
//! of the mesh moves those points into the world — an instance's matrix, then the
//! component's `ComponentToWorld` — and their 2D hull with a height range is the
//! obstacle. Meshes whose collision is complex-as-simple fall back to their bounds.
//! Components with no collision, or that only overlap, are left out.
//!
//! The landscape's heightfields come along in the same pass (terrain.rs).
//!
//! Water is not collision: the hero is stopped by the game's "deadly water" trigger
//! boxes instead (owners named `DeadlyWater…`), which sit at the lake bed and reach up
//! to just under the surface. A box spans land as well — its top is below the shore —
//! so only where the ground lies below the box's top is water.
//!
//! Collecting walks GUObjectArray a slice at a time, so no step stalls; a full pass
//! replaces the set.

use crate::gobjects::Objects;
use crate::mem::{self, Memory};
use crate::names::{Names, CLASS};
use crate::terrain::{self, Heightfield, Terrain};
use std::collections::HashMap;
use std::sync::Arc;

/// USceneComponent::ComponentToWorld — not reflected; found on build 24045435 as the
/// one FTransform in the hero's root component whose translation is its
/// RelativeLocation (.spec/GUIDE.md §7).
pub const COMPONENT_TO_WORLD: u64 = 0x1D0;

/// Objects looked at per step.
const SLICE: usize = 12_000;
/// Only components within this of the hero (cm) are read.
const REACH: f64 = 30_000.0;

#[derive(Clone, Debug, PartialEq)]
pub struct Obstacle {
    /// Convex, world X/Y (cm).
    pub hull: Vec<[f32; 2]>,
    pub zmin: f32,
    pub zmax: f32,
    /// Deadly water: blocks at any height.
    pub water: bool,
}

/// Water is found in cells this big (cm).
const SAMPLE: f32 = 200.0;
/// A deck over water: its top from this far below the surface to this far above (cm),
/// thinner than this (cm), at least this big from above (cm²).
const DECK: (f32, f32, f32, f32) = (50.0, 300.0, 200.0, 20_000.0);

/// What a pass found: what stands in the way, and the ground.
#[derive(Clone, Debug, Default)]
pub struct Scene {
    pub obstacles: Vec<Obstacle>,
    pub terrain: Terrain,
}

/// An FTransform: rotation quaternion (x, y, z, w), translation, scale.
#[derive(Clone, Copy, Debug)]
pub struct Transform {
    pub q: [f64; 4],
    pub t: [f64; 3],
    pub s: [f64; 3],
}

impl Transform {
    pub fn apply(&self, p: [f64; 3]) -> [f64; 3] {
        let v = [p[0] * self.s[0], p[1] * self.s[1], p[2] * self.s[2]];
        let r = rotate(self.q, v);
        [r[0] + self.t[0], r[1] + self.t[1], r[2] + self.t[2]]
    }

    pub fn read_at(m: &dyn Memory, at: u64) -> Option<Transform> {
        let mut b = [0u8; 0x60];
        m.read(at, &mut b).then_some(())?;
        let d = |o: usize| f64::from_le_bytes(b[o..o + 8].try_into().unwrap());
        let t = Transform { q: [d(0), d(8), d(16), d(24)], t: [d(32), d(40), d(48)], s: [d(64), d(72), d(80)] };
        let norm: f64 = t.q.iter().map(|v| v * v).sum();
        ((norm - 1.0).abs() < 1e-2 && t.t.iter().chain(&t.s).all(|v| v.is_finite())).then_some(t)
    }
}

/// v rotated by unit quaternion q.
pub fn rotate(q: [f64; 4], v: [f64; 3]) -> [f64; 3] {
    let (x, y, z, w) = (q[0], q[1], q[2], q[3]);
    // t = 2 · (q.xyz × v); v' = v + w·t + q.xyz × t
    let t = [2.0 * (y * v[2] - z * v[1]), 2.0 * (z * v[0] - x * v[2]), 2.0 * (x * v[1] - y * v[0])];
    [
        v[0] + w * t[0] + (y * t[2] - z * t[1]),
        v[1] + w * t[1] + (z * t[0] - x * t[2]),
        v[2] + w * t[2] + (x * t[1] - y * t[0]),
    ]
}

/// UE's FRotator (pitch, yaw, roll, degrees) as a quaternion.
pub fn rotator(pitch: f64, yaw: f64, roll: f64) -> [f64; 4] {
    let half = |d: f64| (d.to_radians() / 2.0).sin_cos();
    let ((sp, cp), (sy, cy), (sr, cr)) = (half(pitch), half(yaw), half(roll));
    [
        cr * sp * sy - sr * cp * cy,
        -cr * sp * cy - sr * cp * sy,
        cr * cp * sy - sr * sp * cy,
        cr * cp * cy + sr * sp * sy,
    ]
}

/// A row-major FMatrix (doubles) applied to a point: p · M.
fn matrix_apply(mtx: &[f64; 16], p: [f64; 3]) -> [f64; 3] {
    [
        p[0] * mtx[0] + p[1] * mtx[4] + p[2] * mtx[8] + mtx[12],
        p[0] * mtx[1] + p[1] * mtx[5] + p[2] * mtx[9] + mtx[13],
        p[0] * mtx[2] + p[1] * mtx[6] + p[2] * mtx[10] + mtx[14],
    ]
}

/// The 2D convex hull (monotone chain), counter-clockwise.
pub fn hull(points: &mut [[f32; 2]]) -> Vec<[f32; 2]> {
    points.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    let cross = |o: [f32; 2], a: [f32; 2], b: [f32; 2]| (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0]);
    let mut lower: Vec<[f32; 2]> = Vec::new();
    for &p in points.iter() {
        while lower.len() >= 2 && cross(lower[lower.len() - 2], lower[lower.len() - 1], p) <= 0.0 {
            lower.pop();
        }
        lower.push(p);
    }
    let mut upper: Vec<[f32; 2]> = Vec::new();
    for &p in points.iter().rev() {
        while upper.len() >= 2 && cross(upper[upper.len() - 2], upper[upper.len() - 1], p) <= 0.0 {
            upper.pop();
        }
        upper.push(p);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

/// Points in world space → an obstacle, if it has any extent.
fn obstacle(points: &[[f64; 3]]) -> Option<Obstacle> {
    if points.len() < 3 || !points.iter().flatten().all(|v| v.is_finite()) {
        return None;
    }
    let mut flat: Vec<[f32; 2]> = points.iter().map(|p| [p[0] as f32, p[1] as f32]).collect();
    let h = hull(&mut flat);
    let zmin = points.iter().map(|p| p[2]).fold(f64::MAX, f64::min) as f32;
    let zmax = points.iter().map(|p| p[2]).fold(f64::MIN, f64::max) as f32;
    (h.len() >= 3).then_some(Obstacle { hull: h, zmin, zmax, water: false })
}

/// A mesh's collision, as sets of points in its own space — one set per shape.
type Shapes = Arc<Vec<Vec<[f64; 3]>>>;

/// Where, in an element struct, a named field is, and how big.
fn member(n: &Names, m: &dyn Memory, strukt: u64, name: &str) -> Option<(u64, u32)> {
    n.find(m, strukt, name).map(|p| (p.offset as u64, p.size))
}

fn number(m: &dyn Memory, at: u64, size: u32) -> Option<f64> {
    match size {
        4 => mem::read_f32(m, at).map(|v| v as f64),
        8 => mem::read_u64(m, at).map(f64::from_bits),
        _ => None,
    }
}

fn vector(m: &dyn Memory, at: u64) -> Option<[f64; 3]> {
    let mut b = [0u8; 24];
    m.read(at, &mut b).then(|| std::array::from_fn(|i| f64::from_le_bytes(b[i * 8..i * 8 + 8].try_into().unwrap())))
}

/// The elements of one AggGeom array: (element address, its struct).
fn elements(n: &Names, m: &dyn Memory, body: u64, name: &str) -> Vec<(u64, u64)> {
    let Some((at, p)) = n.path(m, body, &["AggGeom", name]) else { return Vec::new() };
    let Some(inner) = n.inner_of(m, p.field) else { return Vec::new() };
    let Some(strukt) = n.struct_of(m, inner) else { return Vec::new() };
    let size = mem::read_u32(m, inner + n.layout.size).unwrap_or(0) as u64;
    let (Some(data), Some(num)) = (mem::read_u64(m, at), mem::read_u32(m, at + 8)) else { return Vec::new() };
    if size == 0 || num == 0 || num > 256 || !mem::plausible(data) {
        return Vec::new();
    }
    (0..num as u64).map(|i| (data + i * size, strukt)).collect()
}

fn ring(c: [f64; 3], r: f64, z0: f64, z1: f64) -> Vec<[f64; 3]> {
    (0..8)
        .flat_map(|i| {
            let a = i as f64 * std::f64::consts::FRAC_PI_4;
            [[c[0] + r * a.cos(), c[1] + r * a.sin(), z0], [c[0] + r * a.cos(), c[1] + r * a.sin(), z1]]
        })
        .collect()
}

fn read_shapes(n: &Names, m: &dyn Memory, mesh: u64, bounds: Option<u64>) -> Vec<Vec<[f64; 3]>> {
    let mut out = Vec::new();
    let Ok(body) = n.follow(m, mesh, "BodySetup") else { return out };
    let rot = |e: u64, s: u64| {
        let (o, _) = member(n, m, s, "Rotation")?;
        let r = vector(m, e + o)?;
        Some(rotator(r[0], r[1], r[2]))
    };
    for (e, s) in elements(n, m, body, "BoxElems") {
        let (Some((co, _)), Some((xo, xs)), Some((yo, ys)), Some((zo, zs))) =
            (member(n, m, s, "Center"), member(n, m, s, "X"), member(n, m, s, "Y"), member(n, m, s, "Z"))
        else {
            continue;
        };
        let (Some(c), Some(x), Some(y), Some(z)) =
            (vector(m, e + co), number(m, e + xo, xs), number(m, e + yo, ys), number(m, e + zo, zs))
        else {
            continue;
        };
        let q = rot(e, s).unwrap_or([0.0, 0.0, 0.0, 1.0]);
        let mut pts = Vec::with_capacity(8);
        for sx in [-0.5, 0.5] {
            for sy in [-0.5, 0.5] {
                for sz in [-0.5, 0.5] {
                    let r = rotate(q, [sx * x, sy * y, sz * z]);
                    pts.push([c[0] + r[0], c[1] + r[1], c[2] + r[2]]);
                }
            }
        }
        out.push(pts);
    }
    for (e, s) in elements(n, m, body, "SphereElems") {
        let (Some((co, _)), Some((ro, rs))) = (member(n, m, s, "Center"), member(n, m, s, "Radius")) else { continue };
        let (Some(c), Some(r)) = (vector(m, e + co), number(m, e + ro, rs)) else { continue };
        out.push(ring(c, r, c[2] - r, c[2] + r));
    }
    for (e, s) in elements(n, m, body, "SphylElems") {
        let (Some((co, _)), Some((ro, rs)), Some((lo, ls))) =
            (member(n, m, s, "Center"), member(n, m, s, "Radius"), member(n, m, s, "Length"))
        else {
            continue;
        };
        let (Some(c), Some(r), Some(len)) = (vector(m, e + co), number(m, e + ro, rs), number(m, e + lo, ls)) else {
            continue;
        };
        let q = rot(e, s).unwrap_or([0.0, 0.0, 0.0, 1.0]);
        let axis = rotate(q, [0.0, 0.0, len / 2.0]);
        let mut pts = Vec::new();
        for end in [1.0, -1.0] {
            let p = [c[0] + end * axis[0], c[1] + end * axis[1], c[2] + end * axis[2]];
            pts.extend(ring(p, r, p[2] - r, p[2] + r));
        }
        out.push(pts);
    }
    for (e, s) in elements(n, m, body, "ConvexElems") {
        let Some((vo, _)) = member(n, m, s, "VertexData") else { continue };
        let (Some(data), Some(num)) = (mem::read_u64(m, e + vo), mem::read_u32(m, e + vo + 8)) else { continue };
        if !(3..=4096).contains(&num) || !mem::plausible(data) {
            continue;
        }
        let mut b = vec![0u8; num as usize * 24];
        if !m.read(data, &mut b) {
            continue;
        }
        out.push(
            b.chunks_exact(24)
                .map(|c| std::array::from_fn(|i| f64::from_le_bytes(c[i * 8..i * 8 + 8].try_into().unwrap())))
                .collect(),
        );
    }
    if out.is_empty() {
        // Complex-as-simple collision: the mesh's own bounds stand in.
        let complex = n
            .field(m, body, "CollisionTraceFlag")
            .and_then(|p| mem::read_u32(m, body + p.offset as u64))
            .map(|v| v & 0xFF)
            == Some(3);
        if let (true, Some(bo)) = (complex, bounds) {
            let mut b = [0u8; 48];
            if m.read(mesh + bo, &mut b) {
                let d = |i: usize| f64::from_le_bytes(b[i * 8..i * 8 + 8].try_into().unwrap());
                let (o, e) = ([d(0), d(1), d(2)], [d(3), d(4), d(5)]);
                let mut pts = Vec::new();
                for sx in [-1.0, 1.0] {
                    for sy in [-1.0, 1.0] {
                        for sz in [-1.0, 1.0] {
                            pts.push([o[0] + sx * e[0], o[1] + sy * e[1], o[2] + sz * e[2]]);
                        }
                    }
                }
                out.push(pts);
            }
        }
    }
    out
}

/// Profiles that never stop a walking character.
const PASSABLE: [&str; 9] = [
    "NoCollision",
    "OverlapAll",
    "OverlapAllDynamic",
    "Trigger",
    "OverlapOnlyPawn",
    "IgnoreOnlyPawn",
    "UI",
    "Spectator",
    // Hell Is Us: shallow water's surface, which stops only the camera.
    "BlockMaterialCast&Camera",
];

/// What kind of component a class is, for this module.
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Plain,
    Instanced,
    /// A BoxComponent; its BoxExtent's offset. Only those owned by deadly water count.
    Box(u64),
    /// A landscape heightfield collision component; its heightfield reference's offset.
    Land(u64),
}

/// Where a component class keeps what this module reads — found once per class, so a
/// component costs a handful of reads, not a walk of its reflection.
#[derive(Clone, Copy)]
struct Fields {
    kind: Kind,
    mesh: u64,
    enabled: Option<u64>,
    profile: Option<u64>,
    /// PerInstanceSMData's offset and element size, for instanced classes.
    instances: Option<(u64, usize)>,
}

/// Where deadly water is: a box's outline from above and the height of its top.
struct Hazard {
    hull: Vec<[f32; 2]>,
    top: f32,
}

/// The water left of the hazards once the ground above them is taken out: runs of
/// 2 m cells along X where the ground lies below a box's top, each run one obstacle.
/// Where the landscape is not loaded, nothing is known, and nothing is water.
fn water(hazards: &[Hazard], terrain: &Terrain, solid: &[Obstacle], hero: [f64; 3]) -> Vec<Obstacle> {
    let key = |v: f32| (v / SAMPLE).floor() as i32;
    let (hx, hy, reach) = (key(hero[0] as f32), key(hero[1] as f32), (REACH as f32 / SAMPLE) as i32);
    // Cells under a deck over the water — a bridge, a pier, a boardwalk: a thin slab
    // whose top is near the surface, big enough to walk on — are not water. Trees,
    // rocks and reeds standing in the water are not decks: counted, their 2 m cells
    // left the marsh full of dry holes that routes went through.
    let lowest = hazards.iter().map(|h| h.top).fold(f32::MAX, f32::min);
    let highest = hazards.iter().map(|h| h.top).fold(f32::MIN, f32::max);
    let deck = |o: &Obstacle| {
        let (lo, hi) = (
            o.hull.iter().fold([f32::MAX; 2], |a, p| [a[0].min(p[0]), a[1].min(p[1])]),
            o.hull.iter().fold([f32::MIN; 2], |a, p| [a[0].max(p[0]), a[1].max(p[1])]),
        );
        o.zmax > lowest - DECK.0
            && o.zmax < highest + DECK.1
            && o.zmax - o.zmin < DECK.2
            && (hi[0] - lo[0]) * (hi[1] - lo[1]) >= DECK.3
    };
    let mut dry: std::collections::HashSet<(i32, i32)> = std::collections::HashSet::new();
    for o in solid.iter().filter(|o| deck(o)) {
        let lo = [
            o.hull.iter().map(|p| p[0]).fold(f32::MAX, f32::min),
            o.hull.iter().map(|p| p[1]).fold(f32::MAX, f32::min),
        ];
        let hi = [
            o.hull.iter().map(|p| p[0]).fold(f32::MIN, f32::max),
            o.hull.iter().map(|p| p[1]).fold(f32::MIN, f32::max),
        ];
        if (hi[0] - lo[0]) * (hi[1] - lo[1]) > 1e8 {
            continue; // bigger than 100 m square: not a bridge
        }
        for y in key(lo[1])..=key(hi[1]) {
            for x in key(lo[0])..=key(hi[0]) {
                dry.insert((x, y));
            }
        }
    }
    let mut out = Vec::new();
    for h in hazards {
        let lo = [
            h.hull.iter().map(|p| p[0]).fold(f32::MAX, f32::min),
            h.hull.iter().map(|p| p[1]).fold(f32::MAX, f32::min),
        ];
        let hi = [
            h.hull.iter().map(|p| p[0]).fold(f32::MIN, f32::max),
            h.hull.iter().map(|p| p[1]).fold(f32::MIN, f32::max),
        ];
        let (x0, x1) = (key(lo[0]).max(hx - reach), key(hi[0]).min(hx + reach));
        let (y0, y1) = (key(lo[1]).max(hy - reach), key(hi[1]).min(hy + reach));
        let inside = |p: [f32; 2]| {
            (0..h.hull.len()).all(|k| {
                let (a, b) = (h.hull[k], h.hull[(k + 1) % h.hull.len()]);
                (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0]) >= 0.0
            })
        };
        for y in y0..=y1 {
            let mut run: Option<i32> = None;
            for x in x0..=x1 + 1 {
                let c = [(x as f32 + 0.5) * SAMPLE, (y as f32 + 0.5) * SAMPLE];
                let wet = x <= x1
                    && inside(c)
                    && !dry.contains(&(x, y))
                    && terrain.height(c[0], c[1]).is_some_and(|z| z < h.top);
                match (wet, run) {
                    (true, None) => run = Some(x),
                    (false, Some(start)) => {
                        let (ax, bx) = (start as f32 * SAMPLE, x as f32 * SAMPLE);
                        let (ay, by) = (y as f32 * SAMPLE, (y + 1) as f32 * SAMPLE);
                        out.push(Obstacle {
                            hull: vec![[ax, ay], [bx, ay], [bx, by], [ax, by]],
                            zmin: f32::MIN,
                            zmax: f32::MAX,
                            water: true,
                        });
                        run = None;
                    }
                    _ => {}
                }
            }
        }
    }
    out
}

#[derive(Default)]
pub struct Obstacles {
    kinds: HashMap<u64, Option<Fields>>,
    shapes: HashMap<u64, Shapes>,
    profiles: HashMap<u32, bool>,
    /// The objects of the pass in progress, and how far through them.
    pending: Vec<u64>,
    cursor: usize,
    building: Vec<Obstacle>,
    hazards: Vec<Hazard>,
    fields: Vec<Heightfield>,
    /// Owner classes, by whether they are deadly water.
    deadly: HashMap<u64, bool>,
    bounds: Option<u64>,
    /// The last complete pass.
    pub done: Arc<Scene>,
}

impl Obstacles {
    fn fields(&mut self, m: &dyn Memory, n: &Names, comp: u64, class: u64) -> Option<Fields> {
        *self.kinds.entry(class).or_insert_with(|| {
            let names: Vec<String> = n.lineage(m, class).into_iter().filter_map(|c| n.object(m, c)).collect();
            let own = names.first().cloned().unwrap_or_default();
            let other = |kind| Some(Fields { kind, mesh: 0, enabled: None, profile: None, instances: None });
            if names.iter().any(|c| c == "BoxComponent") {
                return other(Kind::Box(n.field(m, comp, "BoxExtent")?.offset as u64));
            }
            if names.iter().any(|c| c == "LandscapeHeightfieldCollisionComponent") {
                return other(Kind::Land(terrain::reference_offset(n, m, comp)?));
            }
            if own.starts_with("Grass") || own.starts_with("HLOD") || !names.iter().any(|c| c == "StaticMeshComponent")
            {
                return None;
            }
            let kind =
                if names.iter().any(|c| c == "InstancedStaticMeshComponent") { Kind::Instanced } else { Kind::Plain };
            let mesh = n.field(m, comp, "StaticMesh")?.offset as u64;
            let at = |path: &[&str]| n.path(m, comp, path).map(|(a, _)| a - comp);
            let instances = if kind == Kind::Instanced {
                let (a, p) = n.path(m, comp, &["PerInstanceSMData"])?;
                let size = n.inner_of(m, p.field).and_then(|i| mem::read_u32(m, i + n.layout.size))? as usize;
                Some((a - comp, size))
            } else {
                None
            };
            Some(Fields {
                kind,
                mesh,
                enabled: at(&["BodyInstance", "CollisionEnabled"]),
                profile: at(&["BodyInstance", "CollisionProfileName"]),
                instances,
            })
        })
    }

    fn blocks(&mut self, m: &dyn Memory, n: &Names, comp: u64, f: &Fields) -> bool {
        if f.enabled.and_then(|o| mem::read_u32(m, comp + o)).map(|v| v & 0xFF) == Some(0) {
            return false;
        }
        let Some(idx) = f.profile.and_then(|o| mem::read_u32(m, comp + o)) else { return true };
        *self.profiles.entry(idx).or_insert_with(|| n.get(m, idx).is_none_or(|p| !PASSABLE.contains(&p.as_str())))
    }

    fn shapes(&mut self, m: &dyn Memory, n: &Names, mesh: u64) -> Shapes {
        if self.bounds.is_none() {
            self.bounds = n.field(m, mesh, "ExtendedBounds").map(|p| p.offset as u64);
        }
        let bounds = self.bounds;
        self.shapes.entry(mesh).or_insert_with(|| Arc::new(read_shapes(n, m, mesh, bounds))).clone()
    }

    fn hazard(&mut self, m: &dyn Memory, n: &Names, comp: u64, extent: u64, c2w: Transform) {
        let Some(owner) = mem::read_u64(m, comp + crate::names::OUTER).filter(|&o| mem::plausible(o)) else { return };
        let Some(class) = mem::read_u64(m, owner + CLASS).filter(|&c| mem::plausible(c)) else { return };
        let deadly = *self
            .deadly
            .entry(class)
            .or_insert_with(|| n.object(m, class).is_some_and(|c| c.starts_with("DeadlyWater")));
        let Some(e) = vector(m, comp + extent).filter(|_| deadly) else { return };
        let mut pts = Vec::with_capacity(8);
        for sx in [-1.0, 1.0] {
            for sy in [-1.0, 1.0] {
                for sz in [-1.0, 1.0] {
                    pts.push(c2w.apply([sx * e[0], sy * e[1], sz * e[2]]));
                }
            }
        }
        if let Some(o) = obstacle(&pts) {
            self.hazards.push(Hazard { hull: o.hull, top: o.zmax });
        }
    }

    fn component(&mut self, m: &dyn Memory, n: &Names, comp: u64, f: Fields, hero: [f64; 3]) {
        let Some(c2w) = Transform::read_at(m, comp + COMPONENT_TO_WORLD) else { return };
        match f.kind {
            Kind::Box(extent) => return self.hazard(m, n, comp, extent, c2w),
            Kind::Land(reference) => {
                self.fields.extend(terrain::read(m, comp, reference, c2w.t));
                return;
            }
            _ => {}
        }
        let near = |p: [f64; 3]| (p[0] - hero[0]).hypot(p[1] - hero[1]) <= REACH;
        let Some(mesh) = mem::read_u64(m, comp + f.mesh).filter(|&p| mem::plausible(p)) else { return };
        if !self.blocks(m, n, comp, &f) {
            return;
        }
        let shapes = self.shapes(m, n, mesh);
        if shapes.is_empty() {
            return;
        }
        match f.kind {
            Kind::Box(_) | Kind::Land(_) => {}
            Kind::Plain => {
                if !near(c2w.t) {
                    return;
                }
                for s in shapes.iter() {
                    let pts: Vec<[f64; 3]> = s.iter().map(|p| c2w.apply(*p)).collect();
                    self.building.extend(obstacle(&pts));
                }
            }
            Kind::Instanced => {
                let Some((off, size)) = f.instances else { return };
                let at = comp + off;
                let (Some(data), Some(num)) = (mem::read_u64(m, at), mem::read_u32(m, at + 8)) else { return };
                if size < 128 || num == 0 || num > 200_000 || !mem::plausible(data) {
                    return;
                }
                let mut b = vec![0u8; num as usize * size];
                if !m.read(data, &mut b) {
                    return;
                }
                for inst in b.chunks_exact(size) {
                    let mtx: [f64; 16] =
                        std::array::from_fn(|i| f64::from_le_bytes(inst[i * 8..i * 8 + 8].try_into().unwrap()));
                    let origin = c2w.apply([mtx[12], mtx[13], mtx[14]]);
                    if !near(origin) {
                        continue;
                    }
                    for s in shapes.iter() {
                        let pts: Vec<[f64; 3]> = s.iter().map(|p| c2w.apply(matrix_apply(&mtx, *p))).collect();
                        self.building.extend(obstacle(&pts));
                    }
                }
            }
        }
    }

    /// One slice of the pass; when the pass completes, `done` is replaced.
    pub fn step(&mut self, m: &dyn Memory, n: &Names, objects: &Objects, hero: [f64; 3]) {
        if self.cursor >= self.pending.len() {
            if !self.pending.is_empty() {
                let terrain = Terrain::new(std::mem::take(&mut self.fields));
                let mut obstacles = std::mem::take(&mut self.building);
                let wet = water(&std::mem::take(&mut self.hazards), &terrain, &obstacles, hero);
                obstacles.extend(wet);
                self.done = Arc::new(Scene { obstacles, terrain });
            }
            self.pending = objects.all(m);
            self.cursor = 0;
        }
        let end = (self.cursor + SLICE).min(self.pending.len());
        for i in self.cursor..end {
            let o = self.pending[i];
            let Some(class) = mem::read_u64(m, o + CLASS).filter(|&c| mem::plausible(c)) else { continue };
            if let Some(f) = self.fields(m, n, o, class) {
                self.component(m, n, o, f, hero);
            }
        }
        self.cursor = end;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: [f64; 3], b: [f64; 3]) -> bool {
        (0..3).all(|i| (a[i] - b[i]).abs() < 1e-6)
    }

    #[test]
    fn a_yaw_turns_x_into_y() {
        let q = rotator(0.0, 90.0, 0.0);
        assert!(close(rotate(q, [1.0, 0.0, 0.0]), [0.0, 1.0, 0.0]));
        let t = Transform { q, t: [100.0, 0.0, 0.0], s: [2.0, 2.0, 2.0] };
        assert!(close(t.apply([1.0, 0.0, 0.0]), [100.0, 2.0, 0.0]));
    }

    #[test]
    fn a_pitch_lays_z_along_x() {
        // Pitch 90 tilts +X up to +Z; so +Z goes to -X.
        let q = rotator(90.0, 0.0, 0.0);
        assert!(close(rotate(q, [1.0, 0.0, 0.0]), [0.0, 0.0, 1.0]));
    }

    #[test]
    fn hulls_drop_the_inside() {
        let mut p = [[0.0, 0.0], [4.0, 0.0], [4.0, 4.0], [0.0, 4.0], [2.0, 2.0], [1.0, 3.0]];
        let h = hull(&mut p);
        assert_eq!(h.len(), 4, "{h:?}");
    }

    #[test]
    fn water_is_the_box_where_the_ground_is_below_its_top() {
        // A 20 m box whose top is at z 0, over ground rising west to east: -100 at
        // x = 0, +100 at x = 2000 — so the western half is under water.
        let hazard = Hazard { hull: vec![[0.0, 0.0], [2000.0, 0.0], [2000.0, 2000.0], [0.0, 2000.0]], top: 0.0 };
        let n = 21;
        let z = (0..n * n).map(|i| -100.0 + (i % n) as f32 * 10.0).collect();
        let terrain = Terrain::new(vec![Heightfield { origin: [0.0, 0.0, 0.0], spacing: [100.0, 100.0], n, z }]);
        let w = water(&[hazard], &terrain, &[], [1000.0, 1000.0, 0.0]);
        assert_eq!(w.len(), 10, "one run a row");
        for o in &w {
            let xs: Vec<f32> = o.hull.iter().map(|p| p[0]).collect();
            assert_eq!(xs.iter().cloned().fold(f32::MAX, f32::min), 0.0);
            assert_eq!(xs.iter().cloned().fold(f32::MIN, f32::max), 1000.0, "dry from the middle on");
            assert!(o.water);
        }
    }

    #[test]
    fn a_deck_dries_the_water_under_it_and_a_tree_does_not() {
        let hazard = Hazard { hull: vec![[0.0, 0.0], [2000.0, 0.0], [2000.0, 2000.0], [0.0, 2000.0]], top: 0.0 };
        let n = 21;
        let terrain = Terrain::new(vec![Heightfield {
            origin: [0.0, 0.0, 0.0],
            spacing: [100.0, 100.0],
            n,
            z: vec![-100.0; n * n],
        }]);
        let slab = |x0: f32, x1: f32, z0: f32, z1: f32| Obstacle {
            hull: vec![[x0, 0.0], [x1, 0.0], [x1, 2000.0], [x0, 2000.0]],
            zmin: z0,
            zmax: z1,
            water: false,
        };
        let wet = |solid: &[Obstacle]| -> f32 {
            water(std::slice::from_ref(&hazard), &terrain, solid, [1000.0, 1000.0, 0.0])
                .iter()
                .map(|o| o.hull[1][0] - o.hull[0][0])
                .sum()
        };
        let all = wet(&[]);
        assert_eq!(wet(&[slab(900.0, 1100.0, -500.0, 900.0)]), all, "a tree standing in it: still water");
        assert!(wet(&[slab(800.0, 1200.0, 20.0, 60.0)]) < all, "a bridge deck: dry under it");
    }

    #[test]
    fn an_instance_matrix_moves_points() {
        let mut mtx = [0.0; 16];
        mtx[0] = 1.0;
        mtx[5] = 1.0;
        mtx[10] = 1.0;
        mtx[15] = 1.0;
        mtx[12] = 500.0;
        assert!(close(matrix_apply(&mtx, [1.0, 2.0, 3.0]), [501.0, 2.0, 3.0]));
    }
}
