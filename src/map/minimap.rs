//! The minimap's state: the path walked and the pins placed, per world, the map's
//! settings, and `minimap.txt`. No drawing and no window here (ui/minimap.rs);
//! everything in this file runs in tests. The pins (pins.rs) and the projection
//! (view.rs) are re-exported from here.

use crate::actors::Sub;
use std::collections::{BTreeMap, BTreeSet};

pub use super::pins::*;
pub use super::view::*;

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

/// The largest radius either map draws (m): past about this the game has not loaded the
/// land, and a wider map shows its edge cut off.
pub const RADIUS_MAX: f32 = 400.0;

#[derive(Clone, Debug, PartialEq)]
pub struct MapState {
    /// Which map is up: none, the minimap, or the big map — and which of those the
    /// map key steps through, one bit per `Display::ALL`.
    pub display: Display,
    pub cycle: u8,
    /// Heading-up when true, north-up when false.
    pub heading_up: bool,
    /// How many metres the map's radius covers.
    pub radius_m: f32,
    /// The function keys that show/hide the map and drop a marker (`usable_key`: not the
    /// game's or Steam's).
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
    /// The quest the guide follows, by its journal key — `None` follows the main
    /// story (quests.rs `followed`).
    pub quest: Option<String>,
    /// The quest tracker at the right of the screen.
    pub tracker: bool,
    /// The target was picked by hand (the goal list, the cycle key): auto guiding
    /// leaves it until it is used up. Not kept across runs.
    pub chosen: bool,
    /// A place picked in the panel that is no goal of the moment (a collectible, an NPC
    /// out of range): (world, id, where, label). The overlay guides to it as to a goal.
    pub adhoc: Option<(String, u64, Point, String)>,
    /// Goals the player skipped (done in a way the guide could not see): auto guiding
    /// passes them by this run.
    pub skipped: std::collections::HashSet<u64>,
    /// The world yaw the game calls north (degrees): 270 in Hell Is Us, found by
    /// comparing with the game's compass item. Kept, in case an area differs.
    pub north_yaw: f32,
    /// How opaque the big map is, in percent (20–100).
    pub big_alpha: u8,
    /// The big map and the minimap drawn as outlines on a clear background.
    pub big_outline: bool,
    pub mini_outline: bool,
    /// The big map's ground drawn as dots with gaps between them, so the game shows
    /// through (as Diablo's and Path of Exile's maps): icons, route and pins stay solid.
    /// The minimap, small and in a corner, stays solid.
    pub dots: bool,
    /// Not a setting: the Hazes and the Hollow Walkers they keep alive, put here by the
    /// overlay from each snapshot so every map draws them. Never saved.
    pub haze_links: Vec<crate::actors::HazeLink>,
    /// How opaque the map's layers are (percent): the ground (disc, relief, terrain
    /// fills), the lines (contours, shore, edges, trail, route) and the icons (things,
    /// pins, goals, the hero). Both maps; the big map's own opacity multiplies them.
    pub opacity: [u8; 3],
    /// The big map's radius (m).
    pub big_radius_m: f32,
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
    pub markers: BTreeMap<String, Vec<Marker>>,
    /// The kind the marker key's next pin gets.
    pub pin_kind: PinKind,
    /// Changed since last saved.
    pub dirty: bool,
}

