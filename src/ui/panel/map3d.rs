//! The 3D map page (.spec/MAP.md §16): the hero's region as the game's own data has it — its
//! landscape (`Mods\terrain`, `survey --terrain`) and walkable floors underground included
//! (`Mods\navmesh`, `survey --navmesh`) — with the requirement graph's places on them, drawn with
//! OpenGL in the panel (an egui paint callback on the panel's own context).
//!
//! Seeing the ground and what is under it at once (research in MAP.md §16): the ground stays
//! opaque and drops pixels instead of blending (screen-door dither) along the line from the eye to
//! the point looked at and, if asked, everywhere ("keep"); with X-ray on, what the ground or other
//! floors hide is drawn again as a faint silhouette (depth test GREATER, one layer per pixel by the
//! stencil). Floors under the ground take their colour from their depth: cyan just under it,
//! violet deep down.

use crate::graph::Graph;
use eframe::egui::{self, Color32, RichText};
use eframe::{egui_glow, glow};
use glow::HasContext;
use std::sync::{Arc, Mutex};

/// A region's scene, ready for the GPU: positions in metres, x east, y up, z south, from the
/// landscape's middle and its lowest point.
#[derive(Default)]
pub struct Scene {
    world: String,
    /// x, y, z, nx, ny, nz per vertex; triangles by index.
    terrain: Vec<f32>,
    terrain_idx: Vec<u32>,
    /// x, y, z, r, g, b per vertex, three to a triangle: floors on the ground, under it, indoors.
    floors: [Vec<f32>; 3],
    /// The graph's places: position, colour, round, label.
    nodes: Vec<Node>,
    /// The landscape's height span (m), for the colour ramp.
    span: f32,
    /// Unreal → scene: the landscape's middle (cm) and lowest point (cm).
    origin: [f32; 3],
    extent: f32,
}

#[derive(Clone)]
struct Node {
    at: [f32; 3],
    /// Its marker, as the maps draw the same sort (assets/icons); none for what has no place on
    /// a map of its own (a lever, a trigger, a line said).
    sub: Option<crate::actors::Sub>,
    round: i32,
    label: String,
    below: f32,
}

impl Scene {
    fn to_scene(&self, p: [f32; 3]) -> [f32; 3] {
        [(p[1] - self.origin[1]) / 100.0, (p[2] - self.origin[2]) / 100.0, -(p[0] - self.origin[0]) / 100.0]
    }

    /// Back from the scene's metres to the game's place (cm): `to_scene` undone.
    fn to_game(&self, q: [f32; 3]) -> [f32; 3] {
        [self.origin[0] - q[2] * 100.0, self.origin[1] + q[0] * 100.0, self.origin[2] + q[1] * 100.0]
    }
}

/// The map's sort of a graph node, by its class alone (the maps classify by the class's lineage,
/// which the survey does not keep): a person, a door, a save point, a puzzle, an item…
fn sort_of(class: &str) -> Option<crate::actors::Sub> {
    use crate::actors::Sub;
    let has = |p: &str| class.contains(p);
    let lower = class.to_ascii_lowercase();
    Some(if class == "Say" || class == "CodeGives" || class == "StoryGives" || class == "Trade" {
        return None;
    } else if class.starts_with("Convo_") {
        Sub::NpcTalk
    } else if lower.starts_with("quickchat") {
        if lower.contains("_secret_") {
            Sub::NpcSecret
        } else if lower.contains("_quest_") {
            Sub::NpcQuest
        } else {
            Sub::Npc
        }
    } else if has("SavePoint") {
        if has("NoTravel") {
            Sub::SavePointLocal
        } else {
            Sub::SavePoint
        }
    } else if has("APC_") {
        Sub::Apc
    } else if has("LymbicLock") {
        Sub::LymbicLock
    } else if has("DroneTranslation") {
        Sub::Translation
    } else if class.starts_with("VOFK_") {
        Sub::Vault
    } else if has("_Spawner_C") || class == "FightWon" || class == "BossFightWon" {
        Sub::EnemyGroup
    } else if has("Placement") || has("Puzzle") || has("Keypad") || has("Dial") {
        Sub::Puzzle
    } else if has("Door") || has("Gate") || has("KeyLocked") {
        Sub::Door
    } else if has("Gather") || class.starts_with("Cons_") {
        crate::actors::item(class)
    } else if has("Chest") || has("Stash") {
        Sub::Stash
    } else {
        return crate::actors::by_class(class);
    })
}

/// Plain base64 (the terrain file's heights).
fn base64(s: &str) -> Vec<u8> {
    let val = |c: u8| match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    };
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0);
    for v in s.bytes().filter_map(val) {
        acc = acc << 6 | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    out
}

/// The scene of `world` from the files the survey tool wrote and the requirement graph.
pub fn load(world: &str, graph: &Graph, rounds: &[Option<usize>]) -> Option<Scene> {
    let dir = crate::paths::data_dir();
    let tiles = std::fs::read(dir.join("navmesh").join(format!("{world}.navmesh.bin"))).unwrap_or_default();
    let land = Land::read(&dir.join("terrain").join(format!("{world}.terrain.json")));
    if land.is_none() && tiles.is_empty() {
        return None;
    }
    // Where the scene is centred: the landscape, or (an interior) the floors' bounds.
    let (origin, extent, span) = match &land {
        Some(l) => (
            [l.x0 + l.w as f32 * l.cell / 2.0, l.y0 + l.h as f32 * l.cell / 2.0, l.lo],
            l.w.max(l.h) as f32 * l.cell / 100.0,
            (l.hi - l.lo) / 100.0,
        ),
        None => {
            let pts = tile_points(&tiles);
            if pts.is_empty() {
                return None;
            }
            let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
            for p in &pts {
                for k in 0..3 {
                    lo[k] = lo[k].min(p[k]);
                    hi[k] = hi[k].max(p[k]);
                }
            }
            (
                [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, lo[2]],
                (hi[0] - lo[0]).max(hi[1] - lo[1]) / 100.0,
                (hi[2] - lo[2]) / 100.0,
            )
        }
    };
    let mut s = Scene { world: world.to_string(), origin, span, extent: extent.max(50.0), ..Default::default() };
    let ground = |x: f32, y: f32| land.as_ref().and_then(|l| l.at(x, y));
    if let Some(l) = &land {
        let (w, h) = (l.w, l.h);
        let z = |i: usize, j: usize| l.heights[j.min(h - 1) * w + i.min(w - 1)].unwrap_or(l.lo);
        for j in 0..h {
            for i in 0..w {
                let p = s.to_scene([l.x0 + (i as f32 + 0.5) * l.cell, l.y0 + (j as f32 + 0.5) * l.cell, z(i, j)]);
                let (dx, dy) = (
                    (z(i + 1, j) - z(i.saturating_sub(1), j)) / (2.0 * l.cell),
                    (z(i, j + 1) - z(i, j.saturating_sub(1))) / (2.0 * l.cell),
                );
                // Unreal gradient (x, y) → scene normal: x east is +y, z south is −x.
                let n = [-dy, 1.0, dx];
                let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
                s.terrain.extend([p[0], p[1], p[2], n[0] / len, n[1] / len, n[2] / len]);
            }
        }
        for j in 0..h - 1 {
            for i in 0..w - 1 {
                let (a, b, c, d) = (j * w + i, j * w + i + 1, (j + 1) * w + i, (j + 1) * w + i + 1);
                if [a, b, c, d].iter().any(|&k| l.heights[k].is_none()) {
                    continue;
                }
                s.terrain_idx.extend([a as u32, c as u32, b as u32, b as u32, c as u32, d as u32]);
            }
        }
    }
    // the floors, from the cooked navmesh tiles
    {
        let bytes = &tiles;
        let mut o = 0;
        while o + 4 <= bytes.len() {
            let n = u32::from_le_bytes(bytes[o..o + 4].try_into().unwrap()) as usize;
            let Some(tile) = bytes.get(o + 4..o + 4 + n) else { break };
            o += 4 + n;
            if tile.len() < 0x58 {
                continue;
            }
            let (pc, vc) =
                (u16::from_le_bytes([tile[4], tile[5]]) as usize, u16::from_le_bytes([tile[6], tile[7]]) as usize);
            let vert = |k: usize| -> Option<[f32; 3]> {
                let b = tile.get(0x58 + k * 24..0x58 + k * 24 + 24)?;
                let d = |q: usize| f64::from_le_bytes(b[q..q + 8].try_into().unwrap()) as f32;
                Some([-d(0), -d(16), d(8)])
            };
            let po = 0x58 + vc * 24;
            for p in 0..pc {
                let Some(q) = tile.get(po + p * 32..po + p * 32 + 32) else { break };
                if q[31] >> 6 == 1 {
                    continue;
                }
                let cnt = q[30] as usize;
                let ids: Vec<usize> =
                    (0..cnt.min(6)).map(|m| u16::from_le_bytes([q[4 + 2 * m], q[5 + 2 * m]]) as usize).collect();
                let Some(pts) = ids.iter().map(|&k| vert(k)).collect::<Option<Vec<_>>>() else { continue };
                let c = pts.iter().fold([0.0; 3], |a, v| {
                    [a[0] + v[0] / cnt as f32, a[1] + v[1] / cnt as f32, a[2] + v[2] / cnt as f32]
                });
                let (kind, colour) = match ground(c[0], c[1]) {
                    None => (2, [0.50, 0.69, 0.87]),
                    Some(g) if c[2] < g - 300.0 => (1, depth_colour((g - c[2]) / 100.0)),
                    Some(_) => (0, [0.55, 0.52, 0.40]),
                };
                for m in 1..cnt.saturating_sub(1) {
                    for v in [pts[0], pts[m], pts[m + 1]] {
                        let sp = s.to_scene(v);
                        s.floors[kind].extend([sp[0], sp[1] + 0.25, sp[2], colour[0], colour[1], colour[2]]);
                    }
                }
            }
        }
    }
    // the graph's places of the world
    for (i, n) in graph.nodes.iter().enumerate() {
        if crate::survey::Survey::world_of(&n.world) != world || n.at == [0.0; 3] {
            continue;
        }
        // only what the maps have a sort for: a trigger, a sound or a line said is no place
        let Some(sub) = sort_of(&n.class) else { continue };
        let below = ground(n.at[0], n.at[1]).map_or(0.0, |g| ((g - n.at[2]) / 100.0).max(0.0));
        s.nodes.push(Node {
            at: s.to_scene(n.at),
            sub: Some(sub),
            round: rounds.get(i).copied().flatten().map_or(-1, |r| r as i32),
            label: n.class.trim_end_matches("_C").to_string(),
            below,
        });
    }
    Some(s)
}

