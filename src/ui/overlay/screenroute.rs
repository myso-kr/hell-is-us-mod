//! The route in focus drawn into the game's view (MAP.md §17): a band on the floor ahead of the
//! hero, in perspective, as a game's own guiding line is — and hidden where the world hides it.
//!
//! The overlay cannot read the game's depth buffer. It holds the world's shape as the routes use
//! it, though (obstacles.rs, terrain.rs): each point of the band is seen from the game's camera
//! only when the line to it crosses no obstacle (a convex outline over a height range, clipped
//! like a prism) and does not go under the landscape. Underground, the landscape is overhead
//! everywhere and is left out of the test; walls and floors are obstacles there.

use super::marker;
use crate::obstacles::{clip, Obstacle, Scene};
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
const LIFT: f32 = 15.0;
/// The last stretch before a point is not tested: the floor under it would hide it.
const NEAR_END: f32 = 120.0;
/// The layer's own pace: the camera turns between the overlay's frames (50 ms), and a band on the
/// floor that lags the view by that much jerks.
const PAINT_EVERY: Duration = Duration::from_millis(16);
/// How long before a frame is due the layer stops sleeping and waits for the compositor.
const VSYNC_EARLY: Duration = Duration::from_millis(5);
/// What is seen from the camera is worked out again when the camera has moved this far (cm), or
/// this long after: the costly part (rays against the obstacles and the ground), not the drawing.
const SEEN_MOVED: f32 = 25.0;
const SEEN_FOR: Duration = Duration::from_millis(120);

/// A piece of a band on the screen, between two samples: its ends, its half widths there (px),
/// how far along the way each is (cm), whether both are seen, and what it is.
#[derive(Clone, Copy)]
struct Seg {
    a: (f32, f32),
    b: (f32, f32),
    wa: f32,
    wb: f32,
    sa: f32,
    sb: f32,
    seen: bool,
    kind: Piece,
}

/// What a band is: the route's, or its shortcut's (dashed, its own colour).
#[derive(Clone, Copy, PartialEq)]
enum Piece {
    Band,
    Shortcut,
}

