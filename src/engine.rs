//! The loop every front end drives: attach to the game, watch the hero gate, hold
//! the active toggles, read the values. The CLI's `hold` and the overlay are both a
//! thin layer over `Engine::step`.

use crate::actors::{Scanner, Thing};
use crate::anchors::{self, Anchors};
use crate::attr::{Attr, Session};
use crate::cheats::{self, Active, Kind};
use crate::extras::Extras;
use crate::game::locate;
use crate::game::process::Game;
use crate::geometry::{Footprint, Geometry};
use crate::goals::{Goal, Goals};
use crate::gobjects::{self, Objects};
use crate::hold::{self, Originals};
use crate::knowledge::{self, Knowledge};
use crate::mem::Memory;
use crate::obstacles::{Obstacles, Scene};
use crate::player::{self, Chain};
use std::cell::RefCell;
use std::sync::Arc;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

pub struct Attached {
    pub game: Game,
    pub anchors: Anchors,
    /// Steam's build id.
    pub version: String,
    /// Learned the first time the hero is in play, then kept for the process.
    chain: RefCell<Option<Chain>>,
    /// Enemies, items and the like for the minimap.
    scanner: RefCell<Scanner>,
    /// The minimap's background: static meshes seen from above.
    geometry: RefCell<Geometry>,
    /// The hero's inventory, once found (checked on every use).
    inventory: RefCell<Option<u64>>,
    /// The guide: what the hero knows (from the save state) and where there is more.
    guide: RefCell<Guide>,
}

/// GUObjectArray, the save slots found through it, the knowledge last read, and the
/// interactables' payloads — each refreshed on its own clock.
#[derive(Default)]
struct Guide {
    objects: Option<Objects>,
    saves: Vec<u64>,
    saves_read: Option<Instant>,
    knowledge: Option<Knowledge>,
    knowledge_read: Option<Instant>,
    goals: Goals,
    /// The quest journal, rebuilt a slice at a time.
    quests: crate::quests::Quests,
    /// The game's navmesh, read a little at a time.
    nav: crate::navmesh::Nav,
    /// The survey of every world (Mods\survey), read once; and what the hero knows and
    /// holds by name, to judge it with — renewed with the knowledge.
    survey: Option<crate::survey::Survey>,
    known_facts: HashSet<String>,
    known_tags: HashSet<String>,
    held: HashSet<String>,
    saved: HashSet<String>,
    name_cache: HashMap<u32, String>,
    fact_keys: HashMap<String, String>,
    /// The save state the knowledge was read from.
    save: u64,
    /// What stands in the way: collision shapes, collected a slice per step.
    obstacles: Obstacles,
}

const SAVES_EVERY: Duration = Duration::from_secs(60);
const KNOWLEDGE_EVERY: Duration = Duration::from_secs(2);

fn mem_ptr(m: &dyn crate::mem::Memory, at: u64) -> Result<u64, String> {
    crate::mem::read_u64(m, at).filter(|&p| crate::mem::plausible(p)).ok_or_else(|| "pointer unreadable".into())
}

/// The GUIDs of the placed things the save keeps a state for, in every region
/// (`World.RegionStates[].ElementStates[].Identifier`), as `XXXXXXXX-XXXXXXXX-…` like the
/// survey's.
fn saved_guids(m: &dyn crate::mem::Memory, n: &crate::names::Names, save: u64) -> HashSet<String> {
    let mut out = HashSet::new();
    let Some((at, p)) = n.path(m, save, &["World", "RegionStates"]) else { return out };
    let size = |field: u64| n.inner_of(m, field).and_then(|i| crate::mem::read_u32(m, i + n.layout.size)).unwrap_or(0) as u64;
    let region_size = size(p.field);
    let Some(region_struct) = n.inner_of(m, p.field).and_then(|i| n.struct_of(m, i)) else { return out };
    let Some(states) = n.find(m, region_struct, "ElementStates") else { return out };
    let element_size = size(states.field);
    if region_size == 0 || element_size < 16 {
        return out;
    }
    for region in crate::actors::array_of(m, at, region_size, 64) {
        for e in crate::actors::array_of(m, region + states.offset as u64, element_size, 100_000) {
            let mut g = [0u8; 16];
            if m.read(e, &mut g) {
                let part = |i: usize| u32::from_le_bytes(g[i * 4..i * 4 + 4].try_into().unwrap());
                out.insert(format!("{:08X}-{:08X}-{:08X}-{:08X}", part(0), part(1), part(2), part(3)));
            }
        }
    }
    out
}