/// A region's landscape as `survey --terrain` wrote it (cm).
struct Land {
    w: usize,
    h: usize,
    cell: f32,
    x0: f32,
    y0: f32,
    lo: f32,
    hi: f32,
    heights: Vec<Option<f32>>,
}

impl Land {
    fn read(path: &std::path::Path) -> Option<Land> {
        let t: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
        let (w, h) = (t["w"].as_u64()? as usize, t["h"].as_u64()? as usize);
        // An interior's landscape is a sliver (a few cells): no ground worth drawing.
        if w < 4 || h < 4 {
            return None;
        }
        let raw = base64(t["heights"].as_str()?);
        let heights: Vec<Option<f32>> = raw
            .chunks_exact(2)
            .map(|b| i16::from_le_bytes([b[0], b[1]]))
            .map(|v| (v != i16::MIN).then_some(v as f32 * 10.0))
            .collect();
        if heights.len() != w * h {
            return None;
        }
        let lo = heights.iter().flatten().fold(f32::MAX, |a, &b| a.min(b));
        let hi = heights.iter().flatten().fold(f32::MIN, |a, &b| a.max(b));
        Some(Land {
            w,
            h,
            cell: t["cell"].as_f64()? as f32,
            x0: t["x0"].as_f64()? as f32,
            y0: t["y0"].as_f64()? as f32,
            lo,
            hi,
            heights,
        })
    }

    fn at(&self, x: f32, y: f32) -> Option<f32> {
        let (i, j) = (((x - self.x0) / self.cell).floor(), ((y - self.y0) / self.cell).floor());
        if i < 0.0 || j < 0.0 || i as usize >= self.w || j as usize >= self.h {
            return None;
        }
        self.heights[j as usize * self.w + i as usize]
    }
}

/// Every vertex of the tiles (Unreal, cm): an interior's bounds.
fn tile_points(bytes: &[u8]) -> Vec<[f32; 3]> {
    let mut out = Vec::new();
    let mut o = 0;
    while o + 4 <= bytes.len() {
        let n = u32::from_le_bytes(bytes[o..o + 4].try_into().unwrap()) as usize;
        let Some(tile) = bytes.get(o + 4..o + 4 + n) else { break };
        o += 4 + n;
        if tile.len() < 0x58 {
            continue;
        }
        let vc = u16::from_le_bytes([tile[6], tile[7]]) as usize;
        for k in 0..vc {
            let Some(b) = tile.get(0x58 + k * 24..0x58 + k * 24 + 24) else { break };
            let d = |q: usize| f64::from_le_bytes(b[q..q + 8].try_into().unwrap()) as f32;
            out.push([-d(0), -d(16), d(8)]);
        }
    }
    out
}

/// Under the ground: cyan just under it, violet 30 m down.
fn depth_colour(below_m: f32) -> [f32; 3] {
    let t = ((below_m - 3.0) / 30.0).clamp(0.0, 1.0);
    [0.37 + t * (0.65 - 0.37), 0.88 + t * (0.48 - 0.88), 0.82 + t * (1.0 - 0.82)]
}

/// The page's state: the scene (loaded off the panel's thread), the GPU's copy, the view.
pub struct Map3d {
    scene: Option<Arc<Scene>>,
    loading: Option<std::thread::JoinHandle<Option<Scene>>>,
    /// The world asked for, and whether it had no files (`doctor map3d` not run).
    asked: String,
    missing: bool,
    gpu: Arc<Mutex<Option<Gpu>>>,
    yaw: f32,
    pitch: f32,
    dist: f32,
    target: [f32; 3],
    pub xray: bool,
    pub hole: bool,
    pub hole_m: f32,
    pub keep: f32,
    pub round: i32,
    picked: Option<usize>,
    /// The view keeps the hero in its middle (a right drag lets go).
    follow: bool,
    /// A spot picked on the floor (a double click), in the game's place (cm).
    spot: Option<[f32; 3]>,
    /// Whether the page lets the hero be sent there (the cheats agreed to, the hero in play).
    pub can_teleport: bool,
    /// A teleport asked for on the view, for the page to send.
    pub teleport: Option<[f32; 3]>,
    /// A spot to add to the filming take's points, for the page (groups.rs `film_card`).
    pub film_point: Option<[f32; 3]>,
}

impl Default for Map3d {
    fn default() -> Self {
        Map3d {
            scene: None,
            loading: None,
            asked: String::new(),
            missing: false,
            gpu: Arc::new(Mutex::new(None)),
            yaw: 0.6,
            pitch: 0.75,
            dist: 600.0,
            target: [0.0; 3],
            xray: true,
            hole: true,
            hole_m: 80.0,
            keep: 1.0,
            round: i32::MAX,
            picked: None,
            follow: true,
            spot: None,
            can_teleport: false,
            teleport: None,
            film_point: None,
        }
    }
}

impl Map3d {
    /// Load `world`'s scene when it is not the one shown (off this thread).
    pub fn want(&mut self, world: &str) {
        if let Some(h) = self.loading.take_if(|h| h.is_finished()) {
            match h.join().ok().flatten() {
                Some(s) => {
                    self.target = [0.0; 3];
                    self.dist = s.extent * 0.9;
                    self.scene = Some(Arc::new(s));
                    self.missing = false;
                    if let Some(g) = self.gpu.lock().unwrap().as_mut() {
                        g.stale = true;
                    }
                }
                None => self.missing = true,
            }
        }
        if world.is_empty() || self.asked == world || self.loading.is_some() {
            return;
        }
        self.asked = world.to_string();
        let w = world.to_string();
        self.loading = Some(std::thread::spawn(move || {
            let g = Graph::load(&crate::paths::data_dir().join("survey"));
            let r = g.reach();
            load(&w, &g, &r.depth)
        }));
    }

    pub fn loading(&self) -> bool {
        self.loading.is_some()
    }

    pub fn missing(&self) -> bool {
        self.missing
    }

    pub fn scene(&self) -> Option<&Arc<Scene>> {
        self.scene.as_ref()
    }

