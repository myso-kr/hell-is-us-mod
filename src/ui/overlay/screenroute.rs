//! The route in focus drawn into the game's view (MAP.md §17): a band on the floor ahead of the
//! hero, in perspective, as a game's own guiding line is — and hidden where the world hides it.
//!
//! The overlay cannot read the game's depth buffer. It holds the world's shape as the routes use
//! it, though (obstacles.rs, terrain.rs): each point of the band is seen from the game's camera
//! only when the line to it crosses no obstacle (a convex outline over a height range, clipped
//! like a prism) and does not go under the landscape. Underground, the landscape is overhead
//! everywhere and is left out of the test; walls and floors are obstacles there.

use super::marker;
use crate::obstacles::{Obstacle, Scene};
use crate::raster::Canvas;
use crate::ui::layered::Layered;
use std::collections::HashMap;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// How far ahead of the hero the band goes (cm), its width, how often it is sampled.
const AHEAD: f32 = 8000.0;
const HALF: f32 = 14.0;
/// The band starts this far along (cm): under the camera it would fill the view.
const FROM: f32 = 400.0;
/// Icons over things this near the hero (cm), at most this many, this big (px).
const MARKS_NEAR: f32 = 5000.0;
const MARKS_MAX: usize = 40;
const STEP: f32 = 50.0;
/// Over the floor it lies on (cm): clear of it, not floating.
const LIFT: f32 = 8.0;
/// The last stretch before a point is not tested: the floor under it would hide it.
const NEAR_END: f32 = 120.0;
/// The window grows and shrinks in steps this big (px), not every frame.
const BUCKET: i32 = 128;
/// The layer's own pace: the camera turns between the overlay's frames (50 ms), and a band on the
/// floor that lags the view by that much jerks.
const PAINT_EVERY: Duration = Duration::from_millis(16);
/// What is seen from the camera is worked out again when the camera has moved this far (cm), or
/// this long after: the costly part (rays against the obstacles and the ground), not the drawing.
const SEEN_MOVED: f32 = 25.0;
const SEEN_FOR: Duration = Duration::from_millis(120);

/// A piece of the band on the screen: its corners, its opacity, whether a chevron.
type Quad = ([(f32, f32); 4], f32, bool);
/// A sample's band edges on the screen (left, right) and whether it is seen.
type Edge = ((f32, f32), (f32, f32), bool);

/// The obstacles with their outlines' bounds, kept while the scene is the same.
struct Indexed {
    scene: Arc<Scene>,
    boxes: Vec<[f32; 4]>,
}

pub struct ScreenRoute {
    window: Option<Layered>,
    canvas: Canvas,
    index: Option<Indexed>,
    /// The maps' icons, rasterised at each size a distance gives them (px → set).
    icons: HashMap<usize, crate::icons::Icons>,
    icons_px: u8,
    /// Whether a point (on a 25 cm grid) is seen from the camera at `seen_eye`, as of `seen_at`.
    seen: HashMap<[i32; 3], bool>,
    seen_eye: [f32; 3],
    seen_at: Option<Instant>,
}

impl Default for ScreenRoute {
    fn default() -> Self {
        ScreenRoute {
            window: None,
            canvas: Canvas::new(1, 1),
            index: None,
            icons: Default::default(),
            icons_px: 0,
            seen: HashMap::new(),
            seen_eye: [0.0; 3],
            seen_at: None,
        }
    }
}

/// What the game view's layer is to draw, as the overlay last worked it out (at its pace).
pub struct Job {
    /// Where the camera is read from, in the game's process `pid`: the layer reads it itself.
    pub src: crate::player::PoseSource,
    pub pid: u32,
    pub client: (i32, i32, i32, i32),
    pub route: Vec<[f32; 3]>,
    pub scene: Arc<Scene>,
    pub colour: [u8; 3],
    pub hero: [f32; 3],
    pub things: Vec<crate::actors::Thing>,
    pub icon_px: u8,
}

/// The game view's layer on a thread of its own, drawn every `PAINT_EVERY` from the camera as it
/// is then: the overlay hands it what to draw (`set`) at its own, slower pace.
pub struct Painter {
    job: Arc<Mutex<Option<Arc<Job>>>>,
    panel: Arc<AtomicIsize>,
}

