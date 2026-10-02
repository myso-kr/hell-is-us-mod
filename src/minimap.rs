//! The minimap's state: the path walked and the markers placed, per world — and how
//! a point in the world lands on the map. No drawing and no window here (ui/minimap.rs);
//! everything in this file runs in tests.
//!
//! Unreal's axes: X forward ("north" here, since the game has none), Y right, Z up;
//! yaw is degrees from +X towards +Y, so it turns clockwise seen from above. The map
//! is either north-up (+X up) or heading-up (the camera's facing up).

use crate::actors::Sub;
use std::collections::{BTreeMap, BTreeSet};

/// A trail point is only added this far (cm) from the last one: 3 m.
const STEP: f32 = 300.0;
/// A jump further than this (cm) between two readings is a teleport or a load, not
/// a walk: the trail breaks there instead of drawing a line across the map.
const JUMP: f32 = 5_000.0;
/// Oldest points go first past this many per world.
const MAX_TRAIL: usize = 6_000;
/// The marker key next to a marker removes it rather than adding a second: within 5 m.
const NEAR: f32 = 500.0;

pub type Point = [f32; 3];

#[derive(Clone, Debug, PartialEq)]
pub struct MapState {
    pub show: bool,
    /// Heading-up when true, north-up when false.
    pub heading_up: bool,
    /// How many metres the map's radius covers.
    pub radius_m: f32,
    /// The function keys (1–12) that show/hide the map and drop a marker. F7, the
    /// first choice, is the game's photo mode, so the player picks.
    pub toggle_key: u8,
    pub marker_key: u8,
    /// Which kinds of thing are drawn, one bit per `actors::Kind`.
    pub layers: u8,
    /// Finer sorts switched off within a kind that is on.
    pub hidden: BTreeSet<Sub>,
    /// How big the icons are, in pixels.
    pub icon_px: u8,
    /// Whether the level's walls and floors are drawn under everything else.
    pub terrain: bool,
    /// How the landscape itself is drawn under everything: shaded, contoured, both.
    pub relief: ReliefMode,
    /// The compass strip at the top of the game window, and its key.
    pub compass: bool,
    pub compass_key: u8,
    /// The key that moves the guide to the next place.
    pub cycle_key: u8,
    /// Guide to the nearest quest goal whenever nothing is chosen.
    pub guide_auto: bool,
    /// Which tiers of goal are shown, one bit per `goals::Tier`.
    pub goal_tiers: u8,
    /// The world yaw the game calls north (degrees): 270 in Hell Is Us, found by
    /// comparing with the game's compass item. Kept, in case an area differs.
    pub north_yaw: f32,
    /// How opaque the big map is, in percent (20–100).
    pub big_alpha: u8,
    /// The big map in the middle of the game window, and its key and radius (m).
    pub big_key: u8,
    pub big_radius_m: f32,
    /// Shown now; not kept across runs.
    pub big: bool,
    /// Hide every overlay while a game menu is open (the game shows its cursor, or is
    /// paused).
    pub hide_in_menus: bool,
    /// Draw a walking route (A*) to the guide's goal, not just a straight line.
    pub route: bool,
    /// The goal being guided to, by actor — chosen in the panel or with the cycle key.
    /// Not kept across runs: actors are new each time.
    pub target: Option<u64>,
    /// Per world: the trail, with `None` where it breaks.
    pub trails: BTreeMap<String, Vec<Option<Point>>>,
    pub markers: BTreeMap<String, Vec<Point>>,
    /// Changed since last saved.
    pub dirty: bool,
}

impl Default for MapState {
    fn default() -> MapState {
        MapState {
            show: true,
            heading_up: true,
            radius_m: 60.0,
            toggle_key: 9,
            marker_key: 6,
            layers: ALL_LAYERS,
            hidden: BTreeSet::new(),
            icon_px: 16,
            terrain: true,
            relief: ReliefMode::Both,
            compass: true,
            compass_key: 10,
            cycle_key: 11,
            guide_auto: true,
            goal_tiers: 0b111,
            target: None,
            north_yaw: 270.0,
            big_key: 3,
            big_radius_m: 250.0,
            big_alpha: 100,
            big: false,
            hide_in_menus: true,
            route: true,
            trails: BTreeMap::new(),
            markers: BTreeMap::new(),
            dirty: false,
        }
    }
}

fn dist(a: Point, b: Point) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

