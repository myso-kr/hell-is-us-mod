//! A walking route from the hero to the guide's goal: A* over a grid of what stands in
//! the way (.spec/GUIDE.md §6–7).
//!
//! The game keeps no ground navmesh in memory here, so the grid is drawn from the
//! collision shapes of the world's meshes (obstacles.rs): anything at the hero's level
//! — above a step, below head height — blocks; the trail the hero has walked is
//! cheap (it is known to be walkable); steep ground is dear, and ground too steep to
//! walk up (over 45° — the hero cannot jump) blocks; deadly water blocks;
//! everything else is open ground. "The hero's level" is both the hero's own and the
//! ground's where the obstacle stands, so rocks down a slope count too. When no route
//! gets through, a second pass lets the route cross obstacles at a high cost, so a goal
//! walled in by over-wide collision still gets one.
//!
//! The raw 8-way path is then pulled straight wherever the line between two points
//! crosses no wall.

use crate::obstacles::Obstacle;
use crate::terrain::Terrain;
use std::cmp::Reverse;
use std::collections::BinaryHeap;

/// Cell costs, per cell crossed (×1.414 diagonally). The trail is only a little
/// cheaper: at a third of open ground, routes doubled back along it.
const TRAIL: u16 = 2;
const OPEN: u16 = 3;
/// What an obstacle costs when the first pass found no way round.
const WALL: u16 = 400;
/// An obstacle, in the first pass: not crossed at all.
const BLOCK: u16 = u16::MAX;
/// The grid never grows past this many cells a side; cells grow instead.
const MAX_SIDE: usize = 600;
/// Cells are at least this big (cm): half a metre, so doorways stay open.
const MIN_CELL: f32 = 50.0;
/// An obstacle blocks when it rises above a step and reaches below head height,
/// measured from the feet (cm).
const STEP: f32 = 45.0;
const HEAD: f32 = 180.0;
/// Ground steeper than this (rise per run: 35°) costs this much a cell.
const STEEP: (f32, u16) = (0.70, 12);
/// Ground steeper than this (45°, Unreal's walkable floor angle) cannot be walked up,
/// and the hero cannot jump: it blocks, like a wall.
const CLIFF: f32 = 1.0;
/// A floor: thinner than this top to bottom (cm), at least this big from above (cm²).
const FLOOR: (f32, f32) = (150.0, 40_000.0);
/// A floor counts if its top is no higher than this above the ground or the hero (cm).
const REACH_UP: f32 = 150.0;
/// Thinner than this (cm), a shape never blocks: a plate, a decal, a puddle's surface.
const FLAT: f32 = 20.0;
/// Around the hero and the goal, nothing blocks (cm): the goal is often itself solid.
const CLEAR: f32 = 120.0;
/// The hero's radius (cm): cells this close to an obstacle are closed too.
const AGENT: f32 = 35.0;
/// Room around the hero and the goal (cm).
const MARGIN: f32 = 4_000.0;
/// Margins tried in turn when no way round is found inside the last.
const MARGINS: [f32; 3] = [MARGIN, 15_000.0, 40_000.0];

pub struct Grid {
    pub origin: [f32; 2],
    pub cell: f32,
    pub w: usize,
    pub h: usize,
    pub cost: Vec<u16>,
}

impl Grid {
    /// A grid over the box around `a` and `b`.
    pub fn around(a: [f32; 2], b: [f32; 2], margin: f32) -> Grid {
        let (x0, x1) = (a[0].min(b[0]) - margin, a[0].max(b[0]) + margin);
        let (y0, y1) = (a[1].min(b[1]) - margin, a[1].max(b[1]) + margin);
        let cell = ((x1 - x0).max(y1 - y0) / MAX_SIDE as f32).max(MIN_CELL);
        let w = (((x1 - x0) / cell).ceil() as usize).clamp(2, MAX_SIDE);
        let h = (((y1 - y0) / cell).ceil() as usize).clamp(2, MAX_SIDE);
        Grid { origin: [x0, y0], cell, w, h, cost: vec![OPEN; w * h] }
    }

    pub fn cell_of(&self, p: [f32; 2]) -> Option<(usize, usize)> {
        let x = ((p[0] - self.origin[0]) / self.cell).floor();
        let y = ((p[1] - self.origin[1]) / self.cell).floor();
        (x >= 0.0 && y >= 0.0 && (x as usize) < self.w && (y as usize) < self.h).then_some((x as usize, y as usize))
    }