pub fn attach() -> Result<Attached, String> {
    let game = Game::find()?.ok_or("the game is not running")?;
    let version = locate::from_exe(&game.exe).map(|i| i.version).unwrap_or_else(|_| "unknown".into());
    let anchors = anchors::discover(&game, game.base)?;
    Ok(Attached {
        game,
        anchors,
        version,
        chain: RefCell::new(None),
        scanner: RefCell::default(),
        guide: RefCell::default(),
        geometry: RefCell::default(),
        inventory: RefCell::default(),
    })
}

impl Attached {
    /// The way to the player, learning it if it is not known yet.
    pub fn chain(&self) -> Result<Chain, String> {
        if let Some(c) = self.chain.borrow().as_ref() {
            return Ok(c.clone());
        }
        let c = player::learn(&self.game, &self.anchors)?;
        *self.chain.borrow_mut() = Some(c.clone());
        Ok(c)
    }

    /// The hero's attributes, plus the plain fields the cheat table writes
    /// (`cheats::FIELDS`) — each added only when its owner is the class it must be.
    pub fn session(&self) -> Result<Session<'_>, String> {
        let chain = self.chain()?;
        let arr = chain.attribute_sets(&self.game, &self.anchors)?;
        let n = &self.anchors.names;
        let mut s = Session::open(&self.game, self.game.module(), n, arr)?;
        let pawn = chain.hero(&self.game, &self.anchors)?;
        s.add_fields(n, cheats::HERO, pawn, cheats::HERO_FIELDS);
        if let Ok(mc) = n.follow(&self.game, pawn, "CharacterMovement") {
            if n.is_a(&self.game, mc, "CharacterMovementComponent") {
                s.add_fields(n, cheats::MOVEMENT, mc, cheats::MOVEMENT_FIELDS);
            }
        }
        if let Ok(ws) = self.world_settings() {
            if n.is_a(&self.game, ws, "WorldSettings") {
                s.add_fields(n, cheats::WORLD, ws, cheats::WORLD_FIELDS);
            }
        }
        Ok(s)
    }

    /// The persistent level's WorldSettings: hero → level → world → PersistentLevel.
    pub fn world_settings(&self) -> Result<u64, String> {
        let (m, n) = (&self.game, &self.anchors.names);
        let hero = self.chain()?.hero(m, &self.anchors)?;
        let level = mem_ptr(m, hero + crate::names::OUTER)?;
        let world = mem_ptr(m, level + crate::names::OUTER)?;
        let persistent = n.follow(m, world, "PersistentLevel")?;
        n.follow(m, persistent, "WorldSettings")
    }

    /// The hero's CharlieInventory — under CharlieInventoryLoadoutSubsystem, owned by
    /// the hero. Found once through GUObjectArray, then checked on every use: still a
    /// CharlieInventory, still the hero's.
    pub fn inventory(&self) -> Result<u64, String> {
        let (m, n) = (&self.game, &self.anchors.names);
        let hero = self.chain()?.hero(m, &self.anchors)?;
        let ok = |inv: u64| {
            n.is_a(m, inv, "CharlieInventory")
                && n.field(m, inv, "Owner").and_then(|p| crate::mem::read_u64(m, inv + p.offset as u64)) == Some(hero)
        };
        if let Some(inv) = *self.inventory.borrow() {
            if ok(inv) {
                return Ok(inv);
            }
        }
        let objects = gobjects::discover(m, self.game.base)?;
        let inv = objects
            .of_class(m, n, "CharlieInventory")
            .into_iter()
            .find(|&o| ok(o))
            .ok_or("the hero's inventory was not found")?;
        *self.inventory.borrow_mut() = Some(inv);
        Ok(inv)
    }

    /// Open while the player controls the hero. Closed is not an error to act on —
    /// it is a loading screen, a menu or a cinematic, and nothing is written then.
    pub fn gate(&self) -> Result<(), String> {
        self.chain()?.hero(&self.game, &self.anchors).map(drop).map_err(|e| format!("hero gate closed: {e}"))
    }

    /// Where the hero is and which way the camera looks.
    pub fn pose(&self) -> Result<([f64; 3], f64), String> {
        self.chain()?.pose(&self.game, &self.anchors)
    }

    /// Enemies, items, loot, people and doors in the loaded levels, and where they are.
    pub fn things(&self) -> Result<Vec<Thing>, String> {
        let chain = self.chain()?;
        let hero = chain.hero(&self.game, &self.anchors)?;
        let mut s = self.scanner.borrow_mut();
        s.refresh(&self.game, &self.anchors.names, hero, chain.root, chain.sets)?;
        if let Some(actors) = s.actors_offset() {
            self.geometry.borrow_mut().refresh(
                &self.game,
                &self.anchors.names,
                hero,
                chain.root,
                chain.location,
                actors,
            );
        }
        Ok(s.positions(&self.game, chain.location))
    }

    /// Put the hero at `p` (cm), standing still: the root component's RelativeLocation
    /// and its ComponentToWorld (what movement reads), and the movement's Velocity.
    pub fn teleport(&self, p: [f64; 3]) -> Result<(), String> {
        let (m, n) = (&self.game, &self.anchors.names);
        let chain = self.chain()?;
        let hero = chain.hero(m, &self.anchors)?;
        let root = mem_ptr(m, hero + chain.root)?;
        let bytes: Vec<u8> = p.iter().flat_map(|v| v.to_le_bytes()).collect();
        let c2w = root + crate::obstacles::COMPONENT_TO_WORLD + 0x20;
        if !m.write(root + chain.location, &bytes) || !m.write(c2w, &bytes) {
            return Err("could not write the hero's position".into());
        }
        if let Ok(mc) = n.follow(m, hero, "CharacterMovement") {
            if let Some(v) = n.field(m, mc, "Velocity") {
                m.write(mc + v.offset as u64, &[0u8; 24]);
            }
        }
        Ok(())
    }

    /// Enemies still alive in the loaded levels, as the minimap last scanned them.
    pub fn enemies(&self) -> Vec<u64> {
        self.scanner.borrow().actors_of(&self.game, crate::actors::Kind::Enemy)
    }

    /// Whether the game is paused: the world's `WorldSettings.Pauser` is set. Menus
    /// that stop the game set it.
    pub fn paused(&self) -> Result<bool, String> {
        let (m, n) = (&self.game, &self.anchors.names);
        let hero = self.chain()?.hero(m, &self.anchors)?;
        let level = mem_ptr(m, hero + crate::names::OUTER)?;
        let world = mem_ptr(m, level + crate::names::OUTER)?;
        let persistent = n.follow(m, world, "PersistentLevel")?;
        let settings = n.follow(m, persistent, "WorldSettings")?;
        let p = n.field(m, settings, "Pauser").ok_or("no WorldSettings.Pauser")?;
        Ok(crate::mem::read_u64(m, settings + p.offset as u64).is_some_and(|v| v != 0))
    }

    /// Places where the hero can still learn something, and what the hero knows —
    /// for the compass and the guide.
    pub fn goals(&self) -> Result<(Vec<Goal>, Knowledge), String> {
        let chain = self.chain()?;
        let hero = chain.hero(&self.game, &self.anchors)?;
        let (m, n) = (&self.game, &self.anchors.names);
        let mut g = self.guide.borrow_mut();
        if g.objects.is_none() {
            g.objects = Some(gobjects::discover(m, self.game.base)?);
        }
        if g.saves.is_empty() || g.saves_read.is_none_or(|t| t.elapsed() >= SAVES_EVERY) {
            g.saves = g.objects.as_ref().unwrap().of_class(m, n, "CharlieSaveGame");
            g.saves_read = Some(Instant::now());
        }
        if g.knowledge.is_none() || g.knowledge_read.is_none_or(|t| t.elapsed() >= KNOWLEDGE_EVERY) {
            let save = knowledge::current(n, m, &g.saves).ok_or("no save state found")?;
            g.save = save;
            g.knowledge = Some(knowledge::read(n, m, save).ok_or("the save state could not be read")?);
            g.knowledge_read = Some(Instant::now());
            // The same by name, for the survey.
            let g = &mut *g;
            let k = g.knowledge.as_ref().unwrap();
            let mut name = |i: u32| g.name_cache.entry(i).or_insert_with(|| n.get(m, i).unwrap_or_default()).clone();
            g.known_facts = k.facts.iter().map(|&i| name(i)).collect();
            g.known_tags = k.tags.iter().map(|&i| name(i)).collect();
            g.held = self.held_items();
            g.saved = saved_guids(m, n, save);
            g.fact_keys = g.quests.fact_keys();
        }
        let actors = self.scanner.borrow().actors_offset().ok_or("actors not scanned yet")?;
        {
            let g = &mut *g;
            g.goals.refresh(m, n, hero, chain.root, actors, g.quests.flows());
        }
        if let Ok((p, _)) = chain.pose(m, &self.anchors) {
            let objects = g.objects.take().unwrap();
            g.obstacles.step(m, n, &objects, p);
            g.objects = Some(objects);
        }
        {
            let g = &mut *g;
            let objects = g.objects.as_ref().unwrap();
            g.quests.step(m, n, || objects.all(m));
            g.nav.step(m, n, g.quests.nav_actors());
        }
        let k = g.knowledge.clone().unwrap();
        let mut goals = g.goals.evaluate(m, &k, chain.location);
        // What the survey knows of this world beyond what is loaded.
        let g = &mut *g;
        let survey = g.survey.get_or_insert_with(|| crate::survey::Survey::load(&crate::paths::data_dir().join("survey")));
        if let Ok(world) = chain.world(m, &self.anchors) {
            let known = crate::survey::Known { facts: &g.known_facts, tags: &g.known_tags, held: &g.held, saved: &g.saved, talked: &g.goals.done_npcs };
            goals.extend(survey.goals(crate::survey::Survey::world_of(&world), &known, &g.goals.loaded, &g.fact_keys));
        }
        Ok((goals, k))
    }

    /// The quest journal against what the hero knows now — empty until its first pass.
    pub fn journal(&self) -> Vec<crate::quests::Quest> {
        let g = self.guide.borrow();
        match (&g.knowledge, g.quests.ready()) {
            (Some(k), true) => g.quests.journal(k, &crate::quests::deed_states(&self.game, &self.anchors.names, g.save)),
            _ => Vec::new(),
        }
    }

    /// The item assets the hero holds, by name.
    fn held_items(&self) -> HashSet<String> {
        let (m, n) = (&self.game, &self.anchors.names);
        let Ok(inv) = self.inventory() else { return HashSet::new() };
        let Some(items) = n.field(m, inv, "Items") else { return HashSet::new() };
        crate::actors::array(m, inv + items.offset as u64, 4096)
            .into_iter()
            .filter_map(|item| {
                let data = n.field(m, item, "ItemData")?;
                let def = crate::mem::read_u64(m, item + data.offset as u64).filter(|&p| crate::mem::plausible(p))?;
                n.object(m, def)
            })
            .collect()
    }

    /// The collectibles' counts here and everywhere, and the NPCs with more to tell.
    pub fn collection(&self, world: &str) -> (Vec<crate::survey::Collect>, Vec<crate::survey::Need>) {
        let g = self.guide.borrow();
        let Some(survey) = g.survey.as_ref() else { return Default::default() };
        let known =
            crate::survey::Known { facts: &g.known_facts, tags: &g.known_tags, held: &g.held, saved: &g.saved, talked: &g.goals.done_npcs };
        (survey.collection(crate::survey::Survey::world_of(world), &known), survey.stories(&known))
    }

    /// Every secret of a kind: how many the game has.
    pub fn secret_total(&self, kind: crate::quests::Kind) -> usize {
        self.guide.borrow().quests.secrets(kind).len()
    }

    /// Every good deed the game has: (journal key, title, tag stem).
    pub fn deeds(&self) -> Vec<(String, String, String)> {
        self.guide.borrow().quests.secrets(crate::quests::Kind::GoodDeed)
    }

    /// NPCs that want an item the hero holds (the survey's trades).
    pub fn handovers(&self) -> Vec<crate::survey::Need> {
        let g = self.guide.borrow();
        let Some(survey) = g.survey.as_ref() else { return Vec::new() };
        let known =
            crate::survey::Known { facts: &g.known_facts, tags: &g.known_tags, held: &g.held, saved: &g.saved, talked: &g.goals.done_npcs };
        survey.handovers(&known)
    }

    /// For each quest under way, the places it needs in every world (from the survey).
    pub fn needs(&self, journal: &[crate::quests::Quest]) -> Vec<(String, Vec<crate::survey::Need>)> {
        let g = self.guide.borrow();
        let Some(survey) = g.survey.as_ref().filter(|s| !s.is_empty()) else { return Vec::new() };
        let known = crate::survey::Known { facts: &g.known_facts, tags: &g.known_tags, held: &g.held, saved: &g.saved, talked: &g.goals.done_npcs };
        journal
            .iter()
            .filter(|q| q.active())
            .map(|q| (q.key.clone(), survey.needs(&q.key, q.tags.as_deref(), &known, &g.fact_keys)))
            .collect()
    }

    /// The navmesh as last read (shared, not copied).
    pub fn nav(&self) -> Arc<crate::navmesh::NavMesh> {
        self.guide.borrow().nav.done.clone()
    }

    /// The obstacles and ground of the last complete pass (shared, not copied).
    pub fn obstacles(&self) -> Arc<Scene> {
        self.guide.borrow().obstacles.done.clone()
    }

    /// The footprints found so far (shared, not copied).
    pub fn footprints(&self) -> Arc<Vec<Footprint>> {
        self.geometry.borrow().footprints.clone()
    }
}