impl MapState {
    /// A position reading. Extends the world's trail when the hero has moved far
    /// enough; breaks it on a jump.
    pub fn observe(&mut self, world: &str, p: Point) {
        if !p.iter().all(|v| v.is_finite()) {
            return;
        }
        let trail = self.trails.entry(world.to_string()).or_default();
        match trail.iter().rev().find_map(|x| *x) {
            Some(last) if dist(last, p) < STEP => return,
            Some(last) if dist(last, p) > JUMP && trail.last().is_some_and(Option::is_some) => trail.push(None),
            _ => {}
        }
        trail.push(Some(p));
        if trail.len() > MAX_TRAIL {
            let cut = trail.len() - MAX_TRAIL;
            trail.drain(..cut);
        }
        self.dirty = true;
    }

    /// The marker key: a marker here, or — standing next to one — that one removed. Returns
    /// whether a marker was added.
    pub fn toggle_marker(&mut self, world: &str, p: Point) -> bool {
        let list = self.markers.entry(world.to_string()).or_default();
        self.dirty = true;
        if let Some(i) = list.iter().position(|m| dist(*m, p) < NEAR) {
            list.remove(i);
            false
        } else {
            list.push(p);
            true
        }
    }

    pub fn clear_trail(&mut self, world: &str) {
        self.trails.remove(world);
        self.dirty = true;
    }

    pub fn clear_markers(&mut self, world: &str) {
        self.markers.remove(world);
        self.dirty = true;
    }

    /// The text `minimap.txt` holds.
    pub fn render(&self) -> String {
        let mut out = format!(
            "show {}\nheading_up {}\nradius {}\ntoggle_key {}\nmarker_key {}\nlayers {}\nlayers_version {LAYERS_VERSION}\nicon_px {}\nterrain {}\nrelief {}\ncompass {}\ncompass_key {}\ncycle_key {}\nguide_auto {}\ngoal_tiers {}\nbig_key {}\nbig_radius {}\nbig_alpha {}\nhide_in_menus {}\nroute {}\nnorth_yaw {}\n",
            self.show,
            self.heading_up,
            self.radius_m,
            self.toggle_key,
            self.marker_key,
            self.layers,
            self.icon_px,
            self.terrain,
            self.relief.key(),
            self.compass,
            self.compass_key,
            self.cycle_key,
            self.guide_auto,
            self.goal_tiers,
            self.big_key,
            self.big_radius_m,
            self.big_alpha,
            self.hide_in_menus,
            self.route,
            self.north_yaw
        );
        for s in &self.hidden {
            out += &format!("hide {}\n", s.id());
        }
        for (world, list) in &self.markers {
            for m in list {
                out += &format!("marker {world} {} {} {}\n", m[0], m[1], m[2]);
            }
        }
        for (world, trail) in &self.trails {
            for p in trail {
                out += &match p {
                    Some(p) => format!("trail {world} {} {} {}\n", p[0], p[1], p[2]),
                    None => format!("break {world}\n"),
                };
            }
        }
        out
    }