    /// The view, as a map app's: the scene fills the rect, and its controls float over it — the
    /// layers top left, the compass top right, zoom and "where am I" bottom right, the story
    /// round along the bottom, the picked place's card. The hero is a dot with a cone the way
    /// they face and a pulse, the route's end a pin. Drag turns, a right drag moves, the wheel
    /// zooms; following, the view keeps the hero in its middle.
    #[allow(clippy::too_many_arguments)]
    pub fn view(
        &mut self,
        ui: &mut egui::Ui,
        height: f32,
        hero: Option<([f32; 3], f32)>,
        route: &[[f32; 3]],
        colour: [u8; 3],
        state: &crate::minimap::MapState,
        shortcut: &crate::ui::Shortcut3d,
    ) {
        let _t = crate::prof::span("map3d.view");
        let size = egui::vec2(ui.available_width(), height);
        let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
        let Some(scene) = self.scene.clone() else {
            ui.painter().rect_filled(rect, 10.0, Color32::from_rgb(10, 16, 18));
            return;
        };
        if resp.dragged_by(egui::PointerButton::Primary) {
            let d = resp.drag_delta();
            self.yaw -= d.x * 0.006;
            self.pitch = (self.pitch + d.y * 0.006).clamp(0.08, 1.48);
        }
        if resp.dragged_by(egui::PointerButton::Secondary) {
            let d = resp.drag_delta() * self.dist / rect.height();
            let (s, c) = self.yaw.sin_cos();
            self.target[0] -= d.x * c;
            self.target[2] += d.x * s;
            self.target[1] += d.y;
            self.follow = false;
        }
        // The wheel scrolls the page, as over an embedded map; Ctrl+wheel (or a pinch) zooms,
        // which egui hands over as a zoom and not a scroll, so the page stays put.
        if resp.hovered() {
            let zoom = ui.input(|i| i.zoom_delta());
            if zoom != 1.0 {
                self.dist = (self.dist / zoom).clamp(20.0, scene.extent * 3.0);
            }
        }
        let hero_at = hero.map(|(h, yaw)| (scene.to_scene(h), yaw));
        if self.follow {
            if let Some((h, _)) = hero_at {
                // eased, so the view glides after the hero rather than jumps
                for (t, h) in self.target.iter_mut().zip(h) {
                    *t += (h - *t) * 0.2;
                }
                // frames only while it glides; once there, the snapshots' own frames do
                if (0..3).any(|k| (h[k] - self.target[k]).abs() > 0.05) {
                    ui.ctx().request_repaint_after(std::time::Duration::from_millis(16));
                }
            }
        }
        let aspect = rect.width() / rect.height().max(1.0);
        let eye = self.eye();
        let view = look_at(eye, self.target);
        let proj = perspective(50f32.to_radians(), aspect, 1.0, scene.extent * 8.0);
        let vp = mul(&proj, &view);
        let route: Vec<[f32; 3]> = route.iter().map(|&q| scene.to_scene(q)).collect();
        let cut: Vec<[f32; 3]> = shortcut.0.iter().map(|&q| scene.to_scene(q)).collect();
        // A double click on the floor picks the spot under it: the nearest to the camera of the
        // walkable floors' (else the ground's) points within a few pixels of the click.
        if resp.double_clicked() {
            if let Some(pos) = resp.interact_pointer_pos() {
                let near = |pts: &[f32], stride: usize| {
                    pts.chunks_exact(stride)
                        .map(|c| [c[0], c[1], c[2]])
                        .filter_map(|q| {
                            let p = project(&vp, q, rect)?;
                            let d =
                                ((q[0] - eye[0]).powi(2) + (q[1] - eye[1]).powi(2) + (q[2] - eye[2]).powi(2)).sqrt();
                            (p.distance(pos) < SPOT_PX).then_some((d, q))
                        })
                        .min_by(|a, b| a.0.total_cmp(&b.0))
                };
                let floor = scene.floors.iter().filter_map(|f| near(f, 6)).min_by(|a, b| a.0.total_cmp(&b.0));
                if let Some((_, q)) = floor.or_else(|| near(&scene.terrain, 6)) {
                    self.spot = Some(scene.to_game(q));
                    self.picked = None;
                }
            }
        } else if resp.clicked() {
            if let Some(pos) = resp.interact_pointer_pos() {
                self.spot = None;
                self.picked = scene
                    .nodes
                    .iter()
                    .enumerate()
                    .filter(|(_, n)| n.round >= 0 && n.round <= self.round)
                    .filter_map(|(i, n)| project(&vp, n.at, rect).map(|p| (i, p.distance(pos))))
                    .filter(|(_, d)| *d < 14.0)
                    .min_by(|a, b| a.1.total_cmp(&b.1))
                    .map(|(i, _)| i);
            }
        }
        let frame = Frame {
            vp,
            eye,
            target: self.target,
            span: scene.span,
            xray: self.xray,
            hole: if self.hole { self.hole_m } else { 0.0 },
            keep: self.keep,
            round: self.round,
            hero: None,
            picked: self.picked,
            route: ribbon(&route, colour),
            shortcut: ribbon(&cut, crate::navmesh::shortcut_rgb()),
        };
        let gpu = self.gpu.clone();
        let drawn = scene.clone();
        let cb = egui_glow::CallbackFn::new(move |info, painter| {
            let gl = painter.gl();
            let mut g = gpu.lock().unwrap();
            if g.is_none() {
                *g = Gpu::new(gl);
            }
            if let Some(g) = g.as_mut() {
                g.draw(gl, &drawn, &frame, &info);
            }
        });
        let painter = ui.painter_at(rect);
        painter.add(egui::PaintCallback { rect, callback: Arc::new(cb) });

        // the places as the maps draw them: their icons, the nearest first, none over another
        let eye_at = eye;
        // as the 2D maps fade them (raster.rs): another floor faint with an arrow up or down
        // (compass::floor_alpha, floor_badge), far from the hero faint as the big map's edge,
        // and the icons' layer opacity over all
        let here = hero_at.map(|(h, _)| h);
        let marks = state.opacity[2] as f32 / 100.0;
        let fade = |at: [f32; 3], target: bool| -> (f32, f32) {
            let Some(h) = here else { return (marks, 0.0) };
            let dz = at[1] - h[1];
            let floor = crate::map::compass::floor_alpha(dz);
            let floor = if target { floor.max(190) } else { floor } as f32 / 255.0;
            (floor * edge_fade(at, h, state.big_radius_m) * marks, dz)
        };
        let mut shown: Vec<(f32, egui::Pos2, crate::actors::Sub, [f32; 3])> = scene
            .nodes
            .iter()
            .filter(|n| n.round >= 0 && n.round <= self.round)
            .filter_map(|n| {
                let p = project(&vp, n.at, rect)?;
                let d = ((n.at[0] - eye_at[0]).powi(2) + (n.at[1] - eye_at[1]).powi(2) + (n.at[2] - eye_at[2]).powi(2))
                    .sqrt();
                let sub = n.sub.filter(|s| state.shows(*s))?;
                rect.contains(p).then_some((d, p, sub, n.at))
            })
            .collect();
        shown.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut placed: Vec<egui::Pos2> = Vec::new();
        let size = state.icon_px as f32;
        for (_, p, sub, at) in shown {
            if placed.len() >= ICONS_MAX || placed.iter().any(|q| q.distance(p) < size + 2.0) {
                continue;
            }
            placed.push(p);
            let px = (size * ui.ctx().pixels_per_point()).round() as u32;
            if let Some(t) = crate::ui::svg::texture(ui.ctx(), crate::icons::source(sub), px) {
                let r = egui::Rect::from_center_size(p, egui::vec2(size, size));
                let (alpha, dz) = fade(at, false);
                painter.image(
                    t.id(),
                    r,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    Color32::WHITE.gamma_multiply(alpha),
                );
                floor_badge(&painter, p, size / 2.0, dz);
            }
        }
        // markers over the scene: the route's end, the picked place, the hero
        // the spot picked, a ring on the floor
        if let Some(q) = self.spot.and_then(|at| project(&vp, scene.to_scene(at), rect)) {
            painter.circle_stroke(q, 8.0, egui::Stroke::new(2.5, OUTLINE));
            painter.circle_stroke(q, 8.0, egui::Stroke::new(1.5, Color32::WHITE));
            painter.circle_filled(q, 2.5, Color32::WHITE);
        }
        // the shortcut's drops: ↓ and how far, coloured by what the fall does
        for &(top, h) in &shortcut.1 {
            if let Some(q) = project(&vp, scene.to_scene(top), rect) {
                let [r, g, b] = crate::navmesh::drop_rgb(h);
                let c = Color32::from_rgb(r, g, b);
                painter.circle_filled(q, 7.0, OUTLINE);
                painter.add(egui::Shape::convex_polygon(
                    vec![q + egui::vec2(0.0, 4.5), q + egui::vec2(-4.5, -2.5), q + egui::vec2(4.5, -2.5)],
                    c,
                    egui::Stroke::NONE,
                ));
                painter.text(
                    q + egui::vec2(10.0, 0.0),
                    egui::Align2::LEFT_CENTER,
                    format!("{:.0}m", h / 100.0),
                    egui::FontId::proportional(11.0),
                    c,
                );
            }
        }
        let [cr, cg, cb] = colour;
        if let Some((end, at)) = route.last().and_then(|&q| project(&vp, q, rect).map(|e| (e, q))) {
            let (alpha, dz) = fade(at, true);
            diamond(&painter, end, 7.0, Color32::from_rgb(cr, cg, cb), alpha);
            floor_badge(&painter, end, 8.5, dz);
        }
        if let Some(p) = self.picked.and_then(|i| scene.nodes.get(i)).and_then(|n| project(&vp, n.at, rect)) {
            painter.circle_stroke(p, 9.0, egui::Stroke::new(2.0, Color32::from_rgb(255, 210, 122)));
        }
        if let Some((h, yaw)) = hero_at {
            if let Some(c) = project(&vp, h, rect) {
                // the way the hero faces, on the screen: a point a few metres ahead projected
                let (s, co) = yaw.to_radians().sin_cos();
                let ahead = [h[0] + s * 8.0, h[1], h[2] - co * 8.0];
                let dir = project(&vp, ahead, rect).map(|a| (a - c).normalized()).unwrap_or(egui::vec2(0.0, -1.0));
                // the maps' arrow (raster.rs): white, 11 px ahead, 7 back and aside, outlined
                let side = dir.rot90();
                let tri = |k: f32| {
                    vec![c + dir * 11.0 * k, c - dir * 7.0 * k + side * 7.0 * k, c - dir * 7.0 * k - side * 7.0 * k]
                };
                painter.add(egui::Shape::convex_polygon(tri(1.3), OUTLINE, egui::Stroke::NONE));
                painter.add(egui::Shape::convex_polygon(tri(1.0), Color32::WHITE, egui::Stroke::NONE));
            }
        }

        // the floating controls
        let glass = egui::Frame::new()
            .fill(Color32::from_rgba_unmultiplied(16, 24, 28, 225))
            .corner_radius(12)
            .stroke(egui::Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 22)))
            .inner_margin(egui::Margin::symmetric(8, 6));
        let pad = 12.0;
        // top left: the layers
        let tl = egui::Rect::from_min_size(rect.min + egui::vec2(pad, pad), egui::vec2(rect.width() * 0.6, 120.0));
        ui.scope_builder(egui::UiBuilder::new().max_rect(tl), |ui| {
            ui.horizontal(|ui| {
                glass.show(ui, |ui| {
                    ui.horizontal(|ui| {
                        chip(ui, &mut self.xray, tr!("MAP3D_XRAY"));
                        chip(ui, &mut self.hole, tr!("MAP3D_HOLE"));
                        ui.label(RichText::new(tr!("MAP3D_KEEP")).small());
                        ui.add(egui::Slider::new(&mut self.keep, 0.0..=1.0).show_value(false));
                    });
                });
            });
            // the picked place's card, under the layers
            if let Some(n) = self.picked.and_then(|i| scene.nodes.get(i)) {
                ui.add_space(6.0);
                glass.show(ui, |ui| {
                    ui.set_max_width(320.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&n.label).strong().size(13.0));
                        if ui.small_button("×").clicked() {
                            self.picked = None;
                        }
                    });
                    let round =
                        if n.round < 0 { tr!("MAP3D_NEVER").to_string() } else { trf!("MAP3D_ROUND", round = n.round) };
                    let below = if n.below > 3.0 {
                        format!(" · {}", trf!("MAP3D_BELOW", m = format!("{:.0}", n.below)))
                    } else {
                        String::new()
                    };
                    ui.label(RichText::new(format!("{round}{below}")).small().color(Color32::from_rgb(160, 175, 170)));
                    if self.can_teleport && ui.button(tr!("MAP3D_TELEPORT_HERE")).clicked() {
                        self.teleport = Some(scene.to_game(n.at));
                    }
                });
            }
            // a spot picked on the floor: where, and (cheats agreed to) going there
            if let Some(at) = self.spot {
                ui.add_space(6.0);
                glass.show(ui, |ui| {
                    ui.set_max_width(320.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(tr!("MAP3D_SPOT")).strong().size(13.0));
                        if ui.small_button("×").clicked() {
                            self.spot = None;
                        }
                    });
                    if let Some((h, _)) = hero {
                        ui.label(
                            RichText::new(crate::raster::span(h, at)).small().color(Color32::from_rgb(160, 175, 170)),
                        );
                    }
                    ui.horizontal(|ui| {
                        if self.can_teleport && ui.button(tr!("MAP3D_TELEPORT_HERE")).clicked() {
                            self.teleport = Some(at);
                            self.spot = None;
                        }
                        if self.can_teleport && ui.button(tr!("MAP3D_ADD_FILM_POINT")).clicked() {
                            self.film_point = Some(at);
                            self.spot = None;
                        }
                    });
                });
            }
        });
        // top right: the compass, north up on a click
        let compass = egui::Rect::from_center_size(
            rect.right_top() + egui::vec2(-pad - 22.0, pad + 22.0),
            egui::vec2(44.0, 44.0),
        );
        let cr = ui.interact(compass, ui.id().with("map3d-compass"), egui::Sense::click());
        painter.circle_filled(compass.center(), 22.0, Color32::from_rgba_unmultiplied(16, 24, 28, 225));
        painter.circle_stroke(
            compass.center(),
            22.0,
            egui::Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 22)),
        );
        let north = egui::vec2(-self.yaw.sin(), -self.yaw.cos());
        let (c, side) = (compass.center(), north.rot90() * 4.0);
        painter.add(egui::Shape::convex_polygon(vec![c + north * 6.0, c + side, c - side], NORTH, egui::Stroke::NONE));
        painter.text(c + north * 13.0, egui::Align2::CENTER_CENTER, "N", egui::FontId::proportional(13.0), NORTH);
        if cr.on_hover_text(tr!("MAP3D_NORTH")).clicked() {
            self.yaw = 0.0;
        }
        // bottom right: zoom, and back to the hero (following)
        let br = egui::Rect::from_min_max(
            rect.right_bottom() - egui::vec2(pad + 44.0, pad + 150.0),
            rect.right_bottom() - egui::vec2(pad, pad),
        );
        ui.scope_builder(egui::UiBuilder::new().max_rect(br), |ui| {
            ui.with_layout(egui::Layout::bottom_up(egui::Align::Max), |ui| {
                glass.show(ui, |ui| {
                    // following: the button lit in the accent, as a map app's "my location" is
                    let accent = Color32::from_rgb(108, 188, 174);
                    let me = ui.add(
                        egui::Button::new(RichText::new("◎").size(18.0).color(if self.follow {
                            Color32::from_rgb(8, 20, 18)
                        } else {
                            Color32::from_rgb(170, 182, 178)
                        }))
                        .fill(if self.follow { accent } else { Color32::TRANSPARENT })
                        .min_size(egui::vec2(28.0, 28.0)),
                    );
                    if me.on_hover_text(tr!("MAP3D_ME")).clicked() {
                        self.follow = !self.follow;
                    }
                });
                ui.add_space(6.0);
                glass.show(ui, |ui| {
                    ui.vertical(|ui| {
                        if ui
                            .add(egui::Button::new(RichText::new("+").size(18.0)).min_size(egui::vec2(28.0, 28.0)))
                            .clicked()
                        {
                            self.dist = (self.dist * 0.75).max(20.0);
                        }
                        if ui
                            .add(egui::Button::new(RichText::new("−").size(18.0)).min_size(egui::vec2(28.0, 28.0)))
                            .clicked()
                        {
                            self.dist = (self.dist * 1.33).min(scene.extent * 3.0);
                        }
                    });
                });
            });
        });
        // bottom middle: the story round
        let max = scene.nodes.iter().map(|n| n.round).max().unwrap_or(0);
        let bw = (rect.width() * 0.5).clamp(260.0, 520.0);
        let bm = egui::Rect::from_min_size(
            egui::pos2(rect.center().x - bw / 2.0, rect.bottom() - pad - 40.0),
            egui::vec2(bw, 40.0),
        );
        ui.scope_builder(egui::UiBuilder::new().max_rect(bm), |ui| {
            glass.show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(tr!("MAP3D_ROUNDS")).small());
                    let mut r = self.round.min(max);
                    ui.spacing_mut().slider_width = (bw - 170.0).max(80.0);
                    if ui.add(egui::Slider::new(&mut r, 0..=max.max(0))).changed() {
                        self.round = if r >= max { i32::MAX } else { r };
                    }
                });
            });
        });
        // the hint, faint, bottom left
        painter.text(
            rect.left_bottom() + egui::vec2(pad, -pad),
            egui::Align2::LEFT_BOTTOM,
            tr!("MAP3D_HINT"),
            egui::FontId::proportional(11.0),
            Color32::from_rgba_unmultiplied(200, 210, 206, 110),
        );
    }

    fn eye(&self) -> [f32; 3] {
        let (sp, cp) = self.pitch.sin_cos();
        let (sy, cy) = self.yaw.sin_cos();
        [self.target[0] + self.dist * cp * sy, self.target[1] + self.dist * sp, self.target[2] + self.dist * cp * cy]
    }
}