/// The shortcut: its colour (the maps'), its width against the route's, its dashes (cm).
const SHORTCUT: [u8; 3] = [90, 215, 235];
const SHORTCUT_WIDTH: f32 = 0.7;
const DASH: f32 = 150.0;
/// A band is drawn this wide at most and at least (half, px) — a floor decal narrows with
/// distance; under the camera it would fill the view — with a dark outline this wide (px).
const HALF_MAX_PX: f32 = 36.0;
const HALF_MIN_PX: f32 = 1.6;
const OUTLINE_PX: f32 = 1.6;
/// What the world hides of a band is drawn faint and dotted, not cut: the tests are rays
/// against an approximation, and a hard cut flickered as they changed their mind.
const HIDDEN_ALPHA: f32 = 0.35;
const HIDDEN_DOT: f32 = 100.0;

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
    /// The band's coverage per pixel (outline, core) and its core's colour: kept between
    /// frames, the canvas's size.
    cover: Vec<(u8, u8, [u8; 3])>,
    /// Where the canvas was drawn last (client px): what the next frame clears.
    drawn: Option<(i32, i32, i32, i32)>,
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
            cover: Vec::new(),
            drawn: None,
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
    /// The route's shortcut down, beside it (route.rs `Shortcut`).
    pub shortcut: Vec<[f32; 3]>,
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
                    Some(cam) => layer.draw(
                        &cam,
                        j.client,
                        &j.route,
                        &j.shortcut,
                        &j.scene,
                        j.colour,
                        j.hero,
                        &j.things,
                        j.icon_px,
                    ),
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
        // Paced by the compositor: sleep to just before the next frame is due, then wait for
        // the composition itself (DwmFlush), so the camera is read right after the screen
        // turns over and the band is presented with that frame. A timer alone beats against
        // the display's refresh and the band swims against the game. No composition (the
        // call fails): the timer alone.
        std::thread::sleep(PAINT_EVERY.saturating_sub(start.elapsed() + VSYNC_EARLY));
        // SAFETY: no arguments; blocks this thread until the next composition pass.
        if unsafe { windows_sys::Win32::Graphics::Dwm::DwmFlush() } < 0 {
            std::thread::sleep(VSYNC_EARLY);
        }
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
        shortcut: &[[f32; 3]],
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
        // a band over a way: each sample's middle on the screen, its half width there (the
        // floor's width projected), how far along, and whether it is seen
        let mut band = |way: &[[f32; 3]], half: f32, kind: Piece| -> Vec<Seg> {
            let mut out = Vec::new();
            let mut prev: Option<((f32, f32), f32, f32, bool)> = None;
            for (p, dir, along) in resample(way, STEP, AHEAD) {
                let at = [p[0], p[1], p[2] + LIFT];
                let side = [at[0] - dir[1] * half, at[1] + dir[0] * half, at[2]];
                let cur = match (
                    marker::project(cam, at, w as f32, h as f32),
                    marker::project(cam, side, w as f32, h as f32),
                ) {
                    (Some(c), Some(e)) => {
                        let wpx = (e.0 - c.0).hypot(e.1 - c.1).clamp(HALF_MIN_PX, HALF_MAX_PX);
                        Some(((c.0, c.1), wpx, along, seen_from(at)))
                    }
                    _ => None,
                };
                if let (Some(a), Some(b)) = (prev, cur) {
                    if along > FROM {
                        out.push(Seg { a: a.0, b: b.0, wa: a.1, wb: b.1, sa: a.2, sb: b.2, seen: a.3 && b.3, kind });
                    }
                }
                prev = cur;
            }
            out
        };
        // the shortcut first, the route over it
        let mut segs =
            if shortcut.len() > 1 { band(shortcut, HALF * SHORTCUT_WIDTH, Piece::Shortcut) } else { Vec::new() };
        segs.extend(band(route, HALF, Piece::Band));
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
        if segs.is_empty() && marks.is_empty() {
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
        for g in &segs {
            let m = g.wa.max(g.wb) + OUTLINE_PX + 1.0;
            x0 = x0.min(g.a.0.min(g.b.0) - m);
            y0 = y0.min(g.a.1.min(g.b.1) - m);
            x1 = x1.max(g.a.0.max(g.b.0) + m);
            y1 = y1.max(g.a.1.max(g.b.1) + m);
        }
        let (x0, y0) = ((x0.floor() as i32 - 4).clamp(0, w), (y0.floor() as i32 - 4).clamp(0, h));
        let (x1, y1) = ((x1.ceil() as i32 + 4).clamp(0, w), (y1.ceil() as i32 + 4).clamp(0, h));
        if x1 <= x0 || y1 <= y0 {
            self.hide();
            return;
        }
        // One window over the whole client area, made once and never moved: a window the band's
        // size, made again as it grew past a step and moved as it went, flashed as the camera
        // turned (seen in play) — between a move and its picture, or a new window's first frame.
        if self.window.as_ref().is_none_or(|win| win.w != w || win.h != h) {
            self.window = Layered::new_composed("hiumod-route", "Hell Is Us Route", w, h);
            self.canvas = Canvas::new(w as usize, h as usize);
            self.drawn = None;
        }
        let Some(win) = self.window.as_mut() else { return };
        // cleared where it was drawn last, drawn where it is now
        if let Some(r) = self.drawn.take() {
            clear_rect(&mut self.canvas, r);
        }
        let rect = (x0, y0, x1, y1);
        clear_rect(&mut self.canvas, rect);
        self.drawn = Some(rect);
        let (ox, oy) = (0.0, 0.0);
        paint_band(&mut self.canvas, &mut self.cover, &segs, rect, colour);
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
        win.present(&self.canvas, left, top);
    }
}