    /// A broken line costs only itself.
    pub fn parse(text: &str) -> MapState {
        let mut s = MapState::default();
        let mut layers_version = 0u8;
        let point = |f: &[&str]| -> Option<Point> {
            let v: Vec<f32> = f.iter().filter_map(|x| x.parse().ok()).filter(|v: &f32| v.is_finite()).collect();
            (v.len() == 3).then(|| [v[0], v[1], v[2]])
        };
        for line in text.lines() {
            let f: Vec<&str> = line.split_whitespace().collect();
            match f[..] {
                ["show", v] => s.show = v == "true",
                ["heading_up", v] => s.heading_up = v == "true",
                ["radius", v] => {
                    if let Some(r) = v.parse::<f32>().ok().filter(|r| (10.0..=1000.0).contains(r)) {
                        s.radius_m = r;
                    }
                }
                ["terrain", v] => s.terrain = v == "true",
                ["relief", v] => {
                    if let Some(m) = ReliefMode::ALL.into_iter().find(|m| m.key() == v) {
                        s.relief = m;
                    }
                }
                ["compass", v] => s.compass = v == "true",
                ["hide_in_menus", v] => s.hide_in_menus = v == "true",
                ["route", v] => s.route = v == "true",
                ["north_yaw", v] => {
                    if let Some(n) = v.parse::<f32>().ok().filter(|n| (0.0..360.0).contains(n)) {
                        s.north_yaw = n;
                    }
                }
                ["big_alpha", v] => {
                    if let Some(a) = v.parse::<u8>().ok().filter(|a| (20..=100).contains(a)) {
                        s.big_alpha = a;
                    }
                }
                ["big_radius", v] => {
                    if let Some(r) = v.parse::<f32>().ok().filter(|r| (50.0..=2000.0).contains(r)) {
                        s.big_radius_m = r;
                    }
                }
                ["big_key", v] => {
                    if let Some(k) = v.parse().ok().filter(|k| usable_key(*k)) {
                        s.big_key = k;
                    }
                }
                ["guide_auto", v] => s.guide_auto = v == "true",
                ["goal_tiers", v] => {
                    if let Ok(b) = v.parse::<u8>() {
                        s.goal_tiers = b & 0b111;
                    }
                }
                ["compass_key", v] => {
                    if let Some(k) = v.parse().ok().filter(|k| usable_key(*k)) {
                        s.compass_key = k;
                    }
                }
                ["cycle_key", v] => {
                    if let Some(k) = v.parse().ok().filter(|k| usable_key(*k)) {
                        s.cycle_key = k;
                    }
                }
                ["icon_px", v] => {
                    if let Some(px) = v.parse::<u8>().ok().filter(|p| ICON_PX.contains(p)) {
                        s.icon_px = px;
                    }
                }
                ["hide", id] => {
                    if let Some(sub) = Sub::ALL.into_iter().find(|x| x.id() == id) {
                        s.hidden.insert(sub);
                    }
                }
                ["layers_version", v] => layers_version = v.parse().unwrap_or(0),
                ["layers", v] => {
                    if let Ok(b) = v.parse::<u8>() {
                        s.layers = b & ALL_LAYERS;
                    }
                }
                ["toggle_key", v] => {
                    if let Some(k) = v.parse().ok().filter(|k| usable_key(*k)) {
                        s.toggle_key = k;
                    }
                }
                ["marker_key", v] => {
                    if let Some(k) = v.parse().ok().filter(|k| usable_key(*k)) {
                        s.marker_key = k;
                    }
                }
                ["marker", w, ..] if f.len() == 5 => {
                    if let Some(p) = point(&f[2..]) {
                        s.markers.entry(w.to_string()).or_default().push(p);
                    }
                }
                ["trail", w, ..] if f.len() == 5 => {
                    if let Some(p) = point(&f[2..]) {
                        s.trails.entry(w.to_string()).or_default().push(Some(p));
                    }
                }
                ["break", w] => s.trails.entry(w.to_string()).or_default().push(None),
                _ => {}
            }
        }
        // A file from before the save-point kind: that kind starts on.
        if layers_version < 2 {
            s.layers |= crate::actors::Kind::Save.bit();
        }
        // Five keys, five different keys — or all back to their defaults.
        let keys = s.keys();
        if (0..keys.len()).any(|i| keys[i + 1..].contains(&keys[i])) {
            (s.toggle_key, s.marker_key, s.compass_key, s.cycle_key, s.big_key) = (9, 6, 10, 11, 3);
        }
        s
    }
}

/// The icon sizes the panel offers.
pub const ICON_PX: std::ops::RangeInclusive<u8> = 10..=32;

impl MapState {
    /// Whether a thing of this sort is drawn: its kind on, and the sort not hidden.
    pub fn shows(&self, sub: Sub) -> bool {
        self.layers & sub.kind().bit() != 0 && !self.hidden.contains(&sub)
    }
}

impl MapState {
    /// Every key this map uses: map, marker, compass, cycle, big map.
    pub fn keys(&self) -> [u8; 5] {
        [self.toggle_key, self.marker_key, self.compass_key, self.cycle_key, self.big_key]
    }
}

/// Every `actors::Kind` drawn.
pub const ALL_LAYERS: u8 = 0b11_1111;
/// The layer bits as of this version; files without it predate the save-point kind.
const LAYERS_VERSION: u8 = 2;

/// F1–F12, except F8: that one is the panel's.
pub fn usable_key(k: u8) -> bool {
    (1..=12).contains(&k) && k != 8
}

/// How the landscape is drawn on the map.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReliefMode {
    Off,
    /// Hill shading, tinted by height against the hero's.
    Shade,
    /// Contour lines: thin every 2 m, strong every 10 m.
    Contour,
    Both,
}

impl ReliefMode {
    pub const ALL: [ReliefMode; 4] = [ReliefMode::Off, ReliefMode::Shade, ReliefMode::Contour, ReliefMode::Both];