/// What a frame draws with.
struct Frame {
    vp: [f32; 16],
    eye: [f32; 3],
    target: [f32; 3],
    span: f32,
    xray: bool,
    hole: f32,
    keep: f32,
    round: i32,
    hero: Option<[f32; 3]>,
    picked: Option<usize>,
    /// The route as a ribbon: x, y, z, r, g, b per vertex, in triangles.
    route: Vec<f32>,
    /// Its shortcut down, a dashed ribbon of its own colour (route.rs `Shortcut`).
    shortcut: Vec<f32>,
}

/// The route as a flat band 2.4 m wide, 0.6 m over the floor it runs on, in the goal's colour as
/// the maps draw it.
fn ribbon(pts: &[[f32; 3]], colour: [u8; 3]) -> Vec<f32> {
    let mut out = Vec::new();
    let n = pts.len();
    if n < 2 {
        return out;
    }
    let c = colour.map(|v| v as f32 / 255.0);
    // each leg's unit side
    let side = |k: usize| {
        let (a, b) = (pts[k], pts[k + 1]);
        let (dx, dz) = (b[0] - a[0], b[2] - a[2]);
        let l = (dx * dx + dz * dz).sqrt().max(1e-3);
        [-dz / l, dx / l]
    };
    // each point's: the mean of its legs' (a mitre), lengthened so the band keeps its width
    // through a bend, at most twice
    let edges: Vec<([f32; 3], [f32; 3])> = (0..n)
        .map(|k| {
            let s = match (k.checked_sub(1).map(side), (k + 1 < n).then(|| side(k))) {
                (Some(a), Some(b)) => {
                    let m = [a[0] + b[0], a[1] + b[1]];
                    let l = (m[0] * m[0] + m[1] * m[1]).sqrt().max(1e-3);
                    let m = [m[0] / l, m[1] / l];
                    let stretch = 1.0 / (m[0] * a[0] + m[1] * a[1]).max(0.5);
                    [m[0] * stretch, m[1] * stretch]
                }
                (Some(a), None) | (None, Some(a)) => a,
                (None, None) => [0.0, 0.0],
            };
            let p = pts[k];
            // just over the floor (a depth offset keeps it on top): 0.6 m looked afloat
            let at = |w: f32| [p[0] + s[0] * w, p[1] + RIBBON_LIFT, p[2] + s[1] * w];
            (at(-RIBBON_HALF), at(RIBBON_HALF))
        })
        .collect();
    // how far along the way each point is (m), for the shader's chevrons and dashes
    let mut along = vec![0.0f32; n];
    for k in 1..n {
        let (a, b) = (pts[k - 1], pts[k]);
        along[k] = along[k - 1] + ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2) + (b[2] - a[2]).powi(2)).sqrt();
    }
    // each vertex: where, across (−1 left, 1 right), along, colour
    let v = |p: [f32; 3], across: f32, s: f32| [p[0], p[1], p[2], across, s, c[0], c[1], c[2]];
    for (k, w) in edges.windows(2).enumerate() {
        let ((l0, r0), (l1, r1)) = (w[0], w[1]);
        let (s0, s1) = (along[k], along[k + 1]);
        for q in [v(l0, -1.0, s0), v(r0, 1.0, s0), v(r1, 1.0, s1), v(l0, -1.0, s0), v(r1, 1.0, s1), v(l1, -1.0, s1)] {
            out.extend(q);
        }
    }
    out
}