/// What one step saw. Plain data, so a UI thread can hold a copy.
#[derive(Clone, Debug)]
pub struct Snapshot {
    /// (pid, Steam build id) when attached.
    pub game: Result<(u32, String), String>,
    pub gate: Result<(), String>,
    /// The current value of every attribute the cheat table names, `None` where
    /// unreadable this step.
    pub values: Vec<(Attr, Option<f32>)>,
    pub active: Vec<Active>,
    /// Originals on record, waiting to be put back.
    pub pending: usize,
    /// The originals themselves — what the debug tab compares against.
    pub originals: Vec<(Attr, (f32, f32))>,
    /// Why the toggles stopped, or what the last tick could not do.
    pub notice: Option<String>,
    /// Where the hero is (cm) and the camera's yaw (degrees) — the minimap's input.
    pub pose: Option<([f64; 3], f64)>,
    /// The world the hero is in, by name.
    pub world: Option<String>,
    /// What the minimap marks besides the hero.
    pub things: Vec<Thing>,
    /// The minimap's background.
    pub footprints: Arc<Vec<Footprint>>,
    /// Places with something new to learn.
    pub goals: Vec<Goal>,
    /// The quest journal: main quests and good deeds, with their state.
    pub journal: Vec<crate::quests::Quest>,
    /// The game is paused (a menu that stops it is open).
    pub paused: bool,
    /// What stands in the way, and the ground, for the route.
    pub obstacles: Arc<Scene>,
    /// The game's navmesh: the route's first choice.
    pub nav: Arc<crate::navmesh::NavMesh>,
    /// For each quest under way, what it needs in every world (the survey).
    pub needs: Vec<(String, Vec<crate::survey::Need>)>,
    /// NPCs that want an item the hero holds.
    pub handovers: Vec<crate::survey::Need>,
    /// Missable good deeds not done yet, and their deadlines.
    pub deadlines: Vec<crate::missables::Deadline>,
    /// Collectibles placed and taken, per sort (the survey); NPCs with more to tell.
    pub collection: Vec<crate::survey::Collect>,
    pub stories: Vec<crate::survey::Need>,
    /// How many good deeds, mysteries and timeloops the game has.
    pub secret_totals: [usize; 3],
    /// Saved positions: (world, where).
    pub slots: [Option<(String, [f64; 3])>; SLOTS],
}

