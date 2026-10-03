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

/// What a map pin marks — the player's own note to come back to. Each kind has its
/// icon in `assets/pins/<word>.svg`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PinKind {
    LockedDoor,
    LockedChest,
    LymbicLock,
    Puzzle,
    Code,
    KeyNeeded,
    ItemLater,
    Merchant,
    Npc,
    Quest,
    Danger,
    Boss,
    Timeloop,
    Save,
    Shortcut,
    Ladder,
    DeadEnd,
    Water,
    View,
    Treasure,
    Note,
    Home,
    Question,
    #[default]
    Mark,
}

impl PinKind {
    pub const ALL: [PinKind; 24] = [
        PinKind::LockedDoor,
        PinKind::LockedChest,
        PinKind::LymbicLock,
        PinKind::Puzzle,
        PinKind::Code,
        PinKind::KeyNeeded,
        PinKind::ItemLater,
        PinKind::Merchant,
        PinKind::Npc,
        PinKind::Quest,
        PinKind::Danger,
        PinKind::Boss,
        PinKind::Timeloop,
        PinKind::Save,
        PinKind::Shortcut,
        PinKind::Ladder,
        PinKind::DeadEnd,
        PinKind::Water,
        PinKind::View,
        PinKind::Treasure,
        PinKind::Note,
        PinKind::Home,
        PinKind::Question,
        PinKind::Mark,
    ];

    /// Its word in `minimap.txt`, and its icon's file name.
    pub fn word(self) -> &'static str {
        match self {
            PinKind::LockedDoor => "locked_door",
            PinKind::LockedChest => "locked_chest",
            PinKind::LymbicLock => "lymbic_lock",
            PinKind::Puzzle => "puzzle",
            PinKind::Code => "code",
            PinKind::KeyNeeded => "key_needed",
            PinKind::ItemLater => "item_later",
            PinKind::Merchant => "merchant",
            PinKind::Npc => "npc",
            PinKind::Quest => "quest",
            PinKind::Danger => "danger",
            PinKind::Boss => "boss",
            PinKind::Timeloop => "timeloop",
            PinKind::Save => "save",
            PinKind::Shortcut => "shortcut",
            PinKind::Ladder => "ladder",
            PinKind::DeadEnd => "dead_end",
            PinKind::Water => "water",
            PinKind::View => "view",
            PinKind::Treasure => "treasure",
            PinKind::Note => "note",
            PinKind::Home => "home",
            PinKind::Question => "question",
            PinKind::Mark => "mark",
        }
    }

    /// A word from the file; the first pins' words (`locked`, `later`) still read.
    pub fn parse(w: &str) -> Option<PinKind> {
        match w {
            "locked" => Some(PinKind::LockedDoor),
            "later" => Some(PinKind::ItemLater),
            _ => PinKind::ALL.into_iter().find(|k| k.word() == w),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            PinKind::LockedDoor => tr!("잠긴 문"),
            PinKind::LockedChest => tr!("잠긴 상자"),
            PinKind::LymbicLock => tr!("림빅 잠금"),
            PinKind::Puzzle => tr!("퍼즐"),
            PinKind::Code => tr!("코드·암호"),
            PinKind::KeyNeeded => tr!("열쇠 필요"),
            PinKind::ItemLater => tr!("나중에 주울 것"),
            PinKind::Merchant => tr!("상인"),
            PinKind::Npc => tr!("만날 사람"),
            PinKind::Quest => tr!("퀘스트 단서"),
            PinKind::Danger => tr!("위험"),
            PinKind::Boss => tr!("강적"),
            PinKind::Timeloop => tr!("타임루프"),
            PinKind::Save => tr!("저장 지점"),
            PinKind::Shortcut => tr!("지름길"),
            PinKind::Ladder => tr!("오를 곳"),
            PinKind::DeadEnd => tr!("막다른 길"),
            PinKind::Water => tr!("물·건널 곳"),
            PinKind::View => tr!("둘러볼 곳"),
            PinKind::Treasure => tr!("보물"),
            PinKind::Note => tr!("메모"),
            PinKind::Home => tr!("거점"),
            PinKind::Question => tr!("모르는 것"),
            PinKind::Mark => tr!("표시"),
        }
    }

    /// Its icon's colour, for the compass and lists.
    pub fn rgb(self) -> [u8; 3] {
        match self {
            PinKind::LockedDoor => [217, 73, 61],
            PinKind::LockedChest => [192, 96, 58],
            PinKind::LymbicLock => [138, 79, 216],
            PinKind::Puzzle => [166, 91, 232],
            PinKind::Code => [106, 111, 224],
            PinKind::KeyNeeded => [217, 139, 43],
            PinKind::ItemLater => [232, 194, 58],
            PinKind::Merchant => [63, 174, 106],
            PinKind::Npc => [62, 143, 224],
            PinKind::Quest => [232, 79, 180],
            PinKind::Danger => [224, 112, 42],
            PinKind::Boss => [184, 50, 58],
            PinKind::Timeloop => [125, 95, 224],
            PinKind::Save => [232, 162, 58],
            PinKind::Shortcut => [47, 184, 176],
            PinKind::Ladder => [91, 122, 166],
            PinKind::DeadEnd => [125, 135, 150],
            PinKind::Water => [47, 134, 200],
            PinKind::View => [63, 168, 160],
            PinKind::Treasure => [212, 167, 44],
            PinKind::Note => [168, 135, 92],
            PinKind::Home => [79, 154, 98],
            PinKind::Question => [122, 138, 160],
            PinKind::Mark => [110, 205, 255],
        }
    }
}