/// Floats a ribbon vertex: position, across, along, colour.
const RIBBON_FLOATS: usize = 8;

/// The route ribbon's height over the floor (m).
const RIBBON_LIFT: f32 = 0.15;

/// The route ribbon's half width (m).
const RIBBON_HALF: f32 = 0.9;

/// A double click picks a floor point this near it on the screen (px).
const SPOT_PX: f32 = 10.0;

/// At most this many icons, none over another: the nearest win.
const ICONS_MAX: usize = 220;

/// A toggle chip, on or off at a glance: on, filled in the accent with a check; off, an outline
/// with dim text.
fn chip(ui: &mut egui::Ui, on: &mut bool, label: &str) -> egui::Response {
    let accent = Color32::from_rgb(108, 188, 174);
    let text = if *on { format!("✓ {label}") } else { label.to_string() };
    let galley = ui.painter().layout_no_wrap(text, egui::FontId::proportional(13.0), Color32::WHITE);
    let size = galley.size() + egui::vec2(20.0, 10.0);
    let (rect, mut resp) = ui.allocate_exact_size(size, egui::Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let hover = resp.hovered();
    let (fill, stroke, colour) = if *on {
        (
            if hover { accent.gamma_multiply(1.15) } else { accent },
            egui::Stroke::new(1.0, accent),
            Color32::from_rgb(8, 20, 18),
        )
    } else {
        (
            Color32::from_rgba_unmultiplied(255, 255, 255, if hover { 18 } else { 0 }),
            egui::Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 70)),
            Color32::from_rgb(170, 182, 178),
        )
    };
    ui.painter().rect(rect, 99.0, fill, stroke, egui::StrokeKind::Inside);
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, colour);
    resp
}

/// `rect` (points) in GL pixels, from the bottom of a screen `screen` px: unclamped, so it may
/// start above or below the window.
fn viewport(rect: egui::Rect, ppp: f32, screen: [u32; 2]) -> egui::epaint::ViewportInPixels {
    let left_px = (rect.min.x * ppp).round() as i32;
    let top_px = (rect.min.y * ppp).round() as i32;
    let width_px = (rect.max.x * ppp).round() as i32 - left_px;
    let height_px = (rect.max.y * ppp).round() as i32 - top_px;
    egui::epaint::ViewportInPixels {
        left_px,
        top_px,
        from_bottom_px: screen[1] as i32 - top_px - height_px,
        width_px,
        height_px,
    }
}

/// The maps' colours (raster.rs): the north mark, the marks' outline.
const NORTH: Color32 = Color32::from_rgb(255, 110, 90);
const OUTLINE: Color32 = Color32::from_rgba_premultiplied(0, 0, 0, 200);

/// A goal as the maps draw it: a diamond `s` px from its middle to a corner, outlined, `alpha`
/// opaque.
fn diamond(painter: &egui::Painter, at: egui::Pos2, s: f32, colour: Color32, alpha: f32) {
    let d = |s: f32| {
        vec![at - egui::vec2(0.0, s), at + egui::vec2(s, 0.0), at + egui::vec2(0.0, s), at - egui::vec2(s, 0.0)]
    };
    painter.add(egui::Shape::convex_polygon(d(s + 1.5), OUTLINE.gamma_multiply(alpha), egui::Stroke::NONE));
    painter.add(egui::Shape::convex_polygon(d(s), colour.gamma_multiply(alpha), egui::Stroke::NONE));
}

/// The maps' floor badge (compass::floor_badge) for a mark centred at `c`, `half` across: on
/// another floor (3 m or more up or down), a sky blue (up) or amber (down) disc on a dark rim at
/// the mark's bottom right, a white chevron in it.
fn floor_badge(painter: &egui::Painter, c: egui::Pos2, half: f32, dz_m: f32) {
    if dz_m.abs() < crate::map::compass::FLOOR_DZ {
        return;
    }
    let up = dz_m > 0.0;
    let fill = if up { Color32::from_rgb(70, 160, 235) } else { Color32::from_rgb(230, 140, 40) };
    let r = (half * 0.42).clamp(4.0, 6.5);
    let b = c + egui::vec2(half * 0.72, half * 0.72);
    painter.circle_filled(b, r + 1.3, Color32::from_rgba_unmultiplied(14, 17, 22, 230));
    painter.circle_filled(b, r, fill);
    let (w, h) = (r * 0.6, r * 0.42);
    let (tip, base) = if up { (b.y - h, b.y + h) } else { (b.y + h, b.y - h) };
    painter.add(egui::Shape::convex_polygon(
        vec![egui::pos2(b.x, tip), egui::pos2(b.x - w, base), egui::pos2(b.x + w, base)],
        Color32::WHITE,
        egui::Stroke::NONE,
    ));
}

/// The big map's soft edge (raster::fade_edges) around the hero: full out to 45% of its radius
/// (m), fading to a quarter at it and beyond — a quarter, not nothing, as the 3D map shows the
/// whole region. Scene points are in metres.
fn edge_fade(at: [f32; 3], hero: [f32; 3], radius_m: f32) -> f32 {
    const INNER: f32 = 0.45;
    let d = (at[0] - hero[0]).hypot(at[2] - hero[2]) / radius_m.max(1.0);
    1.0 - 0.75 * ((d - INNER) / (1.0 - INNER)).clamp(0.0, 1.0)
}

fn project(vp: &[f32; 16], p: [f32; 3], rect: egui::Rect) -> Option<egui::Pos2> {
    let c = [
        vp[0] * p[0] + vp[4] * p[1] + vp[8] * p[2] + vp[12],
        vp[1] * p[0] + vp[5] * p[1] + vp[9] * p[2] + vp[13],
        vp[3] * p[0] + vp[7] * p[1] + vp[11] * p[2] + vp[15],
    ];
    if c[2] <= 0.0 {
        return None;
    }
    let (x, y) = (c[0] / c[2], c[1] / c[2]);
    Some(egui::pos2(rect.left() + (x * 0.5 + 0.5) * rect.width(), rect.top() + (0.5 - y * 0.5) * rect.height()))
}

// column-major 4×4 matrices
fn perspective(fovy: f32, aspect: f32, near: f32, far: f32) -> [f32; 16] {
    let f = 1.0 / (fovy / 2.0).tan();
    let mut m = [0.0; 16];
    m[0] = f / aspect;
    m[5] = f;
    m[10] = (far + near) / (near - far);
    m[11] = -1.0;
    m[14] = 2.0 * far * near / (near - far);
    m
}

