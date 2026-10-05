//! The game attached: its memory and anchors, the hero's chain learned once, and
//! the readers kept between steps (actors, footprints, the guide's caches) — each
//! query the engine makes of the game is a method here.

use super::*;

/// A survey place the hero stays within `ABSENT_NEAR` (cm across) and `ABSENT_HEIGHT`
/// (cm up or down) of for `ABSENT_AFTER` with nothing of its name loaded there is empty
/// now: the world streams in what is that close well before then. Someone the story
/// moves on (Father Jaffer's talk by Lake Cynon, measured) is placed in the level all the
/// same, and was guided to with no one there.
const ABSENT_NEAR: f32 = 4000.0;
const ABSENT_HEIGHT: f32 = 1000.0;
const ABSENT_AFTER: Duration = Duration::from_secs(6);

/// The navmesh as routes use it: made from `from`, through the doors `opened`, one way
/// through the shut one-sided doors `one_way`.
struct Bridged {
    from: Arc<crate::navmesh::NavMesh>,
    opened: Vec<([f32; 3], Option<[f32; 3]>)>,
    one_way: Vec<[[f32; 3]; 2]>,
    mesh: Arc<crate::navmesh::NavMesh>,
}

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
    /// What the graph's answers that depend on the knowledge alone (what is known, the logic
    /// puzzles, the doors passable and one-way) were worked out for: the world and the
    /// knowledge's reading. They are kept until either changes, not worked out every step.
    graph_for: Option<(String, Option<Instant>)>,
    known_all: HashSet<String>,
    goals: Goals,
    /// The quest journal, rebuilt a slice at a time.
    quests: crate::quests::Quests,
    /// The game's navmesh, read a little at a time.
    nav: crate::navmesh::Nav,
    /// The doors of the hero's world it can pass (graph.rs `passable`), and the navmesh
    /// with a way through each, for the mesh it was made from.
    opened: Vec<([f32; 3], Option<[f32; 3]>)>,
    /// The shut one-sided doors of the hero's world and the side each opens from.
    one_way: Vec<[[f32; 3]; 2]>,
    bridged: Option<Bridged>,
    /// The survey of every world (Mods\survey), read once; and what the hero knows and
    /// holds by name, to judge it with — renewed with the knowledge.
    survey: Option<crate::survey::Survey>,
    /// The requirement graph of the whole game (graph.rs), from the survey, read once.
    graph: Option<crate::graph::Graph>,
    /// The order and position puzzles of the hero's world (graph.rs), as of the last step.
    logic_puzzles: Vec<crate::graph::LogicPuzzle>,
    /// The shut barriers of the hero's world and their chains' first steps (graph.rs).
    doors: Vec<crate::graph::DoorStep>,
    /// The game's spawner and vault tables (Mods\survey), read once.
    tables: Option<crate::tables::Tables>,
    /// `gamedata::generation` when those two were read: new files read again.
    data: u64,
    known_facts: HashSet<String>,
    known_tags: HashSet<String>,
    held: HashSet<String>,
    /// How many of each the hero holds, by asset name — shards by the stack.
    counts: HashMap<String, u32>,
    saved: HashSet<String>,
    name_cache: HashMap<u32, String>,
    fact_keys: HashMap<String, String>,
    /// The save state the knowledge was read from.
    save: u64,
    /// What stands in the way: collision shapes, collected a slice per step.
    obstacles: Obstacles,
    /// Where each NPC's conversation was last seen loaded, by flow (`Survey::goals`).
    met: HashMap<String, String>,
    /// The survey's places found empty (`ABSENT_*`), by id, in `empty_world`; and since
    /// when the hero has been near each not loaded.
    empty: HashSet<u64>,
    near_since: HashMap<u64, Instant>,
    empty_world: String,
    /// What each choice puzzle's slot was last seen to hold (slots.rs).
    slots_seen: crate::slots::Seen,
}