    pub fn centre(&self, c: (usize, usize)) -> [f32; 2] {
        [self.origin[0] + (c.0 as f32 + 0.5) * self.cell, self.origin[1] + (c.1 as f32 + 0.5) * self.cell]
    }

    /// The cells whose centre lies in the convex polygon, or within the hero's radius of
    /// it — so a fence thinner than a cell still closes the cells it runs through.
    fn cells(&self, poly: &[[f32; 2]], inflate: f32) -> Vec<usize> {
        let lo = [
            poly.iter().map(|p| p[0]).fold(f32::MAX, f32::min) - inflate,
            poly.iter().map(|p| p[1]).fold(f32::MAX, f32::min) - inflate,
        ];
        let hi = [
            poly.iter().map(|p| p[0]).fold(f32::MIN, f32::max) + inflate,
            poly.iter().map(|p| p[1]).fold(f32::MIN, f32::max) + inflate,
        ];
        let (gx, gy) = (self.origin[0] + self.w as f32 * self.cell, self.origin[1] + self.h as f32 * self.cell);
        if hi[0] < self.origin[0] || hi[1] < self.origin[1] || lo[0] > gx || lo[1] > gy {
            return Vec::new();
        }
        let to_cell = |v: f32, o: f32, n: usize| (((v - o) / self.cell).floor().max(0.0) as usize).min(n - 1);
        let (x0, x1) = (to_cell(lo[0], self.origin[0], self.w), to_cell(hi[0], self.origin[0], self.w));
        let (y0, y1) = (to_cell(lo[1], self.origin[1], self.h), to_cell(hi[1], self.origin[1], self.h));
        let inside = |p: [f32; 2]| {
            let mut sign = 0.0f32;
            for k in 0..poly.len() {
                let (a, b) = (poly[k], poly[(k + 1) % poly.len()]);
                let side = (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0]);
                if side != 0.0 {
                    if sign != 0.0 && side.signum() != sign {
                        return false;
                    }
                    sign = side.signum();
                }
            }
            true
        };
        let near = |p: [f32; 2]| {
            inflate > 0.0
                && (0..poly.len()).any(|k| {
                    let (a, b) = (poly[k], poly[(k + 1) % poly.len()]);
                    let (ex, ey) = (b[0] - a[0], b[1] - a[1]);
                    let len = (ex * ex + ey * ey).max(f32::EPSILON);
                    let t = (((p[0] - a[0]) * ex + (p[1] - a[1]) * ey) / len).clamp(0.0, 1.0);
                    let (dx, dy) = (p[0] - a[0] - t * ex, p[1] - a[1] - t * ey);
                    dx * dx + dy * dy <= inflate * inflate
                })
        };
        let mut out = Vec::new();
        for y in y0..=y1 {
            for x in x0..=x1 {
                let c = self.centre((x, y));
                if inside(c) || near(c) {
                    out.push(y * self.w + x);
                }
            }
        }
        out
    }

    /// Steep ground as dear; obstacles at the level walked there — or at the hero's
    /// own — and water, as blocks; the trail as cheap ground; and the ground right
    /// around both ends kept open.
    ///
    /// The level walked in a cell is the ground's, or a floor's — something wide and
    /// thin, like a bridge's deck, whose top is within reach of the ground or the hero
    /// (feet at `feet`, cm). So a bridge's piers, below its deck, do not block it.
    pub fn build(
        a: [f32; 2],
        b: [f32; 2],
        margin: f32,
        feet: f32,
        obstacles: &[Obstacle],
        terrain: &Terrain,
        trail: &[[f32; 2]],
    ) -> Grid {
        let mut g = Grid::around(a, b, margin);
        // The ground, and how steep it is.
        let mut level: Vec<f32> = vec![f32::NAN; g.w * g.h];
        if !terrain.is_empty() {
            for y in 0..g.h {
                for x in 0..g.w {
                    let i = y * g.w + x;
                    let c = g.centre((x, y));
                    level[i] = terrain.height(c[0], c[1]).unwrap_or(f32::NAN);
                    if let Some(s) = terrain.slope(c[0], c[1]) {
                        if s > CLIFF {
                            g.cost[i] = BLOCK;
                        } else if s > STEEP.0 {
                            g.cost[i] = STEEP.1;
                        }
                    }
                }
            }
        }
        // Floors raise the level walked.
        let floor = |o: &Obstacle| !o.water && o.zmax - o.zmin < FLOOR.0 && area(&o.hull) >= FLOOR.1;
        for o in obstacles.iter().filter(|o| floor(o)) {
            for i in g.cells(&o.hull, 0.0) {
                let base = if level[i].is_nan() { feet } else { level[i].max(feet) };
                if o.zmax <= base + REACH_UP && (level[i].is_nan() || o.zmax > level[i]) {
                    level[i] = o.zmax;
                }
            }
        }
        // Everything else blocks where it stands in the way.
        let at = |o: &Obstacle, l: f32| o.zmax > l + STEP && o.zmin < l + HEAD;
        for o in obstacles.iter().filter(|o| !floor(o) && (o.water || o.zmax - o.zmin >= FLAT)) {
            for i in g.cells(&o.hull, if o.water { 0.0 } else { AGENT }) {
                if o.water || at(o, feet) || (!level[i].is_nan() && at(o, level[i])) {
                    g.cost[i] = BLOCK;
                }
            }
        }
        for p in trail {
            if let Some((x, y)) = g.cell_of(*p) {
                g.cost[y * g.w + x] = TRAIL;
            }
        }
        let reach = (CLEAR / g.cell).ceil() as i64;
        for end in [a, b] {
            let Some((cx, cy)) = g.cell_of(end) else { continue };
            for dy in -reach..=reach {
                for dx in -reach..=reach {
                    let (x, y) = (cx as i64 + dx, cy as i64 + dy);
                    if x >= 0 && y >= 0 && (x as usize) < g.w && (y as usize) < g.h {
                        let i = y as usize * g.w + x as usize;
                        if g.cost[i] == BLOCK {
                            g.cost[i] = OPEN;
                        }
                    }
                }
            }
        }
        g
    }

    /// The same grid with blocks made merely dear: for when no route gets through.
    fn softened(&self) -> Grid {
        Grid {
            cost: self.cost.iter().map(|&c| if c == BLOCK { WALL } else { c }).collect(),
            origin: self.origin,
            cell: self.cell,
            w: self.w,
            h: self.h,
        }
    }

    /// The cheapest 8-way route between two cells, as cells, or `None` if either end is
    /// off the grid.
    pub fn astar(&self, from: (usize, usize), to: (usize, usize)) -> Option<Vec<(usize, usize)>> {
        let idx = |c: (usize, usize)| c.1 * self.w + c.0;
        let h = |c: (usize, usize)| {
            let (dx, dy) = ((c.0 as i64 - to.0 as i64).unsigned_abs(), (c.1 as i64 - to.1 as i64).unsigned_abs());
            // Octile distance at the cheapest cost: admissible.
            let (lo, hi) = (dx.min(dy), dx.max(dy));
            (hi - lo) * 10 * TRAIL as u64 + lo * 14 * TRAIL as u64
        };
        let mut best = vec![u64::MAX; self.w * self.h];
        let mut came = vec![u32::MAX; self.w * self.h];
        let mut open = BinaryHeap::new();
        best[idx(from)] = 0;
        open.push(Reverse((h(from), 0u64, from.0 as u32, from.1 as u32)));
        while let Some(Reverse((_, g, x, y))) = open.pop() {
            let c = (x as usize, y as usize);
            if c == to {
                let mut path = vec![c];
                let mut i = idx(c);
                while came[i] != u32::MAX {
                    i = came[i] as usize;
                    path.push((i % self.w, i / self.w));
                }
                path.reverse();
                return Some(path);
            }
            if g > best[idx(c)] {
                continue;
            }
            for (dx, dy) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1), (-1, -1), (1, -1), (-1, 1), (1, 1)] {
                let (nx, ny) = (c.0 as i64 + dx, c.1 as i64 + dy);
                if nx < 0 || ny < 0 || nx as usize >= self.w || ny as usize >= self.h {
                    continue;
                }
                let n = (nx as usize, ny as usize);
                if self.cost[idx(n)] == BLOCK {
                    continue;
                }
                // No squeezing diagonally between two blocked corners.
                if dx != 0
                    && dy != 0
                    && (self.cost[idx((nx as usize, c.1))] == BLOCK || self.cost[idx((c.0, ny as usize))] == BLOCK)
                {
                    continue;
                }
                let step = if dx != 0 && dy != 0 { 14 } else { 10 };
                let ng = g + step * self.cost[idx(n)] as u64;
                if ng < best[idx(n)] {
                    best[idx(n)] = ng;
                    came[idx(n)] = idx(c) as u32;
                    open.push(Reverse((ng + h(n), ng, n.0 as u32, n.1 as u32)));
                }
            }
        }
        None
    }

    /// Whether the straight line between two cells crosses nothing dearer than `limit`.
    fn clear(&self, a: (usize, usize), b: (usize, usize), limit: u16) -> bool {
        // Three looks a cell, so the line cannot slip past a corner between samples.
        let steps = 3 * (a.0.abs_diff(b.0)).max(a.1.abs_diff(b.1)).max(1);
        (0..=steps).all(|i| {
            let t = i as f32 / steps as f32;
            let x = (a.0 as f32 + (b.0 as f32 - a.0 as f32) * t).round() as usize;
            let y = (a.1 as f32 + (b.1 as f32 - a.1 as f32) * t).round() as usize;
            self.cost[y * self.w + x] <= limit
        })
    }

    /// The route with its zigzags pulled straight, as world points.
    pub fn simplify(&self, path: &[(usize, usize)]) -> Vec<[f32; 2]> {
        if path.is_empty() {
            return Vec::new();
        }
        let mut out = vec![path[0]];
        let mut i = 0;
        while i < path.len() - 1 {
            let mut j = path.len() - 1;
            while j > i + 1 && !self.clear(path[i], path[j], OPEN) {
                j -= 1;
            }
            out.push(path[j]);
            i = j;
        }
        out.into_iter().map(|c| self.centre(c)).collect()
    }
}