impl Default for MapState {
    fn default() -> MapState {
        MapState {
            display: Display::Mini,
            cycle: 0b111,
            heading_up: true,
            radius_m: 60.0,
            toggle_key: DEFAULT_KEYS[0],
            marker_key: DEFAULT_KEYS[1],
            layers: ALL_LAYERS,
            hidden: BTreeSet::new(),
            icon_px: 16,
            terrain: true,
            relief: ReliefMode::Both,
            compass: true,
            compass_key: DEFAULT_KEYS[2],
            cycle_key: DEFAULT_KEYS[3],
            guide_auto: true,
            goal_tiers: 0b111,
            quest: None,
            tracker: true,
            chosen: false,
            skipped: Default::default(),
            adhoc: None,
            target: None,
            north_yaw: 270.0,
            big_radius_m: 250.0,
            big_alpha: 100,
            big_outline: true,
            mini_outline: false,
            dots: true,
            haze_links: Vec::new(),
            opacity: [100, 100, 100],
            hide_in_menus: true,
            route: true,
            trails: BTreeMap::new(),
            markers: BTreeMap::new(),
            pin_kind: PinKind::Mark,
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
        if let Some(i) = list.iter().position(|m| dist(m.at, p) < NEAR) {
            list.remove(i);
            false
        } else {
            list.push(Marker { at: p, kind: self.pin_kind, note: String::new() });
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
            "display {}\ncycle_modes {}\nheading_up {}\nradius {}\ntoggle_key {}\nmarker_key {}\nlayers {}\nlayers_version {LAYERS_VERSION}\nicon_px {}\nterrain {}\nrelief {}\ncompass {}\ncompass_key {}\ncycle_key {}\nguide_auto {}\ngoal_tiers {}\nbig_radius {}\nbig_alpha {}\nbig_outline {}\nmini_outline {}\ndots {}\nopacity {} {} {}\nhide_in_menus {}\nroute {}\nnorth_yaw {}\ntracker {}\n",
            self.display.key(),
            self.cycle,
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
            self.big_radius_m,
            self.big_alpha,
            self.big_outline,
            self.mini_outline,
            self.dots,
            self.opacity[0],
            self.opacity[1],
            self.opacity[2],
            self.hide_in_menus,
            self.route,
            self.north_yaw,
            self.tracker
        );
        if let Some(q) = &self.quest {
            out += &format!("quest {q}\n");
        }
        for s in &self.hidden {
            out += &format!("hide {}\n", s.id());
        }
        out += &format!("pin_kind {}\n", self.pin_kind.word());
        for (world, list) in &self.markers {
            for m in list {
                let note = m.note.replace(['\n', '\r'], " ");
                out += &format!("marker {world} {} {} {} {} {note}\n", m.at[0], m.at[1], m.at[2], m.kind.word());
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
                // Before display modes: "show false" was the minimap off.
                ["show", v] => {
                    if v != "true" {
                        s.display = Display::Off;
                    }
                }
                ["display", v] => {
                    if let Some(d) = Display::ALL.into_iter().find(|d| d.key() == v) {
                        s.display = d;
                    }
                }
                ["cycle_modes", v] => {
                    if let Some(c) = v.parse::<u8>().ok().filter(|c| *c & 0b111 != 0) {
                        s.cycle = c & 0b111;
                    }
                }
                ["heading_up", v] => s.heading_up = v == "true",
                ["radius", v] => {
                    if let Some(r) = v.parse::<f32>().ok().filter(|r| (10.0..=1000.0).contains(r)) {
                        s.radius_m = r.min(RADIUS_MAX);
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
                ["big_outline", v] => s.big_outline = v == "true",
                ["mini_outline", v] => s.mini_outline = v == "true",
                ["dots", v] => s.dots = v == "true",
                ["opacity", g, l, i] => {
                    if let (Ok(g), Ok(l), Ok(i)) = (g.parse::<u8>(), l.parse::<u8>(), i.parse::<u8>()) {
                        s.opacity = [g.min(100), l.min(100), i.min(100)];
                    }
                }
                ["big_alpha", v] => {
                    if let Some(a) = v.parse::<u8>().ok().filter(|a| (20..=100).contains(a)) {
                        s.big_alpha = a;
                    }
                }
                ["big_radius", v] => {
                    if let Some(r) = v.parse::<f32>().ok().filter(|r| (50.0..=2000.0).contains(r)) {
                        // At most RADIUS_MAX: a setting saved larger is brought down.
                        s.big_radius_m = r.min(RADIUS_MAX);
                    }
                }
                ["guide_auto", v] => s.guide_auto = v == "true",
                ["tracker", v] => s.tracker = v == "true",
                ["quest", v] => s.quest = Some(v.to_string()),
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
                ["pin_kind", v] => s.pin_kind = PinKind::parse(v).unwrap_or_default(),
                // `marker <world> x y z [kind [note words…]]` — older files stop at z.
                ["marker", w, ..] if f.len() >= 5 => {
                    if let Some(p) = point(&f[2..5]) {
                        let kind = f.get(5).and_then(|k| PinKind::parse(k)).unwrap_or_default();
                        let note = f.get(6..).map(|n| n.join(" ")).unwrap_or_default();
                        s.markers.entry(w.to_string()).or_default().push(Marker { at: p, kind, note });
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
        // Four keys, four different keys — or all back to their defaults. The old defaults,
        // all four untouched, move to the new ones.
        let keys = s.keys();
        if (0..keys.len()).any(|i| keys[i + 1..].contains(&keys[i])) || keys == OLD_DEFAULT_KEYS {
            [s.toggle_key, s.marker_key, s.compass_key, s.cycle_key] = DEFAULT_KEYS;
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
    /// Every key this map uses: map (steps through the displays), marker, compass,
    /// cycle.
    pub fn keys(&self) -> [u8; 4] {
        [self.toggle_key, self.marker_key, self.compass_key, self.cycle_key]
    }

    /// The display the map key moves to: the next in `Display::ALL` order that is in
    /// the cycle — or, if the current one is the only one, the same.
    pub fn next_display(&self) -> Display {
        let at = Display::ALL.iter().position(|d| *d == self.display).unwrap_or(0);
        (1..=Display::ALL.len())
            .map(|k| Display::ALL[(at + k) % Display::ALL.len()])
            .find(|d| self.cycle & d.bit() != 0)
            .unwrap_or(self.display)
    }
}

/// Every `actors::Kind` drawn.
pub const ALL_LAYERS: u8 = 0b11_1111;
/// The layer bits as of this version; files without it predate the save-point kind.
const LAYERS_VERSION: u8 = 2;

/// F1–F12 but those the game and Steam hold by default: F1 shows the game's HUD, F7 is
/// its photo mode, F12 is Steam's screenshot. (The panel is `, the console Shift+`.)
pub fn usable_key(k: u8) -> bool {
    (1..=12).contains(&k) && !TAKEN_KEYS.contains(&k)
}

/// The function keys the game (F1 HUD, F7 photo mode) and Steam (F12 screenshot) use.
pub const TAKEN_KEYS: [u8; 3] = [1, 7, 12];

/// The map's keys by default, in `keys()` order (map display, pin, compass, next goal):
/// F2, F5, F3, F4, next to the game's F1 and clear of its F7.
pub const DEFAULT_KEYS: [u8; 4] = [2, 5, 3, 4];
/// The defaults until 0.2.1 (F9, F6, F10, F11), moved to `DEFAULT_KEYS` when all four
/// are still as they were.
const OLD_DEFAULT_KEYS: [u8; 4] = [9, 6, 10, 11];

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: (f32, f32), b: (f32, f32)) -> bool {
        (a.0 - b.0).abs() < 1e-3 && (a.1 - b.1).abs() < 1e-3
    }

    #[test]
    fn north_up_puts_x_up_and_y_right() {
        let v = View {
            center: [0.0; 3],
            yaw_deg: 0.0,
            heading_up: false,
            scale: 1.0,
            north_deg: 0.0,
            outline: false,
            full: false,
        };
        assert!(close(v.project([100.0, 0.0, 0.0]), (0.0, -100.0)));
        assert!(close(v.project([0.0, 100.0, 0.0]), (100.0, 0.0)));
        assert!(close(v.north(), (0.0, -1.0)));
        // Facing +Y (yaw 90) points the arrow right.
        let v = View { yaw_deg: 90.0, ..v };
        assert!(close(v.heading(), (1.0, 0.0)));
    }

    #[test]
    fn the_map_key_steps_through_the_cycle() {
        let mut s = MapState::default();
        let mut seen = Vec::new();
        for _ in 0..4 {
            s.display = s.next_display();
            seen.push(s.display);
        }
        assert_eq!(seen, [Display::Big, Display::Off, Display::Mini, Display::Big]);
        s.cycle = Display::Mini.bit() | Display::Big.bit();
        s.display = Display::Big;
        assert_eq!(s.next_display(), Display::Mini, "off is left out");
        s.cycle = Display::Big.bit();
        s.display = Display::Big;
        assert_eq!(s.next_display(), Display::Big);
    }

    #[test]
    fn an_old_file_that_hid_the_map_starts_off() {
        assert_eq!(MapState::parse("show false\n").display, Display::Off);
        assert_eq!(MapState::parse("show true\n").display, Display::Mini);
        assert_eq!(MapState::parse("display big\n").display, Display::Big);
    }

    #[test]
    fn unproject_undoes_project() {
        for (heading_up, yaw) in [(false, 30.0), (true, 123.0)] {
            let v = View {
                center: [100.0, -50.0, 0.0],
                yaw_deg: yaw,
                heading_up,
                scale: 0.02,
                north_deg: 270.0,
                outline: false,
                full: false,
            };
            let (x, y) = v.project([900.0, 400.0, 0.0]);
            let w = v.unproject(x, y);
            assert!((w[0] - 900.0).abs() < 0.1 && (w[1] - 400.0).abs() < 0.1, "{w:?}");
        }
    }

    #[test]
    fn north_up_follows_the_games_north() {
        // The game's north along -Y (yaw 270). Yaw turns clockwise seen from above, so
        // facing -Y the right hand points along +X (yaw 0 = 270 + 90): the game's east.
        let v = View {
            center: [0.0; 3],
            yaw_deg: 270.0,
            heading_up: false,
            scale: 1.0,
            north_deg: 270.0,
            outline: false,
            full: false,
        };
        let close = |a: (f32, f32), b: (f32, f32)| (a.0 - b.0).abs() < 1e-3 && (a.1 - b.1).abs() < 1e-3;
        assert!(close(v.project([0.0, -100.0, 0.0]), (0.0, -100.0)), "north is up");
        assert!(close(v.project([100.0, 0.0, 0.0]), (100.0, 0.0)), "+X is the game's east");
        assert!(close(v.north(), (0.0, -1.0)));
        assert!(close(v.heading(), (0.0, -1.0)), "facing the game's north, the arrow points up");
    }

    #[test]
    fn heading_up_puts_what_is_ahead_up() {
        let v = View {
            center: [0.0; 3],
            yaw_deg: 90.0,
            heading_up: true,
            scale: 0.5,
            north_deg: 0.0,
            outline: false,
            full: false,
        };
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
    fn old_marker_lines_still_read() {
        let s = MapState::parse("marker W 1 2 3\n");
        assert_eq!(s.markers["W"], [Marker { at: [1.0, 2.0, 3.0], kind: PinKind::Mark, note: String::new() }]);
        let m = &MapState::parse("marker W 1 2 3 locked vault door B\n").markers["W"][0];
        assert_eq!((m.kind, m.note.as_str()), (PinKind::LockedDoor, "vault door B"));
        assert!(is_pin(m.id("W")) && !is_pin(m.id("W") | 1 << 63));
    }

    #[test]
    fn f6_beside_a_marker_removes_it() {
        let mut s = MapState::default();
        assert!(s.toggle_marker("W", [0.0, 0.0, 0.0]));
        assert!(s.toggle_marker("W", [1000.0, 0.0, 0.0]));
        assert!(!s.toggle_marker("W", [100.0, 100.0, 0.0]));
        assert_eq!(s.markers["W"].iter().map(|m| m.at).collect::<Vec<_>>(), [[1000.0, 0.0, 0.0]]);
    }

    #[test]
    fn round_trips_and_shrugs_off_bad_lines() {
        let mut s = MapState {
            heading_up: false,
            radius_m: 120.0,
            layers: 0b101,
            icon_px: 22,
            terrain: false,
            relief: ReliefMode::Contour,
            compass: false,
            compass_key: 10,
            cycle_key: 11,
            guide_auto: false,
            goal_tiers: 0b101,
            display: Display::Big,
            cycle: 0b011,
            big_radius_m: 300.0,
            big_alpha: 60,
            big_outline: false,
            mini_outline: true,
            dots: false,
            haze_links: Vec::new(),
            opacity: [40, 80, 100],
            north_yaw: 90.0,
            hide_in_menus: false,
            route: false,
            quest: Some("Quest03".into()),
            tracker: false,
            hidden: [Sub::Lore, Sub::Door].into_iter().collect(),
            ..MapState::default()
        };
        s.toggle_marker("Map_A", [1.5, -2.0, 3.0]);
        s.markers.get_mut("Map_A").unwrap()[0] =
            Marker { at: [1.5, -2.0, 3.0], kind: PinKind::Puzzle, note: "pillar order".into() };
        s.observe("Map_A", [0.0, 0.0, 0.0]);
        s.observe("Map_A", [90_000.0, 0.0, 0.0]);
        s.dirty = false;
        assert_eq!(MapState::parse(&s.render()), s);
        let old = MapState::parse("layers 31\n");
        assert_eq!(old.layers, ALL_LAYERS, "a file from before save points turns them on");
        let off = MapState::parse("layers 31\nlayers_version 2\n");
        assert_eq!(off.layers, 31, "a file that knows of them keeps them off");
        let k = MapState::parse("toggle_key 11\nmarker_key 5\ncycle_key 4\n");
        assert_eq!((k.toggle_key, k.marker_key), (11, 5));
        // The game's and Steam's keys (F1 HUD, F7 photo mode, F12 screenshot) are not taken.
        let k = MapState::parse("toggle_key 12\nmarker_key 7\ncompass_key 1\n");
        assert_eq!((k.toggle_key, k.marker_key, k.compass_key), (2, 5, 3));
        let k = MapState::parse("toggle_key 8\nmarker_key 13\n");
        assert_eq!((k.toggle_key, k.marker_key), (8, 5), "F8 is free; F13 is no key");
        let k = MapState::parse("toggle_key 5\n");
        assert_eq!((k.toggle_key, k.marker_key), (2, 5), "one key cannot do both");
        let k = MapState::parse("compass_key 5\n");
        assert_eq!((k.marker_key, k.compass_key), (5, 3), "nor can the compass take the marker's");
        let k = MapState::parse("toggle_key 9\nmarker_key 6\ncompass_key 10\ncycle_key 11\n");
        assert_eq!(k.keys(), DEFAULT_KEYS, "the old defaults, untouched, move to the new");
        let t = MapState::parse("radius 99999\nmarker W 1 2\ntrail W a b c\nshow false\n???\n");
        assert_eq!(t.radius_m, 60.0);
        assert!(t.markers.is_empty() && t.trails.is_empty());
        assert_eq!(t.display, Display::Off);
    }
}
