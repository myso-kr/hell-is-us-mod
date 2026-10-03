//! The big map over the game window: centred on the hero, a circle measured to the
//! window's short side, fading out toward its edge (as Diablo's and Path of Exile's
//! overlay maps).
//!
//! It is always north up, so walking only slides it: the ground (disc, relief, terrain:
//! most of the work) is drawn larger than the window by a margin, at half size and
//! doubled, then copied at the hero's offset each frame (`Scroll`). The dots are put on
//! that ground once, so they move with the land. What moves on it (trail, things, the
//! route, the hero) is drawn at full size every frame, sharp.
//!
//! The ground is drawn again when the hero is halfway out of the margin, has climbed
//! 1.5 m, or what it is drawn from changed (the game streams the land in as the hero
//! walks, so that is often): on a thread of its own, 80 to 140 ms on a wide screen,
//! while the old ground keeps sliding. Drawn on the overlay's thread it stopped the map
//! that long each time.

use crate::geometry::Footprint;
use crate::minimap::{MapState, View};
use crate::raster::Canvas;
use crate::relief::Relief;
use std::sync::Arc;
use std::thread::JoinHandle;

/// A ground drawn: what it was drawn from, the world point at its centre (its height,
/// the feet it was drawn against) and its pixels.
struct Ground {
    key: u64,
    anchor: [f32; 3],
    cv: Canvas,
}

/// The ground being shown, and the next one being drawn.
#[derive(Default)]
pub struct Scroll {
    ground: Option<Ground>,
    next: Option<JoinHandle<Ground>>,
    /// The footprints' content hash, by the list it was taken from: the list is made
    /// anew whenever any actor comes or goes, mostly with the same footprints.
    prints: (usize, u64),
}

/// What the footprints are, whatever their order: changes only when one does.
fn prints_hash(footprints: &[Footprint]) -> u64 {
    use std::hash::{Hash, Hasher};
    footprints.iter().fold(footprints.len() as u64, |sum, f| {
        let mut k = std::collections::hash_map::DefaultHasher::new();
        (f.corners.map(|c| c.map(f32::to_bits)), f.zmin.to_bits(), f.zmax.to_bits()).hash(&mut k);
        sum.wrapping_add(k.finish())
    })
}

/// How the ground is drawn for one window: its size, margin and scale.
#[derive(Clone, Copy)]
struct Shape {
    w: usize,
    h: usize,
    margin: usize,
    r: f32,
    scale: f32,
}

/// A ground for `shape`, centred on `p`.
fn draw(
    shape: Shape,
    key: u64,
    p: [f32; 3],
    state: &MapState,
    relief: Option<&Relief>,
    footprints: &[Footprint],
) -> Ground {
    let Shape { w, h, margin, r, scale } = shape;
    let (gw, gh) = ((w + 2 * margin + 1) & !1, (h + 2 * margin + 1) & !1);
    let mut half = Canvas::new(gw / 2, gh / 2);
    // Out to where the window's circle can reach while the hero is in the margin.
    let reach = (r + margin as f32 * std::f32::consts::SQRT_2) / 2.0 + 2.0;
    crate::raster::paint_ground(&mut half, state, &view(state, p, 0.0, scale / 2.0), relief, footprints, reach);
    let mut cv = Canvas::new(gw, gh);
    crate::raster::upscale2(&half, &mut cv);
    if state.dots {
        crate::raster::dots(&mut cv, h);
    }
    Ground { key, anchor: p, cv }
}

fn view(state: &MapState, center: [f32; 3], yaw: f32, scale: f32) -> View {
    View {
        center,
        yaw_deg: yaw,
        heading_up: false,
        scale,
        north_deg: state.north_yaw,
        outline: state.big_outline,
        full: true,
    }
}