impl Painter {
    pub fn spawn() -> Painter {
        let job: Arc<Mutex<Option<Arc<Job>>>> = Arc::new(Mutex::new(None));
        let panel = Arc::new(AtomicIsize::new(0));
        let (j, p) = (job.clone(), panel.clone());
        let _ = std::thread::Builder::new().name("screen3d".into()).spawn(move || paint(j, p));
        Painter { job, panel }
    }

    /// What to draw from now on; `None` hides the layer.
    pub fn set(&self, job: Option<Job>) {
        *self.job.lock().unwrap() = job.map(Arc::new);
    }

    /// Kept under the panel while it shows (`Some`), else on top.
    pub fn keep_under(&self, panel: Option<windows_sys::Win32::Foundation::HWND>) {
        self.panel.store(panel.map_or(0, |h| h as isize), Ordering::Relaxed);
    }
}

fn paint(job: Arc<Mutex<Option<Arc<Job>>>>, panel: Arc<AtomicIsize>) {
    let mut layer = ScreenRoute::default();
    let mut reader: Option<crate::game::process::Reader> = None;
    let mut tick = 0u32;
    loop {
        let start = Instant::now();
        crate::ui::layered::pump();
        let now = job.lock().unwrap().clone();
        match now {
            Some(j) => {
                if reader.as_ref().map(|r| r.pid) != Some(j.pid) {
                    reader = crate::game::process::Reader::open(j.pid);
                }
                match reader.as_ref().and_then(|r| j.src.camera(r)) {
                    Some(cam) => layer.draw(&cam, j.client, &j.route, &j.scene, j.colour, j.hero, &j.things, j.icon_px),
                    None => layer.hide(),
                }
            }
            None => layer.hide(),
        }
        if tick % 30 == 0 {
            let p = panel.load(Ordering::Relaxed);
            layer.keep_on_top((p != 0).then_some(p as windows_sys::Win32::Foundation::HWND));
        }
        tick = tick.wrapping_add(1);
        std::thread::sleep(PAINT_EVERY.saturating_sub(start.elapsed()).max(Duration::from_millis(2)));
    }
}

impl ScreenRoute {
    pub fn hide(&mut self) {
        if let Some(w) = self.window.as_mut() {
            w.hide();
        }
    }

    pub fn keep_on_top(&self, below: Option<windows_sys::Win32::Foundation::HWND>) {
        if let Some(w) = self.window.as_ref() {
            w.keep_on_top(below);
        }
    }