/// The band's pieces into `cv` (its origin at `o` on the screen), each pixel by its distance to
/// the piece's line: covered where it is within the half width there, the outline a little
/// wider, every edge and joint smooth and round. Where pieces overlap the most covering wins,
/// so joints are not drawn twice.
fn paint_band(
    cv: &mut Canvas,
    cover: &mut Vec<(u8, u8, [u8; 3])>,
    segs: &[Seg],
    rect: (i32, i32, i32, i32),
    colour: [u8; 3],
) {
    // the rectangle of the canvas drawn in (px): the band's bounds
    let (rx0, ry0) = (rect.0.max(0) as usize, rect.1.max(0) as usize);
    let (rx1, ry1) = ((rect.2.max(0) as usize).min(cv.w), (rect.3.max(0) as usize).min(cv.h));
    if rx1 <= rx0 || ry1 <= ry0 {
        return;
    }
    let rw = rx1 - rx0;
    cover.clear();
    cover.resize(rw * (ry1 - ry0), (0, 0, [0; 3]));
    for g in segs {
        let (ax, ay, bx, by) = (g.a.0, g.a.1, g.b.0, g.b.1);
        let m = g.wa.max(g.wb) + OUTLINE_PX + 1.0;
        let px0 = (((ax.min(bx) - m).floor().max(0.0)) as usize).max(rx0);
        let px1 = ((ax.max(bx) + m).ceil().max(0.0) as usize).min(rx1);
        let py0 = (((ay.min(by) - m).floor().max(0.0)) as usize).max(ry0);
        let py1 = ((ay.max(by) + m).ceil().max(0.0) as usize).min(ry1);
        let (dx, dy) = (bx - ax, by - ay);
        let l2 = dx * dx + dy * dy;
        for y in py0..py1 {
            for x in px0..px1 {
                let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
                let t = if l2 > 1e-6 { (((fx - ax) * dx + (fy - ay) * dy) / l2).clamp(0.0, 1.0) } else { 0.0 };
                let d = (fx - ax - dx * t).hypot(fy - ay - dy * t);
                let hw = g.wa + (g.wb - g.wa) * t;
                if d > hw + OUTLINE_PX + 0.5 {
                    continue;
                }
                let along = g.sa + (g.sb - g.sa) * t;
                // fading out ahead; a brighter chevron every 3 m on the route
                let fade = 1.0 - (along / AHEAD).clamp(0.0, 1.0) * 0.7;
                let (rgb, mut alpha, mut dash) = match g.kind {
                    Piece::Band if (along / 300.0).fract() < 0.18 => ([255, 244, 214], 0.9 * fade, None),
                    Piece::Band => (colour, 0.75 * fade, None),
                    Piece::Shortcut => (SHORTCUT, 0.7 * fade, Some(DASH)),
                };
                if !g.seen {
                    alpha *= HIDDEN_ALPHA;
                    dash = Some(HIDDEN_DOT);
                }
                if dash.is_some_and(|p| (along / p).fract() >= 0.5) {
                    continue;
                }
                let core = ((hw + 0.5 - d).clamp(0.0, 1.0) * alpha * 255.0) as u8;
                let edge = ((hw + OUTLINE_PX + 0.5 - d).clamp(0.0, 1.0) * alpha * 0.55 * 255.0) as u8;
                let c = &mut cover[(y - ry0) * rw + (x - rx0)];
                c.0 = c.0.max(edge);
                if core > c.1 {
                    c.1 = core;
                    c.2 = rgb;
                }
            }
        }
    }
    for y in ry0..ry1 {
        for x in rx0..rx1 {
            let (edge, core, rgb) = cover[(y - ry0) * rw + (x - rx0)];
            if edge > 0 {
                cv.blend(x as i32, y as i32, crate::map::canvas::Rgba(0, 0, 0, 255), edge as f32 / 255.0);
            }
            if core > 0 {
                cv.blend(
                    x as i32,
                    y as i32,
                    crate::map::canvas::Rgba(rgb[0], rgb[1], rgb[2], 255),
                    core as f32 / 255.0,
                );
            }
        }
    }
}

/// Clear the rectangle `r` (px) of `cv`.
fn clear_rect(cv: &mut Canvas, r: (i32, i32, i32, i32)) {
    let (x0, x1) = (r.0.max(0) as usize, (r.2.max(0) as usize).min(cv.w));
    for y in (r.1.max(0) as usize)..(r.3.max(0) as usize).min(cv.h) {
        if x1 > x0 {
            cv.px[y * cv.w + x0..y * cv.w + x1].fill(0);
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_band_is_solid_inside_smooth_at_its_edge_and_empty_outside() {
        let mut cv = Canvas::new(60, 30);
        let mut cover = Vec::new();
        // a piece along y = 15 from x 10 to 50, 4 px each side, seen, near (no fade to speak of)
        let g = Seg {
            a: (10.0, 15.0),
            b: (50.0, 15.0),
            wa: 4.0,
            wb: 4.0,
            sa: 500.0,
            sb: 510.0,
            seen: true,
            kind: Piece::Band,
        };
        paint_band(&mut cv, &mut cover, &[g], (0, 0, 60, 30), [200, 100, 50]);
        let alpha = |x: usize, y: usize| cv.px[y * 60 + x] >> 24;
        assert!(alpha(30, 15) > 150, "inside");
        let rim = alpha(30, 19);
        assert!(rim > 0 && rim < alpha(30, 15), "the edge in between: {rim}");
        assert_eq!(alpha(30, 25), 0, "outside");
        // the end is round: past it by half the width, nothing
        assert_eq!(alpha(57, 15), 0);
    }

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