fn look_at(eye: [f32; 3], at: [f32; 3]) -> [f32; 16] {
    let sub = |a: [f32; 3], b: [f32; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    let norm = |a: [f32; 3]| {
        let l = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt().max(1e-6);
        [a[0] / l, a[1] / l, a[2] / l]
    };
    let cross =
        |a: [f32; 3], b: [f32; 3]| [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
    let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let f = norm(sub(at, eye));
    let s = norm(cross(f, [0.0, 1.0, 0.0]));
    let u = cross(s, f);
    [
        s[0],
        u[0],
        -f[0],
        0.0,
        s[1],
        u[1],
        -f[1],
        0.0,
        s[2],
        u[2],
        -f[2],
        0.0,
        -dot(s, eye),
        -dot(u, eye),
        dot(f, eye),
        1.0,
    ]
}

fn mul(a: &[f32; 16], b: &[f32; 16]) -> [f32; 16] {
    let mut m = [0.0; 16];
    for c in 0..4 {
        for r in 0..4 {
            m[c * 4 + r] = (0..4).map(|k| a[k * 4 + r] * b[c * 4 + k]).sum();
        }
    }
    m
}

const TERRAIN_VS: &str = r#"
layout(location=0) in vec3 aPos; layout(location=1) in vec3 aNormal;
uniform mat4 uVP; out vec3 vW; out vec3 vN;
void main(){ vW = aPos; vN = aNormal; gl_Position = uVP * vec4(aPos, 1.0); }
"#;

const TERRAIN_FS: &str = r#"
in vec3 vW; in vec3 vN; out vec4 o;
uniform float uSpan, uHole, uKeep; uniform vec3 uEye, uFocus;
float ign(vec2 p){ return fract(52.9829189 * fract(dot(p, vec2(0.06711056, 0.00583715)))); }
vec3 ramp(float t){
  vec3 a = vec3(0.035,0.075,0.07), b = vec3(0.09,0.15,0.08), c = vec3(0.22,0.22,0.12), d = vec3(0.42,0.37,0.26);
  return t < 0.35 ? mix(a,b,t/0.35) : t < 0.7 ? mix(b,c,(t-0.35)/0.35) : mix(c,d,(t-0.7)/0.3);
}
void main(){
  vec3 ab = uFocus - uEye; float tt = clamp(dot(vW - uEye, ab)/max(dot(ab,ab),1e-3), 0.0, 1.0);
  float dd = length(vW - (uEye + tt*ab));
  float keep = min(uHole > 0.0 ? smoothstep(uHole*0.7, uHole, dd) : 1.0, uKeep);
  if (keep < ign(gl_FragCoord.xy)) discard;
  vec3 n = normalize(vN); if (!gl_FrontFacing) n = -n;
  vec3 sun = normalize(vec3(-0.6, 0.55, 0.4));
  float diff = max(dot(n, sun), 0.0);
  vec3 base = ramp(clamp(vW.y / max(uSpan, 1.0), 0.0, 1.0));
  base = mix(base, vec3(0.42,0.40,0.37), smoothstep(0.25, 0.6, 1.0 - n.y));
  vec3 col = base * (0.28 + 1.25*diff);
  float h = vW.y;
  float m5 = abs(fract(h/5.0 - 0.5) - 0.5) / max(fwidth(h/5.0), 1e-4);
  float m25 = abs(fract(h/25.0 - 0.5) - 0.5) / max(fwidth(h/25.0), 1e-4);
  col = mix(col, col*0.62, (1.0 - min(m5, 1.0))*0.55);
  col = mix(col, vec3(0.47,0.40,0.25), (1.0 - min(m25, 1.0))*0.6);
  if (!gl_FrontFacing) col = vec3(0.16,0.11,0.07) * (0.6 + 0.4*diff);
  float fog = 1.0 - exp(-pow(length(vW - uEye) * 0.0006, 2.0));
  o = vec4(mix(col, vec3(0.07,0.10,0.11), fog), 1.0);
}
"#;

const FLAT_VS: &str = r#"
layout(location=0) in vec3 aPos; layout(location=1) in vec3 aCol;
uniform mat4 uVP; uniform float uSize; out vec3 vC;
void main(){ vC = aCol; gl_Position = uVP * vec4(aPos, 1.0); gl_PointSize = clamp(uSize * 900.0 / max(gl_Position.w, 1.0), 2.0, 4.0); }
"#;

/// The route's ribbon (Mapbox's and the games' look): a bright core with a dark outline, smooth
/// edges by the distance across (`fwidth`, no multisampling), a still chevron every 3 m along
/// it; dashed for a shortcut, dotted where it is hidden (the X-ray pass).
const ROUTE_VS: &str = r#"
layout(location=0) in vec3 aPos; layout(location=1) in float aV; layout(location=2) in float aS;
layout(location=3) in vec3 aCol;
uniform mat4 uVP; out float vV; out float vS; out vec3 vC;
void main(){ vV = aV; vS = aS; vC = aCol; gl_Position = uVP * vec4(aPos, 1.0); }
"#;

const ROUTE_FS: &str = r#"
in float vV; in float vS; in vec3 vC; out vec4 o;
uniform float uAlpha; uniform float uDash; uniform float uHidden;
void main(){
  float d = abs(vV);
  float aa = max(fwidth(d), 0.001);
  float edge = 1.0 - smoothstep(1.0 - aa, 1.0, d);
  float core = 1.0 - smoothstep(0.62 - aa, 0.62 + aa, d);
  vec3 col = mix(vC * 0.22, vC, core);
  float u = fract(vS / 3.0);
  float chev = 1.0 - smoothstep(0.0, 0.03 + aa, abs(u - 0.4 - d * 0.22) - 0.05);
  col = mix(col, vec3(1.0, 0.96, 0.86), chev * core * (1.0 - uDash) * 0.8);
  float dash = mix(1.0, step(0.5, fract(vS / 1.5)), uDash);
  float hid = mix(1.0, step(0.5, fract(vS / 1.0)), uHidden);
  float a = uAlpha * edge * dash * hid;
  if (a < 0.01) discard;
  o = vec4(col, a);
}
"#;

const FLAT_FS: &str = r#"
in vec3 vC; out vec4 o; uniform float uAlpha; uniform int uRound;
void main(){
  if (uRound == 1){ vec2 q = gl_PointCoord*2.0 - 1.0; float r = dot(q,q); if (r > 1.0) discard; o = vec4(vC * (1.15 - 0.45*r), uAlpha); return; }
  o = vec4(vC, uAlpha);
}
"#;

/// The GPU's copy of a scene and the programs that draw it.
struct Gpu {
    terrain_prog: glow::Program,
    flat_prog: glow::Program,
    route_prog: glow::Program,
    terrain: Option<(glow::VertexArray, glow::Buffer, glow::Buffer, i32)>,
    floors: Vec<(glow::VertexArray, glow::Buffer, i32, usize)>,
    nodes: Option<(glow::VertexArray, glow::Buffer, i32)>,
    marks: Option<(glow::VertexArray, glow::Buffer)>,
    world: String,
    round: i32,
    stale: bool,
    /// The scene as last drawn, off the screen: a frame buffer (colour, depth and stencil) the
    /// view's size, and what it was drawn from. A frame that changes nothing in the scene (the
    /// panel repaints for a snapshot, the pointer, a card) copies it, and draws nothing again.
    cache: Option<Cached>,
}

struct Cached {
    fbo: glow::Framebuffer,
    colour: glow::Renderbuffer,
    depth: glow::Renderbuffer,
    size: (i32, i32),
    key: u64,
}

impl Frame {
    /// What the scene's pixels depend on: the camera and the layers, the round, the route, the
    /// picked place. Not the icons and controls, which egui draws over it.
    fn key(&self, world: &str, size: (i32, i32)) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        world.hash(&mut h);
        size.hash(&mut h);
        for v in self.vp.iter().chain(&self.eye).chain(&self.target).chain([&self.span, &self.hole, &self.keep]) {
            v.to_bits().hash(&mut h);
        }
        (self.xray, self.round, self.picked).hash(&mut h);
        for v in &self.shortcut {
            v.to_bits().hash(&mut h);
        }
        self.hero.map(|p| p.map(f32::to_bits)).hash(&mut h);
        self.route.len().hash(&mut h);
        for v in &self.route {
            v.to_bits().hash(&mut h);
        }
        h.finish()
    }
}

impl Gpu {
    fn new(gl: &glow::Context) -> Option<Gpu> {
        let _t = crate::prof::span("map3d.gpu_new");
        let version = if egui_glow::ShaderVersion::get(gl).is_embedded() {
            "#version 300 es\nprecision highp float;\n"
        } else {
            "#version 330 core\n"
        };
        let compile = |vs: &str, fs: &str| -> Option<glow::Program> {
            // SAFETY: GL calls on the context egui hands the callback, on its thread.
            unsafe {
                let p = gl.create_program().ok()?;
                for (kind, src) in [(glow::VERTEX_SHADER, vs), (glow::FRAGMENT_SHADER, fs)] {
                    let sh = gl.create_shader(kind).ok()?;
                    gl.shader_source(sh, &format!("{version}{src}"));
                    gl.compile_shader(sh);
                    if !gl.get_shader_compile_status(sh) {
                        crate::logfile::line(&format!("map3d shader: {}", gl.get_shader_info_log(sh)));
                        return None;
                    }
                    gl.attach_shader(p, sh);
                }
                gl.link_program(p);
                gl.get_program_link_status(p).then_some(p)
            }
        };
        Some(Gpu {
            terrain_prog: compile(TERRAIN_VS, TERRAIN_FS)?,
            flat_prog: compile(FLAT_VS, FLAT_FS)?,
            route_prog: compile(ROUTE_VS, ROUTE_FS)?,
            terrain: None,
            floors: Vec::new(),
            nodes: None,
            marks: None,
            world: String::new(),
            cache: None,
            round: i32::MIN,
            stale: true,
        })
    }

    /// A vertex array of interleaved floats, `layout` the size of each attribute.
    fn upload(gl: &glow::Context, data: &[f32], layout: &[i32]) -> (glow::VertexArray, glow::Buffer) {
        // SAFETY: as above; the slice outlives the call, GL copies it.
        unsafe {
            let vao = gl.create_vertex_array().unwrap();
            let vbo = gl.create_buffer().unwrap();
            gl.bind_vertex_array(Some(vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
            let bytes = std::slice::from_raw_parts(data.as_ptr() as *const u8, std::mem::size_of_val(data));
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytes, glow::STATIC_DRAW);
            let stride = layout.iter().sum::<i32>() * 4;
            let mut off = 0;
            for (k, &n) in layout.iter().enumerate() {
                gl.enable_vertex_attrib_array(k as u32);
                gl.vertex_attrib_pointer_f32(k as u32, n, glow::FLOAT, false, stride, off);
                off += n * 4;
            }
            gl.bind_vertex_array(None);
            (vao, vbo)
        }
    }

    fn free(&mut self, gl: &glow::Context) {
        // SAFETY: as above.
        unsafe {
            if let Some((a, b, c, _)) = self.terrain.take() {
                gl.delete_vertex_array(a);
                gl.delete_buffer(b);
                gl.delete_buffer(c);
            }
            for (a, b, _, _) in self.floors.drain(..) {
                gl.delete_vertex_array(a);
                gl.delete_buffer(b);
            }
            if let Some((a, b, _)) = self.nodes.take() {
                gl.delete_vertex_array(a);
                gl.delete_buffer(b);
            }
        }
    }

    fn sync(&mut self, gl: &glow::Context, s: &Scene, round: i32) {
        if self.stale || self.world != s.world {
            let _t = crate::prof::span("map3d.upload");
            self.free(gl);
            // new buffers: the cached scene is of the old ones
            if let Some(c) = self.cache.as_mut() {
                c.key = 0;
            }
            let (vao, vbo) = Self::upload(gl, &s.terrain, &[3, 3]);
            // SAFETY: as above.
            let ibo = unsafe {
                gl.bind_vertex_array(Some(vao));
                let ibo = gl.create_buffer().unwrap();
                gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(ibo));
                let bytes = std::slice::from_raw_parts(s.terrain_idx.as_ptr() as *const u8, s.terrain_idx.len() * 4);
                gl.buffer_data_u8_slice(glow::ELEMENT_ARRAY_BUFFER, bytes, glow::STATIC_DRAW);
                gl.bind_vertex_array(None);
                ibo
            };
            self.terrain = Some((vao, vbo, ibo, s.terrain_idx.len() as i32));
            for (k, f) in s.floors.iter().enumerate() {
                if !f.is_empty() {
                    let (a, b) = Self::upload(gl, f, &[3, 3]);
                    self.floors.push((a, b, (f.len() / 6) as i32, k));
                }
            }
            self.world = s.world.clone();
            self.stale = false;
            self.round = i32::MIN;
        }
        if self.round != round {
            if let Some((a, b, _)) = self.nodes.take() {
                // SAFETY: as above.
                unsafe {
                    gl.delete_vertex_array(a);
                    gl.delete_buffer(b);
                }
            }
            let data: Vec<f32> = s
                .nodes
                .iter()
                // a small dot at each place's true spot, under its icon: depth-tested, so the
                // X-ray shows it through the ground
                .filter(|n| n.round >= 0 && n.round <= round)
                .flat_map(|n| [n.at[0], n.at[1] + 1.5, n.at[2], 0.62, 0.68, 0.66])
                .collect();
            let (a, b) = Self::upload(gl, &data, &[3, 3]);
            self.nodes = Some((a, b, (data.len() / 6) as i32));
            self.round = round;
        }
    }

    fn draw(&mut self, gl: &glow::Context, s: &Scene, f: &Frame, info: &egui::PaintCallbackInfo) {
        let _t = crate::prof::span("map3d.gl");
        self.sync(gl, s, f.round);
        let vp = viewport(info.viewport, info.pixels_per_point, info.screen_size_px);
        let clip = info.clip_rect_in_pixels();
        let size = (vp.width_px.max(1), vp.height_px.max(1));
        let key = f.key(&s.world, size);
        // SAFETY: GL calls on the context egui hands the callback, on its thread; the frame
        // buffer egui draws into is put back before the copy.
        unsafe {
            let x0 = vp.left_px.max(clip.left_px);
            let y0 = vp.from_bottom_px.max(clip.from_bottom_px);
            let x1 = (vp.left_px + vp.width_px).min(clip.left_px + clip.width_px);
            let y1 = (vp.from_bottom_px + vp.height_px).min(clip.from_bottom_px + clip.height_px);
            if x1 <= x0 || y1 <= y0 {
                return;
            }
            let window = std::num::NonZeroU32::new(gl.get_parameter_i32(glow::DRAW_FRAMEBUFFER_BINDING) as u32)
                .map(glow::NativeFramebuffer);
            if self.cache.as_ref().is_none_or(|c| c.size != size) {
                self.drop_cache(gl);
                self.cache = Self::make_cache(gl, size);
            }
            let Some(fbo) = self.cache.as_ref().map(|c| c.fbo) else { return };
            if self.cache.as_ref().is_some_and(|c| c.key != key) {
                let _t = crate::prof::span("map3d.render");
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
                gl.disable(glow::SCISSOR_TEST);
                gl.viewport(0, 0, size.0, size.1);
                self.render(gl, s, f);
                if let Some(c) = self.cache.as_mut() {
                    c.key = key;
                }
            }
            // the cached scene into the window, cut to what shows
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(fbo));
            gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, window);
            gl.enable(glow::SCISSOR_TEST);
            gl.scissor(x0, y0, x1 - x0, y1 - y0);
            gl.blit_framebuffer(
                0,
                0,
                size.0,
                size.1,
                vp.left_px,
                vp.from_bottom_px,
                vp.left_px + size.0,
                vp.from_bottom_px + size.1,
                glow::COLOR_BUFFER_BIT,
                glow::NEAREST,
            );
            gl.bind_framebuffer(glow::FRAMEBUFFER, window);
        }
    }

    /// A frame buffer `size` px with a colour and a depth-stencil buffer.
    unsafe fn make_cache(gl: &glow::Context, size: (i32, i32)) -> Option<Cached> {
        let fbo = gl.create_framebuffer().ok()?;
        let colour = gl.create_renderbuffer().ok()?;
        let depth = gl.create_renderbuffer().ok()?;
        gl.bind_renderbuffer(glow::RENDERBUFFER, Some(colour));
        gl.renderbuffer_storage(glow::RENDERBUFFER, glow::RGBA8, size.0, size.1);
        gl.bind_renderbuffer(glow::RENDERBUFFER, Some(depth));
        gl.renderbuffer_storage(glow::RENDERBUFFER, glow::DEPTH24_STENCIL8, size.0, size.1);
        gl.bind_renderbuffer(glow::RENDERBUFFER, None);
        let was = std::num::NonZeroU32::new(gl.get_parameter_i32(glow::FRAMEBUFFER_BINDING) as u32)
            .map(glow::NativeFramebuffer);
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
        gl.framebuffer_renderbuffer(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::RENDERBUFFER, Some(colour));
        gl.framebuffer_renderbuffer(glow::FRAMEBUFFER, glow::DEPTH_STENCIL_ATTACHMENT, glow::RENDERBUFFER, Some(depth));
        let ok = gl.check_framebuffer_status(glow::FRAMEBUFFER) == glow::FRAMEBUFFER_COMPLETE;
        gl.bind_framebuffer(glow::FRAMEBUFFER, was);
        let cached = Cached { fbo, colour, depth, size, key: 0 };
        if ok {
            Some(cached)
        } else {
            gl.delete_framebuffer(cached.fbo);
            gl.delete_renderbuffer(cached.colour);
            gl.delete_renderbuffer(cached.depth);
            None
        }
    }

    fn drop_cache(&mut self, gl: &glow::Context) {
        if let Some(c) = self.cache.take() {
            // SAFETY: as above.
            unsafe {
                gl.delete_framebuffer(c.fbo);
                gl.delete_renderbuffer(c.colour);
                gl.delete_renderbuffer(c.depth);
            }
        }
    }

    /// The scene into the bound frame buffer, its viewport already set.
    fn render(&mut self, gl: &glow::Context, s: &Scene, f: &Frame) {
        // SAFETY: as above; every state changed here is put back for egui at the end.
        unsafe {
            gl.clear_color(0.03, 0.06, 0.07, 1.0);
            gl.clear_depth_f32(1.0);
            gl.clear_stencil(0);
            gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT | glow::STENCIL_BUFFER_BIT);
            gl.enable(glow::DEPTH_TEST);
            gl.depth_func(glow::LEQUAL);
            gl.depth_mask(true);
            gl.disable(glow::CULL_FACE);
            gl.enable(glow::PROGRAM_POINT_SIZE);

            // the ground
            if let Some((vao, _, _, n)) = self.terrain {
                gl.disable(glow::BLEND);
                gl.use_program(Some(self.terrain_prog));
                let u = |name: &str| gl.get_uniform_location(self.terrain_prog, name);
                gl.uniform_matrix_4_f32_slice(u("uVP").as_ref(), false, &f.vp);
                gl.uniform_1_f32(u("uSpan").as_ref(), f.span);
                gl.uniform_1_f32(u("uHole").as_ref(), f.hole);
                gl.uniform_1_f32(u("uKeep").as_ref(), f.keep);
                gl.uniform_3_f32(u("uEye").as_ref(), f.eye[0], f.eye[1], f.eye[2]);
                gl.uniform_3_f32(u("uFocus").as_ref(), f.target[0], f.target[1], f.target[2]);
                gl.bind_vertex_array(Some(vao));
                gl.draw_elements(glow::TRIANGLES, n, glow::UNSIGNED_INT, 0);
            }

            // floors and places, then (X-ray) what is hidden of them
            gl.use_program(Some(self.flat_prog));
            let u = |name: &str| gl.get_uniform_location(self.flat_prog, name);
            gl.uniform_matrix_4_f32_slice(u("uVP").as_ref(), false, &f.vp);
            gl.enable(glow::BLEND);
            gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
            let floors = |alpha: f32| {
                gl.uniform_1_i32(u("uRound").as_ref(), 0);
                for &(vao, _, n, k) in &self.floors {
                    if k == 0 && f.xray {
                        continue;
                    }
                    gl.uniform_1_f32(u("uAlpha").as_ref(), if k == 0 { alpha * 0.45 } else { alpha });
                    gl.bind_vertex_array(Some(vao));
                    gl.draw_arrays(glow::TRIANGLES, 0, n);
                }
            };
            let nodes = |alpha: f32, size: f32| {
                if let Some((vao, _, n)) = self.nodes {
                    gl.uniform_1_i32(u("uRound").as_ref(), 1);
                    gl.uniform_1_f32(u("uAlpha").as_ref(), alpha);
                    gl.uniform_1_f32(u("uSize").as_ref(), size);
                    gl.bind_vertex_array(Some(vao));
                    gl.draw_arrays(glow::POINTS, 0, n);
                }
            };
            gl.enable(glow::STENCIL_TEST);
            gl.stencil_func(glow::ALWAYS, 1, 0xff);
            gl.stencil_op(glow::KEEP, glow::KEEP, glow::REPLACE);
            floors(0.9);
            nodes(1.0, 3.0);
            if f.xray {
                gl.depth_func(glow::GREATER);
                gl.depth_mask(false);
                gl.stencil_func(glow::NOTEQUAL, 1, 0xff);
                floors(0.25);
                nodes(0.45, 3.0);
                gl.depth_mask(true);
                gl.depth_func(glow::LEQUAL);
            }
            gl.disable(glow::STENCIL_TEST);
            // the shortcut, then the route over it: seen, then (X-ray) dotted where hidden; kept
            // on the floor it lies on by a depth offset, not by floating it
            gl.use_program(Some(self.route_prog));
            let ur = |name: &str| gl.get_uniform_location(self.route_prog, name);
            gl.uniform_matrix_4_f32_slice(ur("uVP").as_ref(), false, &f.vp);
            gl.enable(glow::POLYGON_OFFSET_FILL);
            gl.polygon_offset(-1.0, -4.0);
            for (ribbon, dash) in [(&f.shortcut, 1.0), (&f.route, 0.0)] {
                if ribbon.is_empty() {
                    continue;
                }
                let (a, b) = Self::upload(gl, ribbon, &[3, 1, 1, 3]);
                gl.bind_vertex_array(Some(a));
                let n = (ribbon.len() / RIBBON_FLOATS) as i32;
                gl.uniform_1_f32(ur("uDash").as_ref(), dash);
                gl.uniform_1_f32(ur("uHidden").as_ref(), 0.0);
                gl.uniform_1_f32(ur("uAlpha").as_ref(), 0.95);
                gl.draw_arrays(glow::TRIANGLES, 0, n);
                gl.depth_func(glow::GREATER);
                gl.depth_mask(false);
                gl.uniform_1_f32(ur("uHidden").as_ref(), 1.0);
                gl.uniform_1_f32(ur("uAlpha").as_ref(), 0.4);
                gl.draw_arrays(glow::TRIANGLES, 0, n);
                gl.depth_mask(true);
                gl.depth_func(glow::LEQUAL);
                gl.delete_vertex_array(a);
                gl.delete_buffer(b);
            }
            gl.disable(glow::POLYGON_OFFSET_FILL);
            gl.use_program(Some(self.flat_prog));
            // the hero and the picked place, on top
            let mut marks = Vec::new();
            if let Some(h) = f.hero {
                marks.extend([h[0], h[1] + 2.0, h[2], 1.0, 0.82, 0.48]);
            }
            if let Some(n) = f.picked.and_then(|i| s.nodes.get(i)) {
                marks.extend([n.at[0], n.at[1] + 1.5, n.at[2], 1.0, 1.0, 1.0]);
            }
            if !marks.is_empty() {
                if let Some((a, b)) = self.marks.take() {
                    gl.delete_vertex_array(a);
                    gl.delete_buffer(b);
                }
                let (a, b) = Self::upload(gl, &marks, &[3, 3]);
                gl.disable(glow::DEPTH_TEST);
                gl.uniform_1_i32(u("uRound").as_ref(), 1);
                gl.uniform_1_f32(u("uAlpha").as_ref(), 1.0);
                gl.uniform_1_f32(u("uSize").as_ref(), 6.0);
                gl.bind_vertex_array(Some(a));
                gl.draw_arrays(glow::POINTS, 0, (marks.len() / 6) as i32);
                self.marks = Some((a, b));
            }

            gl.bind_vertex_array(None);
            gl.use_program(None);
            gl.disable(glow::DEPTH_TEST);
            gl.disable(glow::PROGRAM_POINT_SIZE);
            gl.enable(glow::BLEND);
            gl.blend_func_separate(glow::ONE, glow::ONE_MINUS_SRC_ALPHA, glow::ONE_MINUS_DST_ALPHA, glow::ONE);
        }
    }
}