/// A pin on the map: where, what kind, and the player's note.
#[derive(Clone, Debug, PartialEq)]
pub struct Marker {
    pub at: Point,
    pub kind: PinKind,
    pub note: String,
}

/// Map pins guided to as goals carry ids with bit 62 set (and 63 clear: the survey's).
pub const PIN_BIT: u64 = 1 << 62;

pub fn is_pin(id: u64) -> bool {
    id & (3 << 62) == PIN_BIT
}

impl Marker {
    /// A stable id: the world and the spot, hashed — kept while the pin stays put.
    pub fn id(&self, world: &str) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for b in world.bytes().chain(format!("{:.0},{:.0},{:.0}", self.at[0], self.at[1], self.at[2]).bytes()) {
            h = (h ^ b as u64).wrapping_mul(0x0100_0000_01b3);
        }
        (h & !(3 << 62)) | PIN_BIT
    }

    /// What the guide calls it.
    pub fn title(&self) -> String {
        if self.note.trim().is_empty() {
            trf!("핀: {a0}", a0 = self.kind.label())
        } else {
            trf!("핀: {a0}", a0 = self.note.trim())
        }
    }
}

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
            "display {}\ncycle_modes {}\nheading_up {}\nradius {}\ntoggle_key {}\nmarker_key {}\nlayers {}\nlayers_version {LAYERS_VERSION}\nicon_px {}\nterrain {}\nrelief {}\ncompass {}\ncompass_key {}\ncycle_key {}\nguide_auto {}\ngoal_tiers {}\nbig_radius {}\nbig_alpha {}\nbig_outline {}\nmini_outline {}\nhide_in_menus {}\nroute {}\nnorth_yaw {}\ntracker {}\n",
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
                ["big_outline", v] => s.big_outline = v == "true",
                ["mini_outline", v] => s.mini_outline = v == "true",
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
        // Four keys, four different keys — or all back to their defaults.
        let keys = s.keys();
        if (0..keys.len()).any(|i| keys[i + 1..].contains(&keys[i])) {
            (s.toggle_key, s.marker_key, s.compass_key, s.cycle_key) = (9, 6, 10, 11);
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

/// F1–F12 (the panel's key is ` now, so F8 is free too).
pub fn usable_key(k: u8) -> bool {
    (1..=12).contains(&k)
}

/// Which map is up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Display {
    Mini,
    Big,
    Off,
}