/// A route, and for each of its legs whether it goes through an obstacle — which a
/// route only does when no way round was found.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Path {
    pub points: Vec<[f32; 2]>,
    pub through: Vec<bool>,
}

impl Path {
    /// Some leg goes through something: the way in was not found.
    pub fn uncertain(&self) -> bool {
        self.through.iter().any(|&t| t)
    }
}

/// A polygon's area (cm²).
fn area(p: &[[f32; 2]]) -> f32 {
    (0..p.len()).map(|k| p[k][0] * p[(k + 1) % p.len()][1] - p[(k + 1) % p.len()][0] * p[k][1]).sum::<f32>().abs() / 2.0
}

/// Whether the straight leg from `a` to `b` crosses a blocked cell of `g`.
fn crosses(g: &Grid, a: [f32; 2], b: [f32; 2]) -> bool {
    let len = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
    let steps = ((len / g.cell) * 3.0).ceil().max(1.0) as usize;
    (0..=steps).any(|i| {
        let t = i as f32 / steps as f32;
        g.cell_of([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t])
            .is_some_and(|(x, y)| g.cost[y * g.w + x] == BLOCK)
    })
}

/// A route from `a` to `b` (world X/Y, cm), its ends exactly at `a` and `b`: round the
/// obstacles if there is a way, through the fewest of them if there is not.
pub fn route(
    a: [f32; 2],
    b: [f32; 2],
    feet: f32,
    obstacles: &[Obstacle],
    terrain: &Terrain,
    trail: &[[f32; 2]],
) -> Path {
    let straight = || Path { points: vec![a, b], through: vec![false] };
    // The way round may leave the box around the two ends: look wider before giving up.
    let mut found = None;
    let mut strict = Grid::build(a, b, MARGIN, feet, obstacles, terrain, trail);
    for margin in MARGINS {
        if margin != MARGIN {
            strict = Grid::build(a, b, margin, feet, obstacles, terrain, trail);
        }
        let (Some(s), Some(t)) = (strict.cell_of(a), strict.cell_of(b)) else { return straight() };
        found = strict.astar(s, t);
        if found.is_some() {
            break;
        }
    }
    let (Some(s), Some(t)) = (strict.cell_of(a), strict.cell_of(b)) else { return straight() };
    let (g, cells) = match found {
        Some(c) => (None, c),
        None => {
            let soft = strict.softened();
            match soft.astar(s, t) {
                Some(c) => (Some(soft), c),
                None => return straight(),
            }
        }
    };
    let mut points = g.as_ref().unwrap_or(&strict).simplify(&cells);
    if let Some(first) = points.first_mut() {
        *first = a;
    }
    if let Some(last) = points.last_mut() {
        *last = b;
    }
    let through = points.windows(2).map(|w| g.is_some() && crosses(&strict, w[0], w[1])).collect();
    Path { points, through }
}