    /// As kept in the settings file.
    pub fn key(self) -> &'static str {
        match self {
            ReliefMode::Off => "off",
            ReliefMode::Shade => "shade",
            ReliefMode::Contour => "contour",
            ReliefMode::Both => "both",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ReliefMode::Off => "끔",
            ReliefMode::Shade => "음영",
            ReliefMode::Contour => "등고선",
            ReliefMode::Both => "둘 다",
        }
    }

    pub fn shade(self) -> bool {
        matches!(self, ReliefMode::Shade | ReliefMode::Both)
    }

    pub fn contour(self) -> bool {
        matches!(self, ReliefMode::Contour | ReliefMode::Both)
    }
}

/// Where the map stands and faces: the hero's position and the camera's yaw.
#[derive(Clone, Copy, Debug)]
pub struct View {
    pub center: Point,
    pub yaw_deg: f32,
    pub heading_up: bool,
    /// Pixels per centimetre.
    pub scale: f32,
    /// The world yaw the game calls north (degrees). In Hell Is Us that is 270: what
    /// the game's own compass shows as north lies along −Y.
    pub north_deg: f32,
}

impl View {
    /// A world point as an offset in pixels from the map's centre, y down.
    pub fn project(&self, p: Point) -> (f32, f32) {
        let (dx, dy) = (p[0] - self.center[0], p[1] - self.center[1]);
        // Whatever is up on the map: the camera's facing, or north.
        let up = if self.heading_up { self.yaw_deg } else { self.north_deg };
        let (s, c) = up.to_radians().sin_cos();
        let forward = dx * c + dy * s;
        let right = -dx * s + dy * c;
        (right * self.scale, -forward * self.scale)
    }

    /// The world X/Y under a pixel offset from the map's centre — `project` undone.
    pub fn unproject(&self, x: f32, y: f32) -> [f32; 2] {
        let up = if self.heading_up { self.yaw_deg } else { self.north_deg };
        let (s, c) = up.to_radians().sin_cos();
        let (right, forward) = (x / self.scale, -y / self.scale);
        [self.center[0] + forward * c - right * s, self.center[1] + forward * s + right * c]
    }

    /// Which way the hero's arrow points on the map, as a unit vector, y down.
    pub fn heading(&self) -> (f32, f32) {
        if self.heading_up {
            (0.0, -1.0)
        } else {
            let (s, c) = (self.yaw_deg - self.north_deg).to_radians().sin_cos();
            (s, -c)
        }
    }