    /// Draw `route` (cm, from the hero's feet) as seen from `cam` into the game's client area
    /// `client` (left, top, width, height on the screen), in `colour`.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        cam: &crate::player::Camera,
        client: (i32, i32, i32, i32),
        route: &[[f32; 3]],
        scene: &Arc<Scene>,
        colour: [u8; 3],
        hero: [f32; 3],
        things: &[crate::actors::Thing],
        icon_px: u8,
    ) {
        // the icons are made again when the maps' size (MapState::icon_px) changes
        if self.icons_px != icon_px {
            self.icons.clear();
            self.icons_px = icon_px;
        }
        if !self.index.as_ref().is_some_and(|i| Arc::ptr_eq(&i.scene, scene)) {
            let boxes = scene.obstacles.iter().map(bounds).collect();
            self.index = Some(Indexed { scene: scene.clone(), boxes });
        }
        let index = self.index.as_ref().unwrap();
        let (left, top, w, h) = client;
        let eye = [cam.at[0] as f32, cam.at[1] as f32, cam.at[2] as f32];
        // what is seen, kept while the camera stays put: the view may turn, the rays are the same
        let moved = (0..3).map(|k| (eye[k] - self.seen_eye[k]).powi(2)).sum::<f32>().sqrt();
        if moved > SEEN_MOVED || self.seen_at.is_none_or(|t| t.elapsed() > SEEN_FOR) {
            self.seen.clear();
            self.seen_eye = eye;
            self.seen_at = Some(Instant::now());
        }
        let cache = &mut self.seen;
        let mut seen_from = |at: [f32; 3]| {
            let key = [(at[0] / 25.0).round() as i32, (at[1] / 25.0).round() as i32, (at[2] / 25.0).round() as i32];
            *cache.entry(key).or_insert_with(|| visible(eye, at, scene, &index.boxes))
        };
        let samples = resample(route, STEP, AHEAD);
        // each sample: seen?, and its band's two edges on the screen
        let mut quads: Vec<Quad> = Vec::new();
        let mut prev: Option<Edge> = None;
        for (k, s) in samples.iter().enumerate() {
            let (p, dir, along) = (s.0, s.1, s.2);
            let side = [-dir[1] * HALF, dir[0] * HALF];
            let at = [p[0], p[1], p[2] + LIFT];
            let seen = seen_from(at);
            let l = marker::project(cam, [at[0] + side[0], at[1] + side[1], at[2]], w as f32, h as f32);
            let r = marker::project(cam, [at[0] - side[0], at[1] - side[1], at[2]], w as f32, h as f32);
            let cur = match (l, r) {
                (Some(l), Some(r)) => Some(((l.0, l.1), (r.0, r.1), seen)),
                _ => None,
            };
            if let (Some((pl, pr, ps)), Some((cl, cr, cs))) = (prev, cur) {
                if ps && cs && k > 0 && along > FROM {
                    // fading out ahead, a brighter chevron every 3 m
                    let fade = 1.0 - (along / AHEAD).clamp(0.0, 1.0) * 0.7;
                    let chevron = (along / 300.0).fract() < 0.2;
                    quads.push(([pl, pr, cr, cl], fade * if chevron { 0.85 } else { 0.5 }, chevron));
                }
            }
            prev = cur;
        }
        // the maps' icons over what is near and seen, the nearest first
        let mut near: Vec<(f32, &crate::actors::Thing)> = things
            .iter()
            .map(|t| {
                (((t.at[0] - hero[0]).powi(2) + (t.at[1] - hero[1]).powi(2) + (t.at[2] - hero[2]).powi(2)).sqrt(), t)
            })
            .filter(|(d, _)| *d < MARKS_NEAR && *d > 150.0)
            .collect();
        near.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut marks: Vec<((f32, f32), crate::actors::Sub, f32, usize)> = Vec::new();
        for (d, t) in near {
            if marks.len() >= MARKS_MAX {
                break;
            }
            // over the thing's head, seen from the camera
            let at = [t.at[0], t.at[1], t.at[2] + 120.0];
            if !seen_from(at) {
                continue;
            }
            let Some(p) = marker::project(cam, at, w as f32, h as f32) else { continue };
            if p.0 < 0.0
                || p.1 < 0.0
                || p.0 > w as f32
                || p.1 > h as f32
                || marks.iter().any(|m| (m.0 .0 - p.0).hypot(m.0 .1 - p.1) < 20.0)
            {
                continue;
            }
            // as the compass draws its pins (compass::pin_look): a little large near, smaller
            // and fainter far, on a log scale over 10–200 m; sizes in steps of 2 px
            let (scale, alpha) = crate::map::compass::pin_look(d / 100.0, false);
            let size = (((icon_px as f32 * scale) / 2.0).round() as usize * 2).max(8);
            marks.push(((p.0, p.1), t.sub, alpha, size));
        }
        if quads.is_empty() && marks.is_empty() {
            self.hide();
            return;
        }
        // the window over the band's and the icons' bounds, on the client area, in buckets
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for (p, _, _, size) in &marks {
            let half = *size as f32 / 2.0 + 2.0;
            x0 = x0.min(p.0 - half);
            y0 = y0.min(p.1 - half);
            x1 = x1.max(p.0 + half);
            y1 = y1.max(p.1 + half);
        }
        for (q, _, _) in &quads {
            for p in q {
                x0 = x0.min(p.0);
                y0 = y0.min(p.1);
                x1 = x1.max(p.0);
                y1 = y1.max(p.1);
            }
        }
        let (x0, y0) = ((x0.floor() as i32 - 4).clamp(0, w), (y0.floor() as i32 - 4).clamp(0, h));
        let (x1, y1) = ((x1.ceil() as i32 + 4).clamp(0, w), (y1.ceil() as i32 + 4).clamp(0, h));
        if x1 <= x0 || y1 <= y0 {
            self.hide();
            return;
        }
        let (bw, bh) = (((x1 - x0) / BUCKET + 1) * BUCKET, ((y1 - y0) / BUCKET + 1) * BUCKET);
        let (bw, bh) = (bw.min(w), bh.min(h));
        if self.window.as_ref().is_none_or(|win| win.w != bw || win.h != bh) {
            self.window = Layered::new("hiumod-route", "Hell Is Us Route", bw, bh);
            self.canvas = Canvas::new(bw as usize, bh as usize);
        }
        let Some(win) = self.window.as_mut() else { return };
        self.canvas.clear();
        let (ox, oy) = (x0 as f32, y0 as f32);
        for (q, a, chevron) in &quads {
            let pts: Vec<(f32, f32)> = q.iter().map(|p| (p.0 - ox, p.1 - oy)).collect();
            let alpha = (a * 255.0) as u8;
            let c = if *chevron {
                crate::map::canvas::Rgba(255, 244, 214, alpha)
            } else {
                crate::map::canvas::Rgba(colour[0], colour[1], colour[2], alpha)
            };
            self.canvas.polygon(&pts, c);
        }
        // the far first, so the near sit on top
        for (p, sub, a, size) in marks.iter().rev() {
            let icons = match self.icons.entry(*size) {
                std::collections::hash_map::Entry::Occupied(e) => e.into_mut(),
                std::collections::hash_map::Entry::Vacant(e) => match crate::icons::Icons::new(*size) {
                    Ok(i) => e.insert(i),
                    Err(_) => continue,
                },
            };
            let icon = icons.get(*sub);
            self.canvas.blit_alpha(p.0 - ox, p.1 - oy, icon.size, &icon.px, (a * 255.0) as u32);
        }
        win.present(&self.canvas, left + x0, top + y0);
    }
}