/// A route's length (cm).
pub fn length(route: &[[f32; 2]]) -> f32 {
    route.windows(2).map(|w| ((w[1][0] - w[0][0]).powi(2) + (w[1][1] - w[0][1]).powi(2)).sqrt()).sum()
}

/// How far `from` is off the route, and how much of the route is left from the
/// point on it nearest `from` (cm).
pub fn remaining(route: &[[f32; 2]], from: [f32; 2]) -> Option<(f32, f32)> {
    let mut best: Option<(f32, f32)> = None;
    for i in 0..route.len().saturating_sub(1) {
        let (a, b) = (route[i], route[i + 1]);
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let len2 = (dx * dx + dy * dy).max(f32::EPSILON);
        let t = (((from[0] - a[0]) * dx + (from[1] - a[1]) * dy) / len2).clamp(0.0, 1.0);
        let q = [a[0] + dx * t, a[1] + dy * t];
        let off = (from[0] - q[0]).hypot(from[1] - q[1]);
        if best.is_none_or(|(o, _)| off < o) {
            let left = (b[0] - q[0]).hypot(b[1] - q[1]) + length(&route[i + 1..]);
            best = Some((off, left));
        }
    }
    best
}

/// Whether a fresh route should replace the one being followed. It does when it
/// heads the same way from here (within 100°) — so the route keeps up with the hero —
/// and when the hero has strayed from the old one. One that turns the hero round only
/// when it is clearly shorter, or the old one crossed obstacles and it does not:
/// otherwise two near-equal ways round trade places on every recompute and the
/// compass swings from ahead to behind and back.
pub fn better(old: &Path, new: &Path, from: [f32; 2], ahead: f32) -> bool {
    const STRAYED: f32 = 600.0;
    const SHORTER: f32 = 0.85;
    const SAME_WAY: f32 = 100.0;
    let Some((off, left)) = remaining(&old.points, from) else { return true };
    if off > STRAYED || length(&new.points) < (off + left) * SHORTER || (old.uncertain() && !new.uncertain()) {
        return true;
    }
    let heading = |p: &Path| {
        let q = next_point(&p.points, from, ahead)?;
        Some((q[1] - from[1]).atan2(q[0] - from[0]).to_degrees())
    };
    match (heading(old), heading(new)) {
        (Some(a), Some(b)) => ((b - a + 540.0).rem_euclid(360.0) - 180.0).abs() <= SAME_WAY,
        _ => true,
    }
}