impl Guide {
    /// New game data written since the survey and tables were read (gamedata.rs):
    /// drop them, to be read again.
    fn fresh(&mut self) {
        let now = crate::gamedata::generation();
        if self.data != now {
            self.survey = None;
            self.tables = None;
            self.data = now;
        }
    }
}

const SAVES_EVERY: Duration = Duration::from_secs(60);
const KNOWLEDGE_EVERY: Duration = Duration::from_secs(2);

fn mem_ptr(m: &dyn crate::mem::Memory, at: u64) -> Result<u64, String> {
    crate::mem::read_u64(m, at).filter(|&p| crate::mem::plausible(p)).ok_or_else(|| tr!("POINTER_UNREADABLE").into())
}

/// The GUIDs of the placed things the save keeps a state for, in every region
/// (`World.RegionStates[].ElementStates[].Identifier`), as `XXXXXXXX-XXXXXXXX-…` like the
/// survey's.
/// A GUID as the survey writes it: four little-endian u32, hex.
fn guid_at(m: &dyn crate::mem::Memory, at: u64) -> Option<String> {
    let mut g = [0u8; 16];
    m.read(at, &mut g).then(|| {
        let part = |i: usize| u32::from_le_bytes(g[i * 4..i * 4 + 4].try_into().unwrap());
        format!("{:08X}-{:08X}-{:08X}-{:08X}", part(0), part(1), part(2), part(3))
    })
}

pub fn saved_guids(m: &dyn crate::mem::Memory, n: &crate::names::Names, save: u64) -> HashSet<String> {
    let mut out = HashSet::new();
    let Some((at, p)) = n.path(m, save, &["World", "RegionStates"]) else { return out };
    let size =
        |field: u64| n.inner_of(m, field).and_then(|i| crate::mem::read_u32(m, i + n.layout.size)).unwrap_or(0) as u64;
    let region_size = size(p.field);
    let Some(region_struct) = n.inner_of(m, p.field).and_then(|i| n.struct_of(m, i)) else { return out };
    let Some(states) = n.find(m, region_struct, "ElementStates") else { return out };
    let element_size = size(states.field);
    if region_size == 0 || element_size < 16 {
        return out;
    }
    for region in crate::actors::array_of(m, at, region_size, 64) {
        out.extend(
            crate::actors::array_of(m, region + states.offset as u64, element_size, 100_000)
                .into_iter()
                .filter_map(|e| guid_at(m, e)),
        );
    }
    out
}