/// An outline's bounds: min x, min y, max x, max y.
fn bounds(o: &Obstacle) -> [f32; 4] {
    o.hull.iter().fold([f32::MAX, f32::MAX, f32::MIN, f32::MIN], |b, p| {
        [b[0].min(p[0]), b[1].min(p[1]), b[2].max(p[0]), b[3].max(p[1])]
    })
}

/// The route every `step` cm up to `ahead` along it: each point, the way it goes (unit, x/y),
/// and how far along it is.
fn resample(route: &[[f32; 3]], step: f32, ahead: f32) -> Vec<([f32; 3], [f32; 2], f32)> {
    let mut out = Vec::new();
    let mut along = 0.0;
    for seg in route.windows(2) {
        let (a, b) = (seg[0], seg[1]);
        let (dx, dy, dz) = (b[0] - a[0], b[1] - a[1], b[2] - a[2]);
        let len = (dx * dx + dy * dy).sqrt();
        if len < 1.0 {
            continue;
        }
        let dir = [dx / len, dy / len];
        let mut t = 0.0;
        while t < len {
            out.push(([a[0] + dir[0] * t, a[1] + dir[1] * t, a[2] + dz * t / len], dir, along + t));
            if along + t > ahead {
                return out;
            }
            t += step;
        }
        along += len;
    }
    if let (Some(last), Some(prev)) = (route.last(), out.last()) {
        out.push((*last, prev.1, along));
    }
    out
}

/// Whether `p` is seen from `eye`: the line between them, short of `p`'s own floor, crosses no
/// obstacle and (both above the ground) stays over the landscape.
fn visible(eye: [f32; 3], p: [f32; 3], scene: &Scene, boxes: &[[f32; 4]]) -> bool {
    let d = [p[0] - eye[0], p[1] - eye[1], p[2] - eye[2]];
    let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    if len < NEAR_END * 2.0 {
        return true;
    }
    let end = 1.0 - NEAR_END / len;
    let at = |t: f32| [eye[0] + d[0] * t, eye[1] + d[1] * t, eye[2] + d[2] * t];
    // the landscape, when neither end is under it
    let above = |q: [f32; 3]| scene.terrain.height(q[0], q[1]).is_none_or(|g| q[2] >= g - 50.0);
    if above(eye) && above(p) {
        let n = (len * end / 100.0).ceil().max(1.0) as usize;
        for k in 1..n {
            let q = at(end * k as f32 / n as f32);
            if scene.terrain.height(q[0], q[1]).is_some_and(|g| g > q[2] + 20.0) {
                return false;
            }
        }
    }
    // the obstacles: the segment's part over each outline, at its heights there
    let (sx0, sy0) = (eye[0].min(at(end)[0]), eye[1].min(at(end)[1]));
    let (sx1, sy1) = (eye[0].max(at(end)[0]), eye[1].max(at(end)[1]));
    for (o, b) in scene.obstacles.iter().zip(boxes) {
        if o.water || b[2] < sx0 || b[0] > sx1 || b[3] < sy0 || b[1] > sy1 {
            continue;
        }
        if let Some((t0, t1)) = clip(&o.hull, eye, d, 0.0, end) {
            let (z0, z1) = (eye[2] + d[2] * t0, eye[2] + d[2] * t1);
            if z0.min(z1) <= o.zmax && z0.max(z1) >= o.zmin {
                return false;
            }
        }
    }
    true
}