/// How much further than the nearest segment a later one may be and still count as
/// where the hero is (cm).
const PASSED: f32 = 300.0;

/// The point to head for now: from the route segment nearest `from`, the first point
/// further along that is at least `ahead` cm away — or the end.
pub fn next_point(route: &[[f32; 2]], from: [f32; 2], ahead: f32) -> Option<[f32; 2]> {
    let d2 = |a: [f32; 2], b: [f32; 2]| (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2);
    let to_segment = |a: [f32; 2], b: [f32; 2]| {
        let len = d2(a, b).max(f32::EPSILON);
        let t = (((from[0] - a[0]) * (b[0] - a[0]) + (from[1] - a[1]) * (b[1] - a[1])) / len).clamp(0.0, 1.0);
        d2(from, [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t])
    };
    // The furthest-along segment about as near as the nearest: a corner cut short is
    // passed, not pointed back at.
    let near: Vec<f32> = (0..route.len().saturating_sub(1)).map(|i| to_segment(route[i], route[i + 1])).collect();
    let best = near.iter().copied().fold(f32::INFINITY, f32::min);
    let slack = (best.sqrt() + PASSED).powi(2);
    let seg = near.iter().rposition(|&d| d <= slack)?;
    route[seg + 1..].iter().find(|p| d2(**p, from) >= ahead * ahead).or(route.last()).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An axis-aligned block: centre, half extents (cm), standing at height z0..z1.
    fn block(cx: f32, cy: f32, hx: f32, hy: f32, z0: f32, z1: f32) -> Obstacle {
        Obstacle {
            hull: vec![[cx - hx, cy - hy], [cx + hx, cy - hy], [cx + hx, cy + hy], [cx - hx, cy + hy]],
            zmin: z0,
            zmax: z1,
            water: false,
        }
    }

    #[test]
    fn a_route_is_kept_unless_the_new_one_is_clearly_better() {
        let old = Path { points: vec![[0.0, 0.0], [1000.0, 0.0], [1000.0, 1000.0]], through: vec![false, false] };
        let (off, left) = remaining(&old.points, [500.0, 100.0]).unwrap();
        assert_eq!((off, left), (100.0, 1500.0));
        let back = Path { points: vec![[500.0, 100.0], [0.0, 100.0], [0.0, 1000.0], [1000.0, 1000.0]], through: vec![false; 3] };
        assert!(!better(&old, &back, [500.0, 100.0], 300.0), "turns the hero round, not shorter: keep the old one");
        let same = Path { points: vec![[500.0, 100.0], [1000.0, 100.0], [1000.0, 1000.0]], through: vec![false; 2] };
        assert!(better(&old, &same, [500.0, 100.0], 300.0), "the same way: take it, it is fresher");
        let short = Path { points: vec![[500.0, 100.0], [1000.0, 1000.0]], through: vec![false] };
        assert!(better(&old, &short, [500.0, 100.0], 300.0), "clearly shorter: take it");
        assert!(better(&old, &back, [500.0, 3000.0], 300.0), "strayed far from the old one: take the new one");
    }

    #[test]
    fn a_corner_cut_short_is_passed() {
        let r = [[0.0, 0.0], [1000.0, 0.0], [1000.0, 1000.0]];
        // Cutting across, nearer the second leg's start than the first leg's end.
        assert_eq!(next_point(&r, [900.0, 150.0], 100.0), Some([1000.0, 1000.0]));
    }

    /// A wall `2·half_len` long along Y, 1 m thick, 3 m tall, on the hero's level.
    fn wall(x: f32, y: f32, half_len: f32) -> Obstacle {
        block(x, y, 50.0, half_len, 0.0, 300.0)
    }

    #[test]
    fn a_way_round_is_certain() {
        let p = route([0.0, 0.0], [3000.0, 0.0], 0.0, &[wall(1500.0, 0.0, 1000.0)], &Terrain::default(), &[]);
        assert!(!p.uncertain());
    }

    #[test]
    fn open_ground_is_a_straight_line() {
        let r = route([0.0, 0.0], [3000.0, 0.0], 0.0, &[], &Terrain::default(), &[]).points;
        assert_eq!(r.len(), 2, "{r:?}");
        assert!((length(&r) - 3000.0).abs() < 1.0);
    }

    #[test]
    fn a_wall_in_the_way_is_walked_around() {
        let w = wall(1500.0, 0.0, 1000.0);
        let r = route([0.0, 0.0], [3000.0, 0.0], 0.0, &[w], &Terrain::default(), &[]).points;
        assert!(r.len() >= 3, "a bend: {r:?}");
        assert!(length(&r) > 3000.0 + 500.0, "longer than straight: {}", length(&r));
        // Nowhere does the route cross the wall's span (y within ±10 m at x 14.5..15.5 m).
        for s in r.windows(2) {
            let (a, b) = (s[0], s[1]);
            if (a[0] - 1500.0).signum() != (b[0] - 1500.0).signum() {
                let t = (1500.0 - a[0]) / (b[0] - a[0]);
                let y = a[1] + (b[1] - a[1]) * t;
                assert!(y.abs() >= 1000.0, "crossed the wall at y = {y}");
            }
        }
    }

    #[test]
    fn a_walled_in_goal_still_gets_a_route() {
        // A box of four walls around the goal.
        let walls = [
            wall(2500.0, 3000.0, 600.0),
            wall(3500.0, 3000.0, 600.0),
            block(3000.0, 2500.0, 600.0, 50.0, 0.0, 300.0),
            block(3000.0, 3500.0, 600.0, 50.0, 0.0, 300.0),
        ];
        let p = route([0.0, 0.0], [3000.0, 3000.0], 0.0, &walls, &Terrain::default(), &[]);
        assert_eq!(p.points.last(), Some(&[3000.0, 3000.0]));
        assert!(p.uncertain(), "it had to go through a wall, and says so");
    }

    #[test]
    fn walls_on_another_floor_do_not_block() {
        // The same wall, 10 m above the hero.
        let w = block(1500.0, 0.0, 50.0, 1000.0, 1000.0, 1300.0);
        let r = route([0.0, 0.0], [3000.0, 0.0], 0.0, &[w], &Terrain::default(), &[]).points;
        assert_eq!(r.len(), 2);
    }

    #[test]
    fn a_fence_thinner_than_a_cell_still_blocks() {
        // 10 cm thick, 20 m long, across the way: no cell centre falls inside it.
        let fence = block(1525.0, 0.0, 5.0, 1000.0, 0.0, 120.0);
        let r = route([0.0, 0.0], [3000.0, 0.0], 0.0, &[fence], &Terrain::default(), &[]).points;
        assert!(r.len() >= 3, "goes round: {r:?}");
        for s in r.windows(2) {
            let (a, b) = (s[0], s[1]);
            if (a[0] - 1525.0).signum() != (b[0] - 1525.0).signum() {
                let t = (1525.0 - a[0]) / (b[0] - a[0]);
                let y = a[1] + (b[1] - a[1]) * t;
                assert!(y.abs() >= 1000.0, "crossed the fence at y = {y}");
            }
        }
    }

    #[test]
    fn steep_ground_is_gone_round() {
        // Ground from y = -20 m to 40 m, flat but for a ridge across the way at x 10..14 m,
        // 6 m high (steeper than a cliff), from y = -10 m to 30 m.
        let n = 61;
        let z = (0..n * n)
            .map(|i| {
                let (x, y) = (i % n, i / n);
                let ridge = (10..=14).contains(&x) && (10..50).contains(&y);
                if ridge {
                    600.0 * (1.0 - ((x as f32 - 12.0).abs() / 2.0))
                } else {
                    0.0
                }
            })
            .collect();
        let field = crate::terrain::Heightfield { origin: [0.0, -2000.0, 0.0], spacing: [100.0, 100.0], n, z };
        let terrain = Terrain::new(vec![field]);
        let p = route([500.0, 1000.0], [2500.0, 1000.0], 0.0, &[], &terrain, &[]);
        for s in p.points.windows(2) {
            let (a, b) = (s[0], s[1]);
            if (a[0] - 1200.0).signum() != (b[0] - 1200.0).signum() {
                let y = a[1] + (b[1] - a[1]) * (1200.0 - a[0]) / (b[0] - a[0]);
                assert!(!(-900.0..2900.0).contains(&y), "crossed the ridge at y = {y}: {:?}", p.points);
            }
        }
        assert!(!p.uncertain());
    }

    #[test]
    fn a_bridge_is_walked_over_its_piers() {
        // Ground at -500 (a river bed); the hero on a 10 m deck at z 0..40 (top 40),
        // feet at 40; under it, piers from the bed to the deck's underside.
        let n = 41;
        let field = crate::terrain::Heightfield {
            origin: [-1000.0, -2000.0, 0.0],
            spacing: [100.0, 100.0],
            n,
            z: vec![-500.0; n * n],
        };
        let terrain = Terrain::new(vec![field]);
        let deck = block(500.0, 0.0, 500.0, 150.0, 0.0, 40.0);
        let piers: Vec<Obstacle> =
            [200.0, 500.0, 800.0].iter().map(|&x| block(x, 0.0, 40.0, 150.0, -500.0, 0.0)).collect();
        let mut all = piers;
        all.push(deck);
        let p = route([0.0, 0.0], [1000.0, 0.0], 40.0, &all, &terrain, &[]);
        assert_eq!(p.points.len(), 2, "straight over the deck: {:?}", p.points);
        assert!(!p.uncertain());
    }

    #[test]
    fn a_low_kerb_is_stepped_over() {
        let kerb = block(1500.0, 0.0, 50.0, 1000.0, 0.0, 30.0);
        assert_eq!(route([0.0, 0.0], [3000.0, 0.0], 0.0, &[kerb], &Terrain::default(), &[]).points.len(), 2);
    }

    #[test]
    fn the_next_point_is_ahead_on_the_route() {
        let r = [[0.0, 0.0], [100.0, 0.0], [2000.0, 0.0], [2000.0, 2000.0]];
        assert_eq!(next_point(&r, [0.0, 0.0], 500.0), Some([2000.0, 0.0]));
        assert_eq!(next_point(&r, [1900.0, 0.0], 500.0), Some([2000.0, 2000.0]));
    }
}