impl Display {
    /// In the order the map key steps through them.
    pub const ALL: [Display; 3] = [Display::Mini, Display::Big, Display::Off];

    /// As kept in the settings file.
    pub fn key(self) -> &'static str {
        match self {
            Display::Mini => "mini",
            Display::Big => "big",
            Display::Off => "off",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Display::Mini => tr!("미니맵"),
            Display::Big => tr!("큰 지도"),
            Display::Off => tr!("끔"),
        }
    }

    pub fn bit(self) -> u8 {
        1 << Display::ALL.iter().position(|d| *d == self).unwrap()
    }
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
            ReliefMode::Off => tr!("끔"),
            ReliefMode::Shade => tr!("음영"),
            ReliefMode::Contour => tr!("등고선"),
            ReliefMode::Both => tr!("둘 다"),
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
    /// Drawn as outlines on a clear background (as Diablo's overlay map): no disc, no
    /// fills — walls, contours and shores as lines, so the game shows through.
    pub outline: bool,
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
        let v = View { center: [0.0; 3], yaw_deg: 0.0, heading_up: false, scale: 1.0, north_deg: 0.0, outline: false };
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
        let v =
            View { center: [0.0; 3], yaw_deg: 270.0, heading_up: false, scale: 1.0, north_deg: 270.0, outline: false };
        let close = |a: (f32, f32), b: (f32, f32)| (a.0 - b.0).abs() < 1e-3 && (a.1 - b.1).abs() < 1e-3;
        assert!(close(v.project([0.0, -100.0, 0.0]), (0.0, -100.0)), "north is up");
        assert!(close(v.project([100.0, 0.0, 0.0]), (100.0, 0.0)), "+X is the game's east");
        assert!(close(v.north(), (0.0, -1.0)));
        assert!(close(v.heading(), (0.0, -1.0)), "facing the game's north, the arrow points up");
    }

    #[test]
    fn heading_up_puts_what_is_ahead_up() {
        let v = View { center: [0.0; 3], yaw_deg: 90.0, heading_up: true, scale: 0.5, north_deg: 0.0, outline: false };
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
            compass_key: 2,
            cycle_key: 3,
            guide_auto: false,
            goal_tiers: 0b101,
            display: Display::Big,
            cycle: 0b011,
            big_radius_m: 400.0,
            big_alpha: 60,
            big_outline: false,
            mini_outline: true,
            north_yaw: 90.0,
            hide_in_menus: false,
            route: false,
            quest: Some("Quest03".into()),
            tracker: false,
            hidden: [Sub::Lore, Sub::Door].into_iter().collect(),
            ..MapState::default()
        };
        s.toggle_marker("Map_A", [1.5, -2.0, 3.0]);
        s.markers.get_mut("Map_A").unwrap()[0] = Marker { at: [1.5, -2.0, 3.0], kind: PinKind::Puzzle, note: "pillar order".into() };
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
        assert_eq!((k.toggle_key, k.marker_key), (8, 6), "F8 is free now; F13 is no key");
        let k = MapState::parse("toggle_key 6\n");
        assert_eq!((k.toggle_key, k.marker_key), (9, 6), "one key cannot do both");
        let k = MapState::parse("compass_key 6\n");
        assert_eq!((k.marker_key, k.compass_key), (6, 10), "nor can the compass take the marker's");
        let t = MapState::parse("radius 99999\nmarker W 1 2\ntrail W a b c\nshow false\n???\n");
        assert_eq!(t.radius_m, 60.0);
        assert!(t.markers.is_empty() && t.trails.is_empty());
        assert_eq!(t.display, Display::Off);
    }
}