/// The part [t0, t1] of the segment `a + t·d` (t in `lo..hi`) over a convex outline (x/y), if any
/// (Cyrus–Beck).
fn clip(hull: &[[f32; 2]], a: [f32; 3], d: [f32; 3], lo: f32, hi: f32) -> Option<(f32, f32)> {
    let n = hull.len();
    if n < 3 {
        return None;
    }
    // inward normals: the outline's winding from its signed area
    let area: f32 = (0..n).map(|k| hull[k][0] * hull[(k + 1) % n][1] - hull[(k + 1) % n][0] * hull[k][1]).sum();
    let sign = if area >= 0.0 { 1.0 } else { -1.0 };
    let (mut t0, mut t1) = (lo, hi);
    for k in 0..n {
        let (p, q) = (hull[k], hull[(k + 1) % n]);
        let normal = [-(q[1] - p[1]) * sign, (q[0] - p[0]) * sign];
        let num = normal[0] * (a[0] - p[0]) + normal[1] * (a[1] - p[1]);
        let den = normal[0] * d[0] + normal[1] * d[1];
        if den.abs() < 1e-9 {
            if num < 0.0 {
                return None;
            }
            continue;
        }
        let t = -num / den;
        if den > 0.0 {
            t0 = t0.max(t);
        } else {
            t1 = t1.min(t);
        }
        if t0 > t1 {
            return None;
        }
    }
    Some((t0, t1))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(x0: f32, y0: f32, s: f32) -> Vec<[f32; 2]> {
        vec![[x0, y0], [x0 + s, y0], [x0 + s, y0 + s], [x0, y0 + s]]
    }

    #[test]
    fn a_line_through_a_box_is_clipped_to_it() {
        let (t0, t1) = clip(&square(40.0, -10.0, 20.0), [0.0, 0.0, 0.0], [100.0, 0.0, 0.0], 0.0, 1.0).unwrap();
        assert!((t0 - 0.4).abs() < 1e-4 && (t1 - 0.6).abs() < 1e-4);
        assert!(clip(&square(40.0, 20.0, 20.0), [0.0, 0.0, 0.0], [100.0, 0.0, 0.0], 0.0, 1.0).is_none(), "beside it");
        // either winding
        let mut cw = square(40.0, -10.0, 20.0);
        cw.reverse();
        assert!(clip(&cw, [0.0, 0.0, 0.0], [100.0, 0.0, 0.0], 0.0, 1.0).is_some());
    }

    #[test]
    fn a_wall_hides_what_is_behind_it_and_not_what_is_over_it() {
        let wall = Obstacle {
            hull: vec![[400.0, -500.0], [450.0, -500.0], [450.0, 500.0], [400.0, 500.0]],
            zmin: 0.0,
            zmax: 300.0,
            water: false,
        };
        let scene = Scene { obstacles: vec![wall.clone()], ..Default::default() };
        let boxes = vec![bounds(&wall)];
        assert!(!visible([0.0, 0.0, 150.0], [1000.0, 0.0, 10.0], &scene, &boxes), "behind the wall");
        assert!(visible([0.0, 0.0, 1500.0], [1000.0, 0.0, 10.0], &scene, &boxes), "seen over it from high up");
        assert!(visible([0.0, 0.0, 150.0], [300.0, 0.0, 10.0], &scene, &boxes), "short of it");
    }

    #[test]
    fn the_route_is_sampled_along_its_length() {
        let s = resample(&[[0.0, 0.0, 0.0], [100.0, 0.0, 0.0], [100.0, 100.0, 50.0]], 50.0, 10_000.0);
        assert_eq!(s.len(), 5);
        assert_eq!(s[2].0, [100.0, 0.0, 0.0]);
        assert_eq!(s[3].2, 150.0);
        assert!((s[3].0[2] - 25.0).abs() < 1e-3, "height along the slope");
    }
}