    /// Which way north is on the map, as a unit vector, y down.
    pub fn north(&self) -> (f32, f32) {
        let (s, c) = self.north_deg.to_radians().sin_cos();
        let (x, y) = self.project([self.center[0] + c, self.center[1] + s, self.center[2]]);
        let len = (x * x + y * y).sqrt().max(f32::EPSILON);
        (x / len, y / len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: (f32, f32), b: (f32, f32)) -> bool {
        (a.0 - b.0).abs() < 1e-3 && (a.1 - b.1).abs() < 1e-3
    }

    #[test]
    fn north_up_puts_x_up_and_y_right() {
        let v = View { center: [0.0; 3], yaw_deg: 0.0, heading_up: false, scale: 1.0, north_deg: 0.0 };
        assert!(close(v.project([100.0, 0.0, 0.0]), (0.0, -100.0)));
        assert!(close(v.project([0.0, 100.0, 0.0]), (100.0, 0.0)));
        assert!(close(v.north(), (0.0, -1.0)));
        // Facing +Y (yaw 90) points the arrow right.
        let v = View { yaw_deg: 90.0, ..v };
        assert!(close(v.heading(), (1.0, 0.0)));
    }

    #[test]
    fn unproject_undoes_project() {
        for (heading_up, yaw) in [(false, 30.0), (true, 123.0)] {
            let v = View { center: [100.0, -50.0, 0.0], yaw_deg: yaw, heading_up, scale: 0.02, north_deg: 270.0 };
            let (x, y) = v.project([900.0, 400.0, 0.0]);
            let w = v.unproject(x, y);
            assert!((w[0] - 900.0).abs() < 0.1 && (w[1] - 400.0).abs() < 0.1, "{w:?}");
        }
    }

    #[test]
    fn north_up_follows_the_games_north() {
        // The game's north along -Y (yaw 270). Yaw turns clockwise seen from above, so
        // facing -Y the right hand points along +X (yaw 0 = 270 + 90): the game's east.
        let v = View { center: [0.0; 3], yaw_deg: 270.0, heading_up: false, scale: 1.0, north_deg: 270.0 };
        let close = |a: (f32, f32), b: (f32, f32)| (a.0 - b.0).abs() < 1e-3 && (a.1 - b.1).abs() < 1e-3;
        assert!(close(v.project([0.0, -100.0, 0.0]), (0.0, -100.0)), "north is up");
        assert!(close(v.project([100.0, 0.0, 0.0]), (100.0, 0.0)), "+X is the game's east");
        assert!(close(v.north(), (0.0, -1.0)));
        assert!(close(v.heading(), (0.0, -1.0)), "facing the game's north, the arrow points up");
    }

    #[test]
    fn heading_up_puts_what_is_ahead_up() {
        let v = View { center: [0.0; 3], yaw_deg: 90.0, heading_up: true, scale: 0.5, north_deg: 0.0 };
        // Facing +Y: a point ahead on +Y is up, +X (north) is on the left.
        assert!(close(v.project([0.0, 100.0, 0.0]), (0.0, -50.0)));
        assert!(close(v.project([100.0, 0.0, 0.0]), (-50.0, 0.0)));
        assert!(close(v.north(), (-1.0, 0.0)));
        assert!(close(v.heading(), (0.0, -1.0)));
    }

    #[test]
    fn the_trail_grows_by_steps_and_breaks_on_jumps() {
        let mut s = MapState::default();
        s.observe("W", [0.0, 0.0, 0.0]);
        s.observe("W", [100.0, 0.0, 0.0]); // too close
        s.observe("W", [400.0, 0.0, 0.0]);
        s.observe("W", [90_000.0, 0.0, 0.0]); // teleport
        s.observe("W", [f32::NAN, 0.0, 0.0]);
        assert_eq!(s.trails["W"], [Some([0.0, 0.0, 0.0]), Some([400.0, 0.0, 0.0]), None, Some([90_000.0, 0.0, 0.0])]);
        s.observe("Other", [0.0, 0.0, 0.0]);
        assert_eq!(s.trails.len(), 2, "kept per world");
    }

    #[test]
    fn f6_beside_a_marker_removes_it() {
        let mut s = MapState::default();
        assert!(s.toggle_marker("W", [0.0, 0.0, 0.0]));
        assert!(s.toggle_marker("W", [1000.0, 0.0, 0.0]));
        assert!(!s.toggle_marker("W", [100.0, 100.0, 0.0]));
        assert_eq!(s.markers["W"], [[1000.0, 0.0, 0.0]]);
    }

    #[test]
    fn round_trips_and_shrugs_off_bad_lines() {
        let mut s = MapState {
            show: false,
            heading_up: false,
            radius_m: 120.0,
            layers: 0b101,
            icon_px: 22,
            terrain: false,
            relief: ReliefMode::Contour,
            compass: false,
            compass_key: 2,
            cycle_key: 3,
            guide_auto: false,
            goal_tiers: 0b101,
            big_key: 4,
            big_radius_m: 400.0,
            big_alpha: 60,
            north_yaw: 90.0,
            hide_in_menus: false,
            route: false,
            hidden: [Sub::Lore, Sub::Door].into_iter().collect(),
            ..MapState::default()
        };
        s.toggle_marker("Map_A", [1.5, -2.0, 3.0]);
        s.observe("Map_A", [0.0, 0.0, 0.0]);
        s.observe("Map_A", [90_000.0, 0.0, 0.0]);
        s.dirty = false;
        assert_eq!(MapState::parse(&s.render()), s);
        let old = MapState::parse("layers 31\n");
        assert_eq!(old.layers, ALL_LAYERS, "a file from before save points turns them on");
        let off = MapState::parse("layers 31\nlayers_version 2\n");
        assert_eq!(off.layers, 31, "a file that knows of them keeps them off");
        let k = MapState::parse("toggle_key 12\nmarker_key 5\n");
        assert_eq!((k.toggle_key, k.marker_key), (12, 5));
        let k = MapState::parse("toggle_key 8\nmarker_key 13\n");
        assert_eq!((k.toggle_key, k.marker_key), (9, 6), "F8 is the panel's; F13 is no key");
        let k = MapState::parse("toggle_key 6\n");
        assert_eq!((k.toggle_key, k.marker_key), (9, 6), "one key cannot do both");
        let k = MapState::parse("compass_key 6\n");
        assert_eq!((k.marker_key, k.compass_key), (6, 10), "nor can the compass take the marker's");
        let t = MapState::parse("radius 99999\nmarker W 1 2\ntrail W a b c\nshow false\n???\n");
        assert_eq!(t.radius_m, 60.0);
        assert!(t.markers.is_empty() && t.trails.is_empty());
        assert!(!t.show);
    }
}