/// How many positions can be saved.
pub const SLOTS: usize = 5;
/// A teleport lands this far above the saved spot (cm), so it does not start in the
/// ground.
const LIFT: f64 = 50.0;

impl Snapshot {
    pub fn value(&self, a: Attr) -> Option<f32> {
        self.values.iter().find(|(x, _)| *x == a).and_then(|(_, v)| *v)
    }
}

pub struct Engine {
    attached: Option<Attached>,
    checked: Option<Instant>,
    originals: Originals,
    /// What the cheats past the hero overwrote (extras.rs).
    extras: Extras,
    active: Vec<Active>,
    notice: Option<String>,
    slots: [Option<(String, [f64; 3])>; SLOTS],
}

/// How often to look for the game, or check it is still the same process. Listing
/// processes is the one expensive thing a step does.
const RECHECK: Duration = Duration::from_secs(2);

impl Engine {
    pub fn new() -> Result<Engine, String> {
        Ok(Engine {
            attached: None,
            checked: None,
            originals: Originals::load(&hold::default_path())?,
            extras: Extras::default(),
            slots: Default::default(),
            active: Vec::new(),
            notice: None,
        })
    }

    pub fn pending(&self) -> usize {
        self.originals.len()
    }

    /// Attach, or notice the game went away. When it has, everything the record
    /// would have put back died with it.
    fn refresh(&mut self) -> Result<(), String> {
        let due = self.checked.is_none_or(|t| t.elapsed() >= RECHECK);
        if !due {
            return if self.attached.is_some() { Ok(()) } else { Err("the game is not running".into()) };
        }
        self.checked = Some(Instant::now());
        let live = Game::find()?.map(|g| g.pid);
        if let Some(a) = &self.attached {
            if live == Some(a.game.pid) {
                return Ok(());
            }
            self.attached = None;
            if !self.active.is_empty() {
                self.active.clear();
                self.notice = Some("the game exited — toggles off".into());
            }
            self.extras.forget();
            self.originals.forget()?;
        }
        self.attached = Some(attach()?);
        Ok(())
    }

