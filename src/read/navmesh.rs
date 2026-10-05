//! The game's own walking navmesh, read from memory: what its AI walks on — floors,
//! stairs, cellars, bridges — so a route can go down to a cellar or up a stair, which
//! the grid of obstacles (pathfind.rs) cannot (.spec/ROUTES.md §6–7).
//!
//! World Partition keeps it in `NavigationDataChunkActor`s: `NavDataChunks` (+0x2A8)
//! holds a `RecastNavMeshDataChunk` whose `Tiles` (+0x30) are 0x48-byte records —
//! TileDataSize at +0x14, a TSharedPtr<FRawData> at +0x18 whose object starts with the
//! raw Detour tile (the compressed tile-cache layers beside it are not used). Tiles not
//! in the expected format (a version other than 7) are skipped.
//!
//! A tile (UE 5.5 Detour, large-world coordinates in doubles):
//! - header, 0x58 bytes: u16 version (7), layer, polyCount, vertCount; i32 x, y; u16
//!   counts; at +0x28 double bmin[3], bmax[3]
//! - verts at 0x58: double[3] each, in Recast space (x, up, z) — Unreal (−x, −z, up)
//! - polys after them, 32 bytes each: u32 firstLink, u16 verts[6], u16 neis[6], u16
//!   flags, u8 vertCount, u8 area | type << 6 (type 1: an off-mesh link, skipped)
//!
//! A neighbour is a 1-based poly of the same tile, or (0x8000 set) across the tile's
//! side. Across tiles, polys are joined where their border edges overlap.
//!
//! Routes are A* over polys, through the portals between them, then pulled straight
//! with the funnel algorithm.

use crate::mem::{self, Memory};
use crate::names::Names;
use crate::pathfind::Path;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

const VERSION: u16 = 7;
/// A `FRecastTileData` record's size.
const RECORD: usize = 0x48;
const HEADER: usize = 0x58;
const POLY: usize = 32;
const EXTERNAL: u16 = 0x8000;
/// Two border edges are one portal when their heights differ by less than this (cm).
const STEP: f32 = 60.0;
/// A goal this near a floor that can be walked to (cm across), this far above it at most
/// or this far below, is reached from that floor (`floor_under`).
const SNAP: f32 = 300.0;
const SNAP_BELOW: f32 = 800.0;
const SNAP_ABOVE: f32 = 150.0;
/// The lookup grid's cell (cm).
const BUCKET: f32 = 1000.0;
/// A point finds its poly within this far from above (cm), and this far up or down.
const NEAR: f32 = 500.0;
const UPDOWN: f32 = 300.0;

#[derive(Clone, Debug)]
struct Poly {
    /// Unreal-space corners, in the tile's winding.
    corners: Vec<[f32; 3]>,
    centre: [f32; 3],
    /// Neighbours: (poly, portal ends).
    links: Vec<(u32, [[f32; 3]; 2])>,
}

#[derive(Clone, Debug, Default)]
pub struct NavMesh {
    /// Diagnostics: border edges, those joined, overlaps refused for height.
    pub stats: [usize; 3],
    polys: Vec<Poly>,
    /// Polys by the lookup cells their outline covers.
    grid: HashMap<(i32, i32), Vec<u32>>,
}

/// Recast (x, up, z) → Unreal (x, y, z).
fn unreal(r: [f64; 3]) -> [f32; 3] {
    [-r[0] as f32, -r[2] as f32, r[1] as f32]
}

fn u16_at(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}

fn f64_at(b: &[u8], o: usize) -> f64 {
    f64::from_le_bytes(b[o..o + 8].try_into().unwrap())
}

/// One tile's walkable polys, with their in-tile neighbours and border edges.
struct Tile {
    polys: Vec<(Vec<[f32; 3]>, Vec<u16>)>,
}

fn parse(b: &[u8]) -> Option<Tile> {
    if b.len() < HEADER || u16_at(b, 0) != VERSION {
        return None;
    }
    let (polys, verts) = (u16_at(b, 4) as usize, u16_at(b, 6) as usize);
    let po = HEADER + verts * 24;
    if po + polys * POLY > b.len() {
        return None;
    }
    let vert = |k: usize| {
        unreal([f64_at(b, HEADER + k * 24), f64_at(b, HEADER + k * 24 + 8), f64_at(b, HEADER + k * 24 + 16)])
    };
    let mut out = Vec::with_capacity(polys);
    for k in 0..polys {
        let q = po + k * POLY;
        let n = b[q + 30] as usize;
        if b[q + 31] >> 6 == 1 {
            // An off-mesh link: kept as an empty slot so indices stay right.
            out.push((Vec::new(), Vec::new()));
            continue;
        }
        if !(3..=6).contains(&n) {
            return None;
        }
        let mut corners = Vec::with_capacity(n);
        let mut neis = Vec::with_capacity(n);
        for v in 0..n {
            let i = u16_at(b, q + 4 + v * 2) as usize;
            if i >= verts {
                return None;
            }
            corners.push(vert(i));
            neis.push(u16_at(b, q + 16 + v * 2));
        }
        out.push((corners, neis));
    }
    Some(Tile { polys: out })
}