impl super::Panel {
    /// The 3D map as the Map page's first card: the view as tall as fits; what to run when the
    /// region's files are not there yet.
    pub(super) fn map3d_card(
        &mut self,
        t: &mut egui_taffy::Tui,
        state: &crate::minimap::MapState,
        snap: Option<&crate::engine::Snapshot>,
    ) {
        use crate::ui::tw::{self, card, note};
        let world = snap
            .and_then(|s| s.world.clone())
            .map(|w| crate::survey::Survey::world_of(&w).to_string())
            .unwrap_or_default();
        let hero = snap.and_then(|s| s.pose).map(|(p, yaw)| ([p[0] as f32, p[1] as f32, p[2] as f32], yaw as f32));
        let (route, colour) = self.shared.route3d.lock().unwrap().clone();
        let shortcut = self.shared.shortcut3d.lock().unwrap().clone();
        self.map3d.want(&world);
        // Sending the hero to a spot is a cheat: with that consent, the hero in play.
        self.map3d.can_teleport = self.grants(crate::settings::Consent::CHEATS) && snap.is_some_and(|s| s.gate.is_ok());
        let full_world = snap.and_then(|s| s.world.clone()).unwrap_or_default();
        card(t, &format!("{} · {}", tr!("MAP3D"), if world.is_empty() { "—" } else { world.as_str() }), |t| {
            if self.map3d.missing() {
                note(t, tr!("MAP3D_MISSING"));
                return;
            }
            if self.map3d.loading() && self.map3d.scene().is_none() {
                note(t, tr!("MAP3D_LOADING"));
            }
            tw::block(t, |ui| {
                let height = (ui.ctx().content_rect().height() * 0.62).clamp(380.0, 720.0);
                self.map3d.view(ui, height, hero, &route, colour, state, &shortcut);
                if let Some(at) = self.map3d.film_point.take() {
                    self.film.points.push(at);
                }
                if let Some(at) = self.map3d.teleport.take() {
                    let _ = self.tx.send(crate::ui::Request::TeleportHere(full_world.clone(), at));
                    // "back to where it was" in the teleport card
                    self.went = true;
                }
            });
        });
    }