    fn attached(&mut self) -> Result<&Attached, String> {
        self.refresh()?;
        Ok(self.attached.as_ref().expect("refresh attached"))
    }

    pub fn step(&mut self) -> Snapshot {
        let mut snap = Snapshot {
            game: Err(String::new()),
            gate: Err(String::new()),
            values: Vec::new(),
            active: Vec::new(),
            pending: 0,
            originals: Vec::new(),
            notice: None,
            pose: None,
            world: None,
            things: Vec::new(),
            footprints: Arc::default(),
            goals: Vec::new(),
            collection: Vec::new(),
            stories: Vec::new(),
            secret_totals: [0; 3],
            deadlines: Vec::new(),
            handovers: Vec::new(),
            needs: Vec::new(),
            nav: Default::default(),
            journal: Vec::new(),
            paused: false,
            obstacles: Arc::default(),
            slots: self.slots.clone(),
        };
        if let Err(e) = self.refresh() {
            snap.game = Err(e.clone());
            snap.gate = Err(e);
        } else {
            let a = self.attached.as_ref().unwrap();
            snap.game = Ok((a.game.pid, a.version.clone()));
            snap.gate = a.gate();
            snap.pose = a.pose().ok();
            snap.paused = a.paused().unwrap_or(false);
            snap.world = a.chain().ok().and_then(|c| c.world(&a.game, &a.anchors).ok());
            // A closed gate pauses the toggles rather than ending them: it closes on
            // every loading screen, and the player expects god mode to survive one.
            if snap.gate.is_ok() {
                match a.things() {
                    Ok(t) => {
                        snap.things = t;
                        snap.footprints = a.footprints();
                    }
                    Err(e) => snap.notice = Some(format!("minimap: {e}")),
                }
                match a.goals() {
                    Ok((g, _)) => {
                        snap.goals = g;
                        snap.journal = a.journal();
                        snap.needs = a.needs(&snap.journal);
                        snap.handovers = a.handovers();
                        snap.deadlines = crate::missables::deadlines(&snap.journal, &a.deeds());
                        if let Some(w) = snap.world.as_deref() {
                            (snap.collection, snap.stories) = a.collection(w);
                        }
                        snap.secret_totals = crate::quests::Kind::SECRETS.map(|(k, _)| a.secret_total(k));
                        snap.obstacles = a.obstacles();
                        snap.nav = a.nav();
                    }
                    Err(e) => snap.notice = Some(format!("minimap: {e}")),
                }
                match a.session() {
                    Err(e) => snap.notice = Some(e),
                    Ok(s) => {
                        match hold::tick(&s, &self.active, &mut self.originals) {
                            Ok(errors) if errors.is_empty() => {}
                            Ok(errors) => snap.notice = Some(errors.join("; ")),
                            Err(e) => {
                                self.active.clear();
                                self.notice = Some(format!("cannot record originals — toggles off: {e}"));
                            }
                        }
                        snap.values = cheats::attributes().into_iter().map(|a| (a, s.current(a).ok())).collect();
                    }
                }
                let errors = self.extras.tick(a, &self.active);
                if !errors.is_empty() && snap.notice.is_none() {
                    snap.notice = Some(errors.join("; "));
                }
            }
        }
        snap.active = self.active.clone();
        snap.pending = self.originals.len();
        snap.originals = self.originals.entries();
        snap.notice = self.notice.take().or(snap.notice);
        snap
    }