fn cell(x: f32, y: f32) -> (i32, i32) {
    ((x / BUCKET).floor() as i32, (y / BUCKET).floor() as i32)
}

fn flat(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

/// How far `p` is from the outline of a poly, across (cm).
fn outline_distance(corners: &[[f32; 3]], p: [f32; 3]) -> f32 {
    (0..corners.len())
        .map(|k| {
            let (a, b) = (corners[k], corners[(k + 1) % corners.len()]);
            let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
            let len = dx * dx + dy * dy;
            let t = if len > 0.0 { (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / len).clamp(0.0, 1.0) } else { 0.0 };
            (p[0] - (a[0] + t * dx)).hypot(p[1] - (a[1] + t * dy))
        })
        .fold(f32::INFINITY, f32::min)
}

fn dist(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// 2D cross product of (b − a) × (c − a): > 0 when c is left of a→b (Unreal X right,
/// Y down on the map — the sign only has to be used consistently).
fn cross(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> f32 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}

impl NavMesh {
    /// Build from raw tiles.
    pub fn from_tiles(tiles: &[Vec<u8>]) -> NavMesh {
        let mut mesh = NavMesh::default();
        // Border edges waiting for a partner: (poly, a, b).
        let mut border: Vec<(u32, [f32; 3], [f32; 3])> = Vec::new();
        // The same tile comes from more than one chunk (every one, twice or more): once.
        let mut seen = std::collections::HashSet::new();
        for raw in tiles {
            if raw.len() >= 0x10 && !seen.insert((u16_at(raw, 2), &raw[8..16])) {
                continue;
            }
            let Some(t) = parse(raw) else { continue };
            let base = mesh.polys.len() as u32;
            for (corners, _) in &t.polys {
                let n = corners.len().max(1) as f32;
                let centre = corners.iter().fold([0.0; 3], |s, c| [s[0] + c[0] / n, s[1] + c[1] / n, s[2] + c[2] / n]);
                mesh.polys.push(Poly { corners: corners.clone(), centre, links: Vec::new() });
            }
            for (k, (corners, neis)) in t.polys.iter().enumerate() {
                let me = base + k as u32;
                for (e, &nei) in neis.iter().enumerate() {
                    let (a, b) = (corners[e], corners[(e + 1) % corners.len()]);
                    if nei & EXTERNAL != 0 {
                        border.push((me, a, b));
                    } else if nei != 0
                        && ((nei - 1) as usize) < t.polys.len()
                        && !t.polys[(nei - 1) as usize].0.is_empty()
                    {
                        mesh.polys[me as usize].links.push((base + nei as u32 - 1, [a, b]));
                    }
                }
            }
        }
        mesh.join(&border);
        for (i, p) in mesh.polys.iter().enumerate() {
            if p.corners.is_empty() {
                continue;
            }
            let (lo, hi) = p.corners.iter().fold(([f32::MAX; 2], [f32::MIN; 2]), |(lo, hi), c| {
                ([lo[0].min(c[0]), lo[1].min(c[1])], [hi[0].max(c[0]), hi[1].max(c[1])])
            });
            let (c0, c1) = (cell(lo[0], lo[1]), cell(hi[0], hi[1]));
            for y in c0.1..=c1.1 {
                for x in c0.0..=c1.0 {
                    mesh.grid.entry((x, y)).or_default().push(i as u32);
                }
            }
        }
        mesh
    }

    /// Join polys of neighbouring tiles where their border edges lie on the same line
    /// and overlap, at about the same height.
    fn join(&mut self, border: &[(u32, [f32; 3], [f32; 3])]) {
        // Tile sides are axis-aligned: bucket by the side's line.
        let mut lines: HashMap<(u8, i32), Vec<usize>> = HashMap::new();
        self.stats[0] = border.len();
        let mut joined = vec![false; border.len()];
        for (i, &(_, a, b)) in border.iter().enumerate() {
            let key = if (a[0] - b[0]).abs() < 1.0 {
                (0, a[0].round() as i32)
            } else if (a[1] - b[1]).abs() < 1.0 {
                (1, a[1].round() as i32)
            } else {
                continue;
            };
            lines.entry(key).or_default().push(i);
        }
        for ((axis, _), list) in lines {
            let along = if axis == 0 { 1 } else { 0 };
            for (n, &i) in list.iter().enumerate() {
                for &j in &list[n + 1..] {
                    let ((pi, a1, b1), (pj, a2, b2)) = (border[i], border[j]);
                    if pi == pj {
                        continue;
                    }
                    let (lo1, hi1) = (a1[along].min(b1[along]), a1[along].max(b1[along]));
                    let (lo2, hi2) = (a2[along].min(b2[along]), a2[along].max(b2[along]));
                    let (lo, hi) = (lo1.max(lo2), hi1.min(hi2));
                    if hi - lo < 10.0 {
                        continue;
                    }
                    // Heights of both edges over the overlap.
                    let at = |a: [f32; 3], b: [f32; 3], t: f32| {
                        let k = if (b[along] - a[along]).abs() < f32::EPSILON {
                            0.0
                        } else {
                            (t - a[along]) / (b[along] - a[along])
                        };
                        let mut p = [a[0] + (b[0] - a[0]) * k, a[1] + (b[1] - a[1]) * k, a[2] + (b[2] - a[2]) * k];
                        p[along] = t;
                        p
                    };
                    let (p0, p1) = (at(a1, b1, lo), at(a1, b1, hi));
                    let (q0, q1) = (at(a2, b2, lo), at(a2, b2, hi));
                    if (p0[2] - q0[2]).abs() > STEP || (p1[2] - q1[2]).abs() > STEP {
                        self.stats[2] += 1;
                        continue;
                    }
                    joined[i] = true;
                    joined[j] = true;
                    self.polys[pi as usize].links.push((pj, [p0, p1]));
                    self.polys[pj as usize].links.push((pi, [p1, p0]));
                }
            }
        }
        self.stats[1] = joined.iter().filter(|&&j| j).count();
    }

    /// Diagnostics: each poly's component id, and how many border edges found no partner.
    pub fn components(&self) -> Vec<u32> {
        let mut comp = vec![u32::MAX; self.polys.len()];
        let mut next = 0;
        for s in 0..self.polys.len() {
            if comp[s] != u32::MAX || self.polys[s].corners.is_empty() {
                continue;
            }
            let mut stack = vec![s as u32];
            comp[s] = next;
            while let Some(u) = stack.pop() {
                for &(v, _) in &self.polys[u as usize].links {
                    if comp[v as usize] == u32::MAX {
                        comp[v as usize] = next;
                        stack.push(v);
                    }
                }
            }
            next += 1;
        }
        comp
    }

    pub fn is_empty(&self) -> bool {
        self.polys.is_empty()
    }

    pub fn len(&self) -> usize {
        self.polys.len()
    }

    /// The height of poly `i` under (x, y), if (x, y) is inside it.
    fn height_in(&self, i: u32, x: f32, y: f32) -> Option<f32> {
        let c = &self.polys[i as usize].corners;
        if c.len() < 3 {
            return None;
        }
        let p = [x, y, 0.0];
        // Convex: inside when on the same side of every edge.
        let sides: Vec<f32> = (0..c.len()).map(|k| cross(c[k], c[(k + 1) % c.len()], p)).collect();
        if !(sides.iter().all(|&s| s >= -1.0) || sides.iter().all(|&s| s <= 1.0)) {
            return None;
        }
        // The fan triangle holding it: its plane's height.
        for k in 1..c.len() - 1 {
            let (a, b, d) = (c[0], c[k], c[k + 1]);
            let det = (b[1] - d[1]) * (a[0] - d[0]) + (d[0] - b[0]) * (a[1] - d[1]);
            if det.abs() < f32::EPSILON {
                continue;
            }
            let l1 = ((b[1] - d[1]) * (x - d[0]) + (d[0] - b[0]) * (y - d[1])) / det;
            let l2 = ((d[1] - a[1]) * (x - d[0]) + (a[0] - d[0]) * (y - d[1])) / det;
            let l3 = 1.0 - l1 - l2;
            if l1 >= -0.01 && l2 >= -0.01 && l3 >= -0.01 {
                return Some(l1 * a[2] + l2 * b[2] + l3 * d[2]);
            }
        }
        Some(self.polys[i as usize].centre[2])
    }

    /// The poly a point (Unreal cm; z at the feet) stands on: the one under it nearest
    /// in height, else the nearest one around — and the point on it.
    pub fn locate(&self, p: [f32; 3]) -> Option<(u32, [f32; 3])> {
        let (cx, cy) = cell(p[0], p[1]);
        let mut best: Option<(f32, u32, [f32; 3])> = None;
        for y in cy - 1..=cy + 1 {
            for x in cx - 1..=cx + 1 {
                for &i in self.grid.get(&(x, y)).into_iter().flatten() {
                    if let Some(z) = self.height_in(i, p[0], p[1]) {
                        let d = (z - p[2]).abs();
                        if d <= UPDOWN && best.is_none_or(|(b, _, _)| d < b) {
                            best = Some((d, i, [p[0], p[1], z]));
                        }
                    }
                }
            }
        }
        if let Some((_, i, q)) = best {
            return Some((i, q));
        }
        // Not over any: the nearest centre about the same height.
        let mut near: Option<(f32, u32)> = None;
        for y in cy - 1..=cy + 1 {
            for x in cx - 1..=cx + 1 {
                for &i in self.grid.get(&(x, y)).into_iter().flatten() {
                    let c = self.polys[i as usize].centre;
                    let (d, dz) = (flat(c, p), (c[2] - p[2]).abs());
                    if d <= NEAR && dz <= UPDOWN && near.is_none_or(|(b, _)| d < b) {
                        near = Some((d, i));
                    }
                }
            }
        }
        near.map(|(_, i)| (i, self.polys[i as usize].centre))
    }

    /// A walking route from `a` to `b` (Unreal cm, z at the feet), or `None` when
    /// either end is off the navmesh or they are not connected.
    ///
    /// When `b` cannot be walked to from `a` — behind a door that opens by a key or a
    /// puzzle, in a cellar reached another way — the route goes to the reachable place
    /// nearest it, and its last leg, to `b` itself, is marked as going through.
    pub fn route(&self, a: [f32; 3], b: [f32; 3]) -> Option<(Path, Vec<f32>)> {
        let (s, pa) = self.locate(a)?;
        match self.locate(b) {
            Some((t, pb)) if self.connected(s, t) => self.walk(s, t, pa, pb),
            _ => {
                // The nearest reachable poly, height counting double: a floor above or
                // below is further than the same distance across.
                let reach = self.reachable(s);
                let score = |c: [f32; 3]| flat(c, b) + 2.0 * (c[2] - b[2]).abs();
                let near = (0..self.polys.len())
                    .filter(|&i| reach[i])
                    .min_by(|&i, &j| score(self.polys[i].centre).total_cmp(&score(self.polys[j].centre)))?;
                let pn = self.polys[near].centre;
                let (mut path, mut heights) = self.walk(s, near as u32, pa, pn)?;
                // Within reach from a floor it can walk to: the goal is reached, not through
                // something. A lever on a wall, a mechanism up high, an item on a shelf: the
                // goal's point is not on the floor, or on a scrap of navmesh of its own.
                let reached = self.floor_under(b, &reach);
                path.points.push([b[0], b[1]]);
                path.through.push(!reached);
                heights.push(b[2]);
                Some((path, heights))
            }
        }
    }

    /// Whether a floor of `reach` lies within `SNAP` across of `b`, at most `SNAP_BELOW`
    /// below it or `SNAP_ABOVE` above it: the goal can be reached from there.
    fn floor_under(&self, b: [f32; 3], reach: &[bool]) -> bool {
        let (cx, cy) = cell(b[0], b[1]);
        (cy - 1..=cy + 1).any(|y| {
            (cx - 1..=cx + 1).any(|x| {
                self.grid.get(&(x, y)).into_iter().flatten().any(|&i| {
                    let p = &self.polys[i as usize];
                    let dz = b[2] - p.centre[2];
                    reach[i as usize]
                        && (-SNAP_ABOVE..=SNAP_BELOW).contains(&dz)
                        && outline_distance(&p.corners, b) <= SNAP
                })
            })
        })
    }

    /// Whether `t` can be walked to from `s`.
    fn connected(&self, s: u32, t: u32) -> bool {
        self.reachable(s)[t as usize]
    }

    /// Every poly that can be walked to from `s`.
    fn reachable(&self, s: u32) -> Vec<bool> {
        let mut seen = vec![false; self.polys.len()];
        let mut stack = vec![s];
        seen[s as usize] = true;
        while let Some(u) = stack.pop() {
            for &(v, _) in &self.polys[u as usize].links {
                if !seen[v as usize] {
                    seen[v as usize] = true;
                    stack.push(v);
                }
            }
        }
        seen
    }

    fn walk(&self, s: u32, t: u32, pa: [f32; 3], pb: [f32; 3]) -> Option<(Path, Vec<f32>)> {
        let polys = self.astar(s, t, pa, pb)?;
        // The portals along the way, as (left, right) for the funnel.
        let mut portals: Vec<([f32; 3], [f32; 3])> = vec![(pa, pa)];
        for w in polys.windows(2) {
            let (_, [e0, e1]) = *self.polys[w[0] as usize].links.iter().find(|(n, _)| *n == w[1])?;
            let (from, to) = (self.polys[w[0] as usize].centre, self.polys[w[1] as usize].centre);
            // Left/right as seen going from one poly's centre to the next's.
            let (l, r) = if cross(from, to, e0) > 0.0 { (e0, e1) } else { (e1, e0) };
            portals.push((l, r));
        }
        portals.push((pb, pb));
        let pts = funnel(&portals);
        let points: Vec<[f32; 2]> = pts.iter().map(|p| [p[0], p[1]]).collect();
        let heights = pts.iter().map(|p| p[2]).collect();
        let through = vec![false; points.len().saturating_sub(1)];
        Some((Path { points, through }, heights))
    }

    fn astar(&self, s: u32, t: u32, pa: [f32; 3], pb: [f32; 3]) -> Option<Vec<u32>> {
        let n = self.polys.len();
        let mut g = vec![f32::INFINITY; n];
        let mut at = vec![[0.0f32; 3]; n];
        let mut prev = vec![u32::MAX; n];
        let mut open = BinaryHeap::new();
        g[s as usize] = 0.0;
        at[s as usize] = pa;
        open.push((Reverse((dist(pa, pb) * 10.0) as u64), s));
        let mut steps = 0;
        while let Some((_, u)) = open.pop() {
            if u == t {
                let mut path = vec![t];
                let mut c = t;
                while prev[c as usize] != u32::MAX {
                    c = prev[c as usize];
                    path.push(c);
                }
                path.reverse();
                return Some(path);
            }
            steps += 1;
            if steps > 200_000 {
                return None;
            }
            let here = at[u as usize];
            for &(v, [e0, e1]) in &self.polys[u as usize].links {
                let mid = [(e0[0] + e1[0]) / 2.0, (e0[1] + e1[1]) / 2.0, (e0[2] + e1[2]) / 2.0];
                let to = if v == t { pb } else { mid };
                let cost = g[u as usize] + dist(here, to);
                if cost < g[v as usize] {
                    g[v as usize] = cost;
                    at[v as usize] = to;
                    prev[v as usize] = u;
                    open.push((Reverse(((cost + dist(to, pb)) * 10.0) as u64), v));
                }
            }
        }
        None
    }
}

/// The simple stupid funnel algorithm: the shortest line through the portals, as its
/// corners (first and last portals are the two ends).
fn funnel(portals: &[([f32; 3], [f32; 3])]) -> Vec<[f32; 3]> {
    let mut out = vec![portals[0].0];
    let (mut apex, mut left, mut right) = (portals[0].0, portals[0].0, portals[0].1);
    let (mut left_i, mut right_i) = (0usize, 0usize);
    let mut i = 1;
    while i < portals.len() {
        let (l, r) = portals[i];
        // Tighten the right side.
        if cross(apex, right, r) >= 0.0 {
            if apex == right || cross(apex, left, r) < 0.0 {
                right = r;
                right_i = i;
            } else {
                // Right crossed left: left is a corner.
                apex = left;
                let apex_i = left_i;
                out.push(apex);
                right = apex;
                right_i = apex_i;
                i = apex_i + 1;
                continue;
            }
        }
        // Tighten the left side.
        if cross(apex, left, l) <= 0.0 {
            if apex == left || cross(apex, right, l) > 0.0 {
                left = l;
                left_i = i;
            } else {
                apex = right;
                let apex_i = right_i;
                out.push(apex);
                left = apex;
                left_i = apex_i;
                i = apex_i + 1;
                continue;
            }
        }
        i += 1;
    }
    let end = portals[portals.len() - 1].0;
    if out.last() != Some(&end) {
        out.push(end);
    }
    out
}

/// The raw tiles of every loaded `RecastNavMeshDataChunk`.
pub fn read_tiles(m: &dyn Memory, n: &Names, chunks: &[u64]) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    for &c in chunks {
        if n.class(m, c).as_deref() != Some("RecastNavMeshDataChunk") {
            continue;
        }
        let (Some(data), Some(num)) = (mem::read_u64(m, c + 0x30), mem::read_u32(m, c + 0x38)) else { continue };
        if !mem::plausible(data) || num > 100_000 {
            continue;
        }
        let mut recs = vec![0u8; num as usize * RECORD];
        if !m.read(data, &mut recs) {
            continue;
        }
        for r in recs.chunks_exact(RECORD) {
            let size = u32::from_le_bytes(r[0x14..0x18].try_into().unwrap()) as usize;
            let obj = u64::from_le_bytes(r[0x18..0x20].try_into().unwrap());
            if !(HEADER..=1 << 20).contains(&size) || !mem::plausible(obj) {
                continue;
            }
            let Some(raw) = mem::read_u64(m, obj).filter(|&p| mem::plausible(p)) else { continue };
            let mut head = [0u8; 2];
            if !m.read(raw, &mut head) || u16::from_le_bytes(head) != VERSION {
                continue;
            }
            let mut b = vec![0u8; size];
            if m.read(raw, &mut b) {
                out.push(b);
            }
        }
    }
    out
}

/// How near an opened door a poly must come to be joined through it (cm, across; and up
/// or down from the door's foot).
const BRIDGE: f32 = 300.0;
const BRIDGE_UPDOWN: f32 = 200.0;

impl NavMesh {
    /// The mesh with a way through each of `doors`: where a door the hero can pass stands,
    /// and for a one-sided door still shut, the side it opens from. The navmesh is the
    /// game's baked one, and every door is closed in it: a wall for good, opened or not
    /// (ROUTES.md §9). Near each door, the polys fall in groups joined among themselves;
    /// the group nearest the door is joined to each other group through the door's point —
    /// for a one-sided door, not from its locked side to its open side. Where the two sides
    /// were joined already, nothing changes.
    pub fn bridged(&self, doors: &[([f32; 3], Option<[f32; 3]>)]) -> NavMesh {
        let mut out = self.clone();
        for &(d, from) in doors {
            // Which side of a one-sided door a point is: < 0 its locked side.
            let side =
                |p: [f32; 3]| from.map_or(0.0, |o| (p[0] - d[0]) * (o[0] - d[0]) + (p[1] - d[1]) * (o[1] - d[1]));
            let (cx, cy) = cell(d[0], d[1]);
            let mut near: Vec<u32> = Vec::new();
            for y in cy - 1..=cy + 1 {
                for x in cx - 1..=cx + 1 {
                    for &i in out.grid.get(&(x, y)).into_iter().flatten() {
                        let p = &out.polys[i as usize];
                        let close = outline_distance(&p.corners, d) <= BRIDGE;
                        if close && (p.centre[2] - d[2]).abs() <= BRIDGE_UPDOWN && !near.contains(&i) {
                            near.push(i);
                        }
                    }
                }
            }
            // The groups, by links among the near polys only.
            let mut group = vec![usize::MAX; near.len()];
            let mut groups = 0;
            for k in 0..near.len() {
                if group[k] != usize::MAX {
                    continue;
                }
                let mut stack = vec![k];
                group[k] = groups;
                while let Some(u) = stack.pop() {
                    for &(v, _) in &out.polys[near[u] as usize].links {
                        if let Some(j) = near.iter().position(|&w| w == v).filter(|&j| group[j] == usize::MAX) {
                            group[j] = groups;
                            stack.push(j);
                        }
                    }
                }
                groups += 1;
            }
            if groups < 2 {
                continue;
            }
            // Each group's poly nearest the door.
            let mut best: Vec<Option<(f32, u32)>> = vec![None; groups];
            for (k, &i) in near.iter().enumerate() {
                let dist = flat(out.polys[i as usize].centre, d);
                if best[group[k]].is_none_or(|(b, _)| dist < b) {
                    best[group[k]] = Some((dist, i));
                }
            }
            let mut ends: Vec<(f32, u32)> = best.into_iter().flatten().collect();
            ends.sort_by(|a, b| a.0.total_cmp(&b.0));
            let (_, hub) = ends[0];
            for &(_, other) in &ends[1..] {
                for (a, b) in [(hub, other), (other, hub)] {
                    if side(out.polys[a as usize].centre) < 0.0 && side(out.polys[b as usize].centre) > 0.0 {
                        continue;
                    }
                    let z = out.polys[a as usize].centre[2];
                    let gate = [d[0], d[1], z];
                    out.polys[a as usize].links.push((b, [gate, gate]));
                }
            }
        }
        out
    }
}

/// How near a shut one-sided door a portal must be to be the way through it (cm, across;
/// and up or down).
const ONE_WAY_NEAR: f32 = 200.0;

impl NavMesh {
    /// The mesh with each of `doors` (where a shut one-sided door stands, and the side it
    /// opens from) passable one way only: from the side it opens from. A portal near the
    /// door from a poly on its locked side to one on its open side is dropped, so a route
    /// from the locked side goes round, or ends short where there is no way round.
    pub fn one_way(mut self, doors: &[[[f32; 3]; 2]]) -> NavMesh {
        let centres: Vec<[f32; 3]> = self.polys.iter().map(|p| p.centre).collect();
        for &[at, from] in doors {
            // Which side of the door a point is: > 0 the side it opens from.
            let side = |p: [f32; 3]| (p[0] - at[0]) * (from[0] - at[0]) + (p[1] - at[1]) * (from[1] - at[1]);
            let (cx, cy) = cell(at[0], at[1]);
            let mut near: Vec<u32> = Vec::new();
            for y in cy - 1..=cy + 1 {
                for x in cx - 1..=cx + 1 {
                    near.extend(self.grid.get(&(x, y)).into_iter().flatten().copied());
                }
            }
            near.sort_unstable();
            near.dedup();
            for u in near {
                let cu = self.polys[u as usize].centre;
                if side(cu) >= 0.0 {
                    continue;
                }
                self.polys[u as usize].links.retain(|&(v, [e0, e1])| {
                    let mid = [(e0[0] + e1[0]) / 2.0, (e0[1] + e1[1]) / 2.0, (e0[2] + e1[2]) / 2.0];
                    let through = flat(mid, at) <= ONE_WAY_NEAR
                        && (mid[2] - at[2]).abs() <= ONE_WAY_NEAR
                        && side(centres[v as usize]) > 0.0;
                    !through
                });
            }
        }
        self
    }
}

/// Reads the navmesh a little at a time: when the set of chunk actors changes (World
/// Partition streamed some in or out), their tiles are read two actors a step, then
/// the mesh is built and published.
#[derive(Default)]
pub struct Nav {
    actors: Vec<u64>,
    queue: Vec<u64>,
    tiles: Vec<Vec<u8>>,
    pub done: std::sync::Arc<NavMesh>,
}

impl Nav {
    pub fn step(&mut self, m: &dyn Memory, n: &Names, actors: &[u64]) {
        let mut now = actors.to_vec();
        now.sort_unstable();
        if now != self.actors {
            self.actors = now.clone();
            self.queue = now;
            self.tiles.clear();
            return;
        }
        if self.queue.is_empty() {
            return;
        }
        let take = self.queue.len().min(2);
        let batch: Vec<u64> = self.queue.drain(..take).collect();
        self.tiles.extend(read_tiles(m, n, &chunks_of(m, &batch)));
        if self.queue.is_empty() {
            let mesh = NavMesh::from_tiles(&std::mem::take(&mut self.tiles));
            crate::logfile::line(&format!("navmesh: {} actors, {} polys", self.actors.len(), mesh.len()));
            self.done = std::sync::Arc::new(mesh);
        }
    }
}

/// Every loaded chunk actor's chunks: `NavigationDataChunkActor.NavDataChunks`.
pub fn chunks_of(m: &dyn Memory, actors: &[u64]) -> Vec<u64> {
    actors.iter().flat_map(|&a| crate::actors::array(m, a + 0x2a8, 64)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tile in the format read here: `polys` as corner lists over `verts` (Recast
    /// space), with neighbours.
    fn tile(verts: &[[f64; 3]], polys: &[(&[u16], &[u16])]) -> Vec<u8> {
        let mut b = vec![0u8; HEADER];
        b[0..2].copy_from_slice(&VERSION.to_le_bytes());
        b[4..6].copy_from_slice(&(polys.len() as u16).to_le_bytes());
        b[6..8].copy_from_slice(&(verts.len() as u16).to_le_bytes());
        for v in verts {
            for c in v {
                b.extend(c.to_le_bytes());
            }
        }
        for (vs, ns) in polys {
            let mut p = vec![0u8; POLY];
            for (k, v) in vs.iter().enumerate() {
                p[4 + k * 2..6 + k * 2].copy_from_slice(&v.to_le_bytes());
                p[16 + k * 2..18 + k * 2].copy_from_slice(&ns[k].to_le_bytes());
            }
            p[30] = vs.len() as u8;
            p[31] = 63;
            b.extend(p);
        }
        b
    }

    #[test]
    fn a_route_goes_round_through_the_portals() {
        // An L of three squares (Recast x, z): A (0..100, 0..100), B (100..200, 0..100),
        // C (100..200, 100..200). From A's middle to C's middle the route must pass B.
        let v = [
            [0.0, 0.0, 0.0],
            [100.0, 0.0, 0.0],
            [100.0, 0.0, 100.0],
            [0.0, 0.0, 100.0],
            [200.0, 0.0, 0.0],
            [200.0, 0.0, 100.0],
            [200.0, 0.0, 200.0],
            [100.0, 0.0, 200.0],
        ];
        let t =
            tile(&v, &[(&[0, 1, 2, 3], &[0, 2, 0, 0]), (&[1, 4, 5, 2], &[0, 0, 3, 1]), (&[2, 5, 6, 7], &[2, 0, 0, 0])]);
        let mesh = NavMesh::from_tiles(&[t]);
        assert_eq!(mesh.len(), 3);
        let (a, b) = ([-50.0, -50.0, 0.0], [-150.0, -150.0, 0.0]);
        let (path, _) = mesh.route(a, b).expect("connected");
        assert_eq!(path.points.first(), Some(&[-50.0, -50.0]));
        assert_eq!(path.points.last(), Some(&[-150.0, -150.0]));
        // It bends at the shared corner (Recast (100, 100) → Unreal (−100, −100)).
        assert!(
            path.points.iter().any(|p| (p[0] + 100.0).abs() < 1.0 && (p[1] + 100.0).abs() < 1.0),
            "{:?}",
            path.points
        );
    }

    #[test]
    fn an_opened_door_joins_the_two_sides() {
        // Two rooms, 0..100 and 140..240 along Recast x, a closed door's gap between.
        let v = [
            [0.0, 0.0, 0.0],
            [1000.0, 0.0, 0.0],
            [1000.0, 0.0, 1000.0],
            [0.0, 0.0, 1000.0],
            [1400.0, 0.0, 0.0],
            [2400.0, 0.0, 0.0],
            [2400.0, 0.0, 1000.0],
            [1400.0, 0.0, 1000.0],
        ];
        let mesh = NavMesh::from_tiles(&[tile(&v, &[(&[0, 1, 2, 3], &[0, 0, 0, 0]), (&[4, 5, 6, 7], &[0, 0, 0, 0])])]);
        let (a, b) = ([-500.0, -500.0, 0.0], [-1900.0, -500.0, 0.0]);
        // Shut: the way stops short and is marked as through something.
        assert!(mesh.route(a, b).is_some_and(|(p, _)| p.uncertain()));
        // Opened (the door in the gap, Unreal (−120, −50)): walked through.
        let open = mesh.bridged(&[([-1200.0, -500.0, 0.0], None)]);
        let (path, _) = open.route(a, b).expect("joined");
        assert!(!path.uncertain());
        assert!(path.points.iter().any(|q| (q[0] + 1200.0).abs() < 10.0));
        // A door where the sides are joined already changes nothing.
        assert_eq!(open.bridged(&[([-1200.0, -500.0, 0.0], None)]).polys[0].links.len(), open.polys[0].links.len());
        // Shut and one-sided, opening from the far room (Unreal x < −120): one way.
        let one = mesh.bridged(&[([-1200.0, -500.0, 0.0], Some([-1700.0, -500.0, 0.0]))]);
        assert!(one.route(b, a).is_some_and(|(p, _)| !p.uncertain()), "from the side it opens from");
        assert!(one.route(a, b).is_some_and(|(p, _)| p.uncertain()), "not from its locked side");
    }

    #[test]
    fn a_shut_one_sided_door_lets_through_from_its_open_side_only() {
        // Two rooms joined across Recast x = 100; the door there, opening from x > 100
        // (Unreal x < −100).
        let v = [
            [0.0, 0.0, 0.0],
            [1000.0, 0.0, 0.0],
            [1000.0, 0.0, 1000.0],
            [0.0, 0.0, 1000.0],
            [2000.0, 0.0, 0.0],
            [2000.0, 0.0, 1000.0],
        ];
        let mesh = NavMesh::from_tiles(&[tile(&v, &[(&[0, 1, 2, 3], &[0, 2, 0, 0]), (&[1, 4, 5, 2], &[0, 0, 0, 1])])]);
        let door = [[-1000.0, -500.0, 0.0], [-1500.0, -500.0, 0.0]];
        let shut = mesh.one_way(&[door]);
        let (locked, open) = ([-500.0, -500.0, 0.0], [-1500.0, -500.0, 0.0]);
        assert!(shut.route(open, locked).is_some_and(|(p, _)| !p.uncertain()), "from the side it opens from");
        assert!(shut.route(locked, open).is_some_and(|(p, _)| p.uncertain()), "not from the locked side");
    }

    #[test]
    fn tiles_join_across_their_border() {
        let a = tile(
            &[[0.0, 0.0, 0.0], [100.0, 0.0, 0.0], [100.0, 0.0, 100.0], [0.0, 0.0, 100.0]],
            &[(&[0, 1, 2, 3], &[0, EXTERNAL, 0, 0])],
        );
        let b = tile(
            &[[100.0, 0.0, 0.0], [200.0, 0.0, 0.0], [200.0, 0.0, 100.0], [100.0, 0.0, 100.0]],
            &[(&[0, 1, 2, 3], &[0, 0, 0, EXTERNAL])],
        );
        let mesh = NavMesh::from_tiles(&[a, b]);
        assert!(mesh.route([-50.0, -50.0, 0.0], [-150.0, -50.0, 0.0]).is_some());
    }

    #[test]
    fn a_point_finds_the_floor_it_stands_on() {
        // Two floors over the same square, 0 and 400 up.
        let v = [
            [0.0, 0.0, 0.0],
            [100.0, 0.0, 0.0],
            [100.0, 0.0, 100.0],
            [0.0, 0.0, 100.0],
            [0.0, 400.0, 0.0],
            [100.0, 400.0, 0.0],
            [100.0, 400.0, 100.0],
            [0.0, 400.0, 100.0],
        ];
        let mesh = NavMesh::from_tiles(&[tile(&v, &[(&[0, 1, 2, 3], &[0; 4]), (&[4, 5, 6, 7], &[0; 4])])]);
        assert_eq!(mesh.locate([-50.0, -50.0, 380.0]).map(|(i, _)| i), Some(1));
        assert_eq!(mesh.locate([-50.0, -50.0, 20.0]).map(|(i, _)| i), Some(0));
    }
}