/// One frame of the big map into `out` (the game window's size, or the panel's preview:
/// with a new `Scroll`, its ground is drawn at once).
#[allow(clippy::too_many_arguments)]
pub fn frame(
    scroll: &mut Scroll,
    out: &mut Canvas,
    state: &MapState,
    world: &str,
    (p, yaw): ([f32; 3], f32),
    things: &[crate::actors::Thing],
    icons: Option<&crate::icons::Icons>,
    footprints: &Arc<Vec<Footprint>>,
    goals: &[crate::goals::Goal],
    path: &crate::pathfind::Path,
    relief: Option<&Arc<Relief>>,
) {
    let (w, h) = (out.w, out.h);
    let r = crate::raster::map_radius(out, true);
    let scale = r / (state.big_radius_m * 100.0);
    // A tenth of the height: about 40 m at the widest radius on any screen.
    let margin = ((h / 10).max(16) + 1) & !1;
    let shape = Shape { w, h, margin, r, scale };
    let ptr = footprints.as_ptr() as usize;
    if scroll.prints.0 != ptr {
        scroll.prints = (ptr, prints_hash(footprints));
    }
    let key = {
        use std::hash::{Hash, Hasher};
        let mut k = std::collections::hash_map::DefaultHasher::new();
        (w, h, margin, scale.to_bits(), state.north_yaw.to_bits(), state.big_outline).hash(&mut k);
        (state.relief.key(), state.opacity[0], state.opacity[1], state.terrain).hash(&mut k);
        relief.map(|rel| (rel.z.as_ptr() as usize, rel.feet.to_bits())).hash(&mut k);
        (scroll.prints.1, state.dots).hash(&mut k);
        k.finish()
    };

    // A ground drawn on its thread is shown from the frame it is ready.
    if scroll.next.as_ref().is_some_and(|n| n.is_finished()) {
        if let Ok(g) = scroll.next.take().unwrap().join() {
            scroll.ground = Some(g);
        }
    }
    let here = view(state, p, yaw, scale);
    // Where the ground's centre is now, from the hero (px).
    let offset = |g: &Ground| {
        let (ax, ay) = here.project(g.anchor);
        (ax.round() as i64, ay.round() as i64)
    };
    let m = margin as i64;
    let usable = |g: &Ground| (g.cv.w, g.cv.h) == ((w + 2 * margin + 1) & !1, (h + 2 * margin + 1) & !1);
    match scroll.ground.as_ref().filter(|g| usable(g)) {
        None => {
            // Nothing to show yet: drawn here, at once.
            scroll.next = None;
            scroll.ground = Some(draw(shape, key, p, state, relief.map(|r| &**r), footprints));
        }
        Some(g) => {
            let (ox, oy) = offset(g);
            let due = g.key != key || (p[2] - g.anchor[2]).abs() > 150.0 || ox.abs() > m / 2 || oy.abs() > m / 2;
            if due && scroll.next.is_none() {
                // Only the ground's part of the settings: the trails and pins stay here.
                let mut state = state.clone();
                state.trails.clear();
                state.markers.clear();
                let (relief, footprints) = (relief.cloned(), footprints.clone());
                scroll.next =
                    Some(std::thread::spawn(move || draw(shape, key, p, &state, relief.as_deref(), &footprints)));
            }
        }
    }

    // The window's pixel (x, y) is the ground's (x - ox + margin, y - oy + margin); past
    // the margin (the next ground not ready yet) the edge is held.
    let g = scroll.ground.as_ref().unwrap();
    let (ox, oy) = offset(g);
    let (sx, sy) = ((m - ox).clamp(0, 2 * m) as usize, (m - oy).clamp(0, 2 * m) as usize);
    for (y, row) in out.px.chunks_mut(w).enumerate() {
        let at = (y + sy) * g.cv.w + sx;
        row.copy_from_slice(&g.cv.px[at..at + w]);
    }
    crate::raster::draw_above(out, state, world, &here, things, icons, goals, path, r);
    crate::raster::fade_edges(out);
}