    pub fn set(&mut self, name: &str, v: f32) -> Result<(), String> {
        self.refresh()?;
        let a = self.attached.as_ref().unwrap();
        a.gate()?;
        if let Some(Kind::SetStock { class, max, .. }) = cheats::find(name).map(|c| c.kind) {
            if !(1.0..=max).contains(&v) {
                return Err(format!("{name} takes 1..={max}"));
            }
            return self.extras.set_stock(a, class, v as u32).map(drop);
        }
        cheats::set_value(&a.session()?, name, v)
    }

    /// Remember where the hero stands, in slot `i`.
    pub fn save_position(&mut self, i: usize) -> Result<[f64; 3], String> {
        self.refresh()?;
        let a = self.attached.as_ref().unwrap();
        a.gate()?;
        let (p, _) = a.pose()?;
        let world = a.chain()?.world(&a.game, &a.anchors)?;
        *self.slots.get_mut(i).ok_or("no such slot")? = Some((world, p));
        Ok(p)
    }

    /// Back to slot `i` — only in the world it was saved in.
    pub fn load_position(&mut self, i: usize) -> Result<(), String> {
        self.refresh()?;
        let a = self.attached.as_ref().unwrap();
        a.gate()?;
        let (world, p) = self.slots.get(i).cloned().flatten().ok_or("nothing saved in that slot")?;
        if a.chain()?.world(&a.game, &a.anchors)? != world {
            return Err(format!("saved in another area ({world})"));
        }
        a.teleport([p[0], p[1], p[2] + LIFT])
    }