    /// The game view's 3D layer (overlay/screenroute.rs): the route laid on the floor and the
    /// maps' icons over what is near, each hidden where the world hides it.
    pub(super) fn screen3d_body(&mut self, t: &mut egui_taffy::Tui, state: &mut crate::minimap::MapState) {
        use crate::ui::tw::{self, note};
        tw::switch(t, &mut state.screen_route, tr!("SCREEN3D_ROUTE"));
        tw::switch(t, &mut state.screen_marks, tr!("SCREEN3D_MARKS"));
        tw::switch(t, &mut state.streamer, tr!("STREAMER_MODE"));
        note(t, tr!("SCREEN3D_NOTE"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_viewport_keeps_its_size_when_scrolled_out() {
        // a 400 x 300 view scrolled 100 px above a 1000 x 800 window
        let r = egui::Rect::from_min_size(egui::pos2(50.0, -100.0), egui::vec2(400.0, 300.0));
        let v = viewport(r, 1.0, [1000, 800]);
        assert_eq!((v.left_px, v.width_px, v.height_px), (50, 400, 300));
        assert_eq!(v.from_bottom_px, 800 + 100 - 300);
    }

    #[test]
    fn a_spot_on_the_map_is_the_place_it_was_in_the_game() {
        let s = Scene { origin: [1200.0, -3400.0, 500.0], ..Default::default() };
        let p = [-15000.0, 22000.0, 3100.0];
        let back = s.to_game(s.to_scene(p));
        assert!((0..3).all(|k| (back[k] - p[k]).abs() < 0.01), "{back:?}");
    }

    #[test]
    fn base64_reads_back() {
        assert_eq!(base64("AAH/fw=="), vec![0x00, 0x01, 0xff, 0x7f]);
    }

    #[test]
    fn the_view_sees_what_is_in_front() {
        let vp = mul(&perspective(1.0, 1.0, 1.0, 1000.0), &look_at([0.0, 0.0, 10.0], [0.0; 3]));
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(100.0, 100.0));
        let p = project(&vp, [0.0; 3], rect).unwrap();
        assert!((p.x - 50.0).abs() < 0.01 && (p.y - 50.0).abs() < 0.01, "the target in the middle");
        assert!(project(&vp, [0.0, 0.0, 20.0], rect).is_none(), "behind the eye");
    }
}