pub fn attach() -> Result<Attached, String> {
    let game = Game::find()?.ok_or(tr!("THE_GAME_IS_NOT_RUNNING"))?;
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
            .ok_or(tr!("THE_HEROS_INVENTORY_WAS_NOT_FOUND"))?;
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

    /// The Hazes and the Hollow Walkers they keep alive, as of the last `things`.
    pub fn haze_links(&self) -> Vec<crate::actors::HazeLink> {
        self.scanner.borrow().links()
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
            return Err(tr!("COULD_NOT_WRITE_THE_HEROS_POSITION").into());
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
        let p = n.field(m, settings, "Pauser").ok_or(tr!("NO_WORLDSETTINGS_PAUSER"))?;
        Ok(crate::mem::read_u64(m, settings + p.offset as u64).is_some_and(|v| v != 0))
    }

    /// Places where the hero can still learn something, and what the hero knows —
    /// for the compass and the guide.
    pub fn goals(&self) -> Result<(Vec<Goal>, Knowledge), String> {
        let chain = self.chain()?;
        let hero = chain.hero(&self.game, &self.anchors)?;
        let (m, n) = (&self.game, &self.anchors.names);
        let mut g = self.guide.borrow_mut();
        g.fresh();
        if g.objects.is_none() {
            g.objects = Some(gobjects::discover(m, self.game.base)?);
        }
        // The save objects: the quest pass finds them as it walks every object (a new
        // one each time the game saves — then the knowledge is read again at once);
        // until its first pass, a search of its own (~0.4 s).
        let seen = g.quests.saves();
        if !seen.is_empty() && seen != g.saves.as_slice() {
            g.saves = seen.to_vec();
            g.saves_read = Some(Instant::now());
            g.knowledge_read = None;
        } else if g.saves.is_empty() || seen.is_empty() && g.saves_read.is_none_or(|t| t.elapsed() >= SAVES_EVERY) {
            let _t = crate::prof::span("saves");
            g.saves = g.objects.as_ref().unwrap().of_class(m, n, "CharlieSaveGame");
            g.saves_read = Some(Instant::now());
        }
        if g.knowledge.is_none() || g.knowledge_read.is_none_or(|t| t.elapsed() >= KNOWLEDGE_EVERY) {
            let _t = crate::prof::span("knowledge");
            let save = knowledge::current(n, m, &g.saves).ok_or(tr!("NO_SAVE_STATE_FOUND"))?;
            g.save = save;
            g.knowledge = Some(knowledge::read(n, m, save).ok_or(tr!("THE_SAVE_STATE_COULD_NOT_BE"))?);
            g.knowledge_read = Some(Instant::now());
            // The same by name, for the survey.
            let g = &mut *g;
            let k = g.knowledge.as_ref().unwrap();
            let mut name = |i: u32| g.name_cache.entry(i).or_insert_with(|| n.get(m, i).unwrap_or_default()).clone();
            g.known_facts = k.facts.iter().map(|&i| name(i)).collect();
            crate::i18n::set_known(&g.known_facts);
            g.known_tags = k.tags.iter().map(|&i| name(i)).collect();
            g.counts = self.held_counts();
            g.held = g.counts.keys().cloned().collect();
            g.saved = saved_guids(m, n, save);
            g.fact_keys = g.quests.fact_keys();
        }
        let actors = self.scanner.borrow().actors_offset().ok_or(tr!("ACTORS_NOT_SCANNED_YET"))?;
        {
            let _t = crate::prof::span("goals.refresh");
            let g = &mut *g;
            g.goals.refresh(m, n, hero, chain.root, chain.location, actors, g.quests.flows());
        }
        if let Ok((p, _)) = chain.pose(m, &self.anchors) {
            let _t = crate::prof::span("obstacles");
            let objects = g.objects.take().unwrap();
            // The world names the scene kept on disk (scene_cache.rs); unknown, none is.
            let world = chain.world(m, &self.anchors).unwrap_or_default();
            g.obstacles.step(m, n, &objects, p, &world);
            g.objects = Some(objects);
        }
        {
            let g = &mut *g;
            let objects = g.objects.as_ref().unwrap();
            {
                let _t = crate::prof::span("quests");
                g.quests.step(m, n, || objects.all(m));
            }
            let _t = crate::prof::span("nav");
            g.nav.step(m, n, g.quests.nav_actors());
        }
        let k = g.knowledge.clone().unwrap();
        let mut goals = {
            let _t = crate::prof::span("goals.evaluate");
            g.goals.evaluate(m, &k, chain.location)
        };
        // What the survey knows of this world beyond what is loaded.
        let g = &mut *g;
        let survey =
            g.survey.get_or_insert_with(|| crate::survey::Survey::load(&crate::paths::data_dir().join("survey")));
        if let Ok(world) = chain.world(m, &self.anchors) {
            let _t = crate::prof::span("survey");
            let known = crate::survey::Known {
                facts: &g.known_facts,
                tags: &g.known_tags,
                held: &g.held,
                saved: &g.saved,
                talked: &g.goals.done_npcs,
            };
            let w = crate::survey::Survey::world_of(&world);
            if g.empty_world != w {
                g.empty.clear();
                g.near_since.clear();
                g.empty_world = w.to_string();
            }
            if let (Ok((p, _)), Some(list)) = (chain.pose(m, &self.anchors), survey.worlds.get(w)) {
                let p = [p[0] as f32, p[1] as f32, p[2] as f32];
                for e in list {
                    let id = e.id();
                    let near = (e.at[0] - p[0]).hypot(e.at[1] - p[1]) <= ABSENT_NEAR
                        && (e.at[2] - p[2]).abs() <= ABSENT_HEIGHT;
                    if !near || crate::survey::is_loaded(e, list, &g.goals.loaded) {
                        g.near_since.remove(&id);
                        if near {
                            g.empty.remove(&id);
                        }
                        continue;
                    }
                    if g.near_since.entry(id).or_insert_with(Instant::now).elapsed() >= ABSENT_AFTER {
                        g.empty.insert(id);
                    }
                }
            }
            goals.extend(survey.goals(w, &known, &g.goals.loaded, &g.fact_keys, &mut g.met, &g.empty));
            // What must come first (requires.rs): the place to put items held, and what
            // gives an item a placement still takes.
            goals.extend(crate::requires::placement_goals(survey, w, &g.saved, &g.held));
            crate::requires::mark_givers(&mut goals, survey, w, &g.saved, &g.held);
            // What must come first, by the whole game's graph: a goal whose place needs
            // something not done yet gives way to the first thing of its chain.
            let graph =
                g.graph.get_or_insert_with(|| crate::graph::Graph::load(&crate::paths::data_dir().join("survey")));
            let key = (w.to_string(), g.knowledge_read);
            let fresh = g.graph_for.as_ref() != Some(&key);
            if fresh {
                let mut known: HashSet<String> = g.known_facts.union(&g.known_tags).cloned().collect();
                // Standing in a region, it is reached, whatever the APC knows (graph.rs:
                // regions are gated by their travel fact).
                known.insert(format!("WMA_{}_Travel_BifrostTransitionFact_DA", crate::survey::Survey::world_of(w)));
                g.known_all = known;
            }
            let state = crate::graph::State { used: &g.saved, known: &g.known_all, held: &g.held };
            // Only when the player asked for it (Settings: what must come first).
            let steps = crate::settings::live(crate::settings::Consent::STEPS);
            if steps {
                graph.gate(&mut goals, w, &state);
                // Under deadly water now: held back, the drain's chain guided to instead.
                let done = g.obstacles.done.clone();
                graph.flood(&mut goals, w, &done.pools, &state);
            }
            if fresh {
                g.logic_puzzles = graph.logic_puzzles(w, &state);
                g.opened = graph.passable(w, &state);
                g.one_way = graph.one_way(w, &state);
                g.graph_for = Some(key);
            }
            g.doors = if steps { graph.door_steps(&goals, w, &state) } else { Vec::new() };
        }
        Ok((goals, k))
    }

    /// The quest journal against what the hero knows now — empty until its first pass.
    pub fn journal(&self) -> Vec<crate::quests::Quest> {
        let g = self.guide.borrow();
        match (&g.knowledge, g.quests.ready()) {
            (Some(k), true) => {
                g.quests.journal(k, &crate::quests::deed_states(&self.game, &self.anchors.names, g.save))
            }
            _ => Vec::new(),
        }
    }

    /// The item assets the hero holds, by name, and how many: a stack's count is the
    /// u32 right after its `ItemData` pointer (cheat/extras.rs reads it the same way).
    fn held_counts(&self) -> HashMap<String, u32> {
        let (m, n) = (&self.game, &self.anchors.names);
        let mut out = HashMap::new();
        let Ok(inv) = self.inventory() else { return out };
        let Some(items) = n.field(m, inv, "Items") else { return out };
        for item in crate::actors::array(m, inv + items.offset as u64, 4096) {
            let Some(data) = n.field(m, item, "ItemData") else { continue };
            let at = item + data.offset as u64;
            let Some(def) = crate::mem::read_u64(m, at).filter(|&p| crate::mem::plausible(p)) else { continue };
            let Some(name) = n.object(m, def) else { continue };
            // A count no stack could hold is not a count: the layout moved; it is still held.
            let count = crate::mem::read_u32(m, at + 8).filter(|&c| (1..=100_000).contains(&c)).unwrap_or(1);
            *out.entry(name).or_insert(0) += count;
        }
        out
    }

    /// The shut barriers of the hero's world, with what opens each first (the guide).
    pub fn doors(&self) -> Vec<crate::graph::DoorStep> {
        self.guide.borrow().doors.clone()
    }

    /// The order and position puzzles of the hero's world (the Puzzles page).
    pub fn logic_puzzles(&self) -> Vec<crate::graph::LogicPuzzle> {
        self.guide.borrow().logic_puzzles.clone()
    }

    /// The ways out of `world` the survey knows: the APC's door and the save points.
    pub fn exits(&self, world: &str) -> Vec<(crate::actors::Sub, [f32; 3])> {
        let g = self.guide.borrow();
        g.survey.as_ref().map(|s| s.exits(crate::survey::Survey::world_of(world))).unwrap_or_default()
    }

    /// The collectibles' counts here and everywhere, and the NPCs with more to tell.
    pub fn collection(&self, world: &str) -> (Vec<crate::survey::Collect>, Vec<crate::survey::Need>) {
        let g = self.guide.borrow();
        let Some(survey) = g.survey.as_ref() else { return Default::default() };
        let known = crate::survey::Known {
            facts: &g.known_facts,
            tags: &g.known_tags,
            held: &g.held,
            saved: &g.saved,
            talked: &g.goals.done_npcs,
        };
        (survey.collection(crate::survey::Survey::world_of(world), &known), survey.stories(&known))
    }

    /// The research state of the save: entries known, vaults known (in the datapad)
    /// and opened, by GUID — `Player.ResearchState`.
    fn research(&self) -> (usize, HashSet<String>, HashSet<String>) {
        let (m, n) = (&self.game, &self.anchors.names);
        let save = self.guide.borrow().save;
        // `shown`: only the entries whose bIsShownToPlayer (+0x11) is set — every vault
        // has an entry, from the start.
        let guids = |field: &str, shown: bool| -> (usize, HashSet<String>) {
            let Some((at, p)) = n.path(m, save, &["Player", "ResearchState", field]) else {
                return (0, HashSet::new());
            };
            let size =
                n.inner_of(m, p.field).and_then(|i| crate::mem::read_u32(m, i + n.layout.size)).unwrap_or(16) as u64;
            let items = crate::actors::array_of(m, at, size.max(16), 4096);
            let flag = |e: u64| {
                let mut b = [0u8; 1];
                !shown || (m.read(e + 0x11, &mut b) && b[0] != 0)
            };
            (items.len(), items.into_iter().filter(|&e| flag(e)).filter_map(|e| guid_at(m, e)).collect())
        };
        let (lore, _) = guids("KnownLoreEntries", false);
        (lore, guids("KnownCacheEntries", true).1, guids("OpenedCaches", false).1)
    }

    /// The vault notebook (F7): every vault, known or opened, with its door; and the
    /// research entries known.
    pub fn vaults(&self) -> (Vec<crate::tables::VaultNote>, usize) {
        let (lore, known, opened) = self.research();
        let mut g = self.guide.borrow_mut();
        g.fresh();
        let g = &mut *g;
        let tables =
            g.tables.get_or_insert_with(|| crate::tables::Tables::load(&crate::paths::data_dir().join("survey")));
        let doors = g.survey.as_ref().map(|s| s.doors.as_slice()).unwrap_or_default();
        (tables.vaults(&known, &opened, lore, doors), lore)
    }

    /// Every world's Hollows left (F8).
    pub fn hollows(&self) -> Vec<crate::tables::Hollows> {
        let mut g = self.guide.borrow_mut();
        g.fresh();
        let g = &mut *g;
        let tables =
            g.tables.get_or_insert_with(|| crate::tables::Tables::load(&crate::paths::data_dir().join("survey")));
        tables.hollows(&g.saved)
    }

    /// Every puzzle the survey found, and whether the save says it was solved (its
    /// GUID has a state).
    pub fn catalogue(&self) -> Vec<(crate::survey::Placed, bool)> {
        let g = self.guide.borrow();
        let Some(survey) = g.survey.as_ref() else { return Vec::new() };
        survey.puzzles.iter().map(|p| (p.clone(), p.guid.as_ref().is_some_and(|id| g.saved.contains(id)))).collect()
    }

    /// The choice puzzles (slots.rs), with what their slots were last seen to hold: the
    /// loaded ones among `live`.
    pub fn slot_puzzles(
        &self,
        catalogue: &[(crate::survey::Placed, bool)],
        live: &[crate::puzzles::Puzzle],
    ) -> Vec<crate::slots::SlotPuzzle> {
        let mut g = self.guide.borrow_mut();
        g.slots_seen.see(catalogue, live);
        crate::slots::group(catalogue, &g.slots_seen)
    }

    /// The puzzles within `reach` (cm) of `here`, with their answers (F6).
    pub fn puzzles(&self, here: [f32; 3], reach: f32) -> Vec<crate::puzzles::Puzzle> {
        let (m, n) = (&self.game, &self.anchors.names);
        let g = self.guide.borrow();
        let mut out: Vec<crate::puzzles::Puzzle> = g
            .quests
            .puzzles()
            .iter()
            .filter_map(|&(comp, kind)| crate::puzzles::read(m, n, comp, kind))
            .filter(|p| {
                ((p.at[0] - here[0]).powi(2) + (p.at[1] - here[1]).powi(2)).sqrt() <= reach
                    && (p.at[2] - here[2]).abs() <= reach
            })
            .collect();
        let d = |p: &crate::puzzles::Puzzle| ((p.at[0] - here[0]).powi(2) + (p.at[1] - here[1]).powi(2)).sqrt();
        out.sort_by(|a, b| d(a).total_cmp(&d(b)));
        out
    }

    /// Every secret of a kind: how many the game has.
    pub fn secret_total(&self, kind: crate::quests::Kind) -> usize {
        self.guide.borrow().quests.secrets(kind).len()
    }

    /// Every good deed the game has: (journal key, title, tag stem).
    /// Every good deed's, mystery's and timeloop's tag prefix and title in the game's
    /// language, begun or not: what names a trigger that only sets their tags.
    ///
    /// A deed's row names only its started, completed, failed and rewarded tags; the steps
    /// between are tags of their own (`Secrets.Facts.PhotoAcquired` on the way to
    /// `ClassroomPicture`'s). Tags handed out together, by one payload or one hand-over in
    /// the survey, belong to the same deed: each of them is added under its title too.
    pub fn secret_titles(&self) -> Vec<(String, String)> {
        let g = self.guide.borrow();
        let deeds: Vec<(String, String)> = crate::quests::Kind::SECRETS
            .iter()
            .flat_map(|(k, _)| g.quests.secrets(*k))
            .filter(|(_, title, tags)| !tags.is_empty() && !title.is_empty())
            .map(|(_, title, tags)| (tags, title))
            .collect();
        let groups: Vec<&[String]> = g
            .survey
            .iter()
            .flat_map(|s| s.worlds.values().flatten())
            .flat_map(|e| std::iter::once(e.tags.as_slice()).chain(e.trades.iter().map(|(_, _, t)| t.as_slice())))
            .collect();
        let mut out = deeds.clone();
        out.extend(crate::goals::tags_together(&deeds, groups));
        out
    }

    pub fn deeds(&self) -> Vec<(String, String, String)> {
        self.guide.borrow().quests.secrets(crate::quests::Kind::GoodDeed)
    }

    /// What the hero knows from the Datapad, by entry, and the items held by name — the
    /// clue board (JOURNEY.md §3.2).
    pub fn clues(&self) -> crate::clues::Clues {
        let g = self.guide.borrow();
        let lang = crate::i18n::lang();
        let subjects = crate::clues::subjects(
            &g.known_facts,
            |f| lang.names.fact(f),
            |unit| lang.names.subject(unit, &g.known_facts),
        );
        let mut items: Vec<String> = g.held.iter().filter_map(|i| lang.names.item(i)).collect();
        items.sort();
        items.dedup();
        crate::clues::Clues { subjects, items, text: lang.names.has_facts() }
    }

    /// What the three upgrade achievements still cost in shards (JOURNEY.md §3.6).
    pub fn budget(&self) -> crate::budget::Budget {
        let mut g = self.guide.borrow_mut();
        g.fresh();
        let g = &mut *g;
        let tables =
            g.tables.get_or_insert_with(|| crate::tables::Tables::load(&crate::paths::data_dir().join("survey")));
        if tables.recipes.is_empty() {
            return Default::default();
        }
        crate::budget::budget(&tables.recipes, &g.counts)
    }

    /// Every Lymbic lock, with the rods held and where the missing ones are (JOURNEY.md §3.3).
    pub fn locks(&self) -> Vec<crate::survey::Lock> {
        let g = self.guide.borrow();
        let Some(survey) = g.survey.as_ref() else { return Vec::new() };
        let known = crate::survey::Known {
            facts: &g.known_facts,
            tags: &g.known_tags,
            held: &g.held,
            saved: &g.saved,
            talked: &g.goals.done_npcs,
        };
        survey.locks(&known)
    }

    /// NPCs that want an item the hero holds (the survey's trades).
    pub fn handovers(&self) -> Vec<crate::survey::Need> {
        let g = self.guide.borrow();
        let Some(survey) = g.survey.as_ref() else { return Vec::new() };
        let known = crate::survey::Known {
            facts: &g.known_facts,
            tags: &g.known_tags,
            held: &g.held,
            saved: &g.saved,
            talked: &g.goals.done_npcs,
        };
        survey.handovers(&known)
    }

    /// For each quest under way, the places it needs in every world (from the survey).
    pub fn needs(&self, journal: &[crate::quests::Quest]) -> Vec<(String, Vec<crate::survey::Need>)> {
        let g = self.guide.borrow();
        let Some(survey) = g.survey.as_ref().filter(|s| !s.is_empty()) else { return Vec::new() };
        let known = crate::survey::Known {
            facts: &g.known_facts,
            tags: &g.known_tags,
            held: &g.held,
            saved: &g.saved,
            talked: &g.goals.done_npcs,
        };
        journal
            .iter()
            .filter(|q| q.active())
            .map(|q| (q.key.clone(), survey.needs(&q.key, q.tags.as_deref(), &known, &g.fact_keys)))
            .collect()
    }

    /// The navmesh as last read, with a way through each door opened and one way only
    /// through each one-sided door still shut (shared; made anew only when the mesh or the
    /// doors change).
    pub fn nav(&self) -> Arc<crate::navmesh::NavMesh> {
        let mut g = self.guide.borrow_mut();
        let base = g.nav.done.clone();
        if g.opened.is_empty() && g.one_way.is_empty() {
            return base;
        }
        if let Some(b) = &g.bridged {
            if Arc::ptr_eq(&b.from, &base) && b.opened == g.opened && b.one_way == g.one_way {
                return b.mesh.clone();
            }
        }
        let mesh = Arc::new(base.bridged(&g.opened).one_way(&g.one_way));
        g.bridged =
            Some(Bridged { from: base, opened: g.opened.clone(), one_way: g.one_way.clone(), mesh: mesh.clone() });
        mesh
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

impl crate::extras::Reach for Attached {
    fn memory(&self) -> &dyn crate::mem::Memory {
        &self.game
    }
    fn names(&self) -> &crate::names::Names {
        &self.anchors.names
    }
    fn base(&self) -> u64 {
        self.game.base
    }
    fn hero(&self) -> Result<u64, String> {
        self.chain().and_then(|c| c.hero(&self.game, &self.anchors))
    }
    fn enemies(&self) -> Vec<u64> {
        Attached::enemies(self)
    }
    fn inventory(&self) -> Result<u64, String> {
        Attached::inventory(self)
    }
}