    /// Replace the active toggles. Turning anything on needs the gate; whatever a
    /// toggle being dropped had overwritten goes back at once.
    pub fn set_active(&mut self, toggles: Vec<Active>) -> Result<(), String> {
        let a = self.attached()?;
        if !toggles.is_empty() {
            a.gate()?;
        }
        let keep: Vec<Attr> = toggles.iter().flat_map(|t| t.restores()).collect();
        let drop: Vec<Attr> = self.active.iter().flat_map(|t| t.restores()).filter(|x| !keep.contains(x)).collect();
        self.active = toggles;
        let a = self.attached.as_ref().unwrap();
        self.extras.release(a, &self.active);
        if drop.is_empty() {
            return Ok(());
        }
        let failed = self.originals.restore_only(&a.session()?, &drop);
        if failed.is_empty() {
            Ok(())
        } else {
            Err(format!("could not restore: {}", failed.join("; ")))
        }
    }

    /// Switch everything off and put back every original on record. Only while the
    /// hero is in play: the record names attributes, and only the hero's are ours.
    pub fn stop(&mut self) -> Result<(), String> {
        self.active.clear();
        if let Some(a) = self.attached.as_ref() {
            self.extras.release(a, &[]);
        }
        if self.originals.is_empty() {
            return Ok(());
        }
        self.refresh()?;
        let a = self.attached.as_ref().unwrap();
        a.gate()?;
        let s = a.session()?;
        let failed = self.originals.restore(&s);
        if failed.is_empty() {
            Ok(())
        } else {
            Err(format!("could not restore: {} — run `hiumod restore` later", failed.join("; ")))
        }
    }
}
