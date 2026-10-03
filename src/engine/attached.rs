//! The game attached: its memory and anchors, the hero's chain learned once, and
//! the readers kept between steps (actors, footprints, the guide's caches) — each
//! query the engine makes of the game is a method here.

use super::*;

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
    crate::mem::read_u64(m, at).filter(|&p| crate::mem::plausible(p)).ok_or_else(|| tr!("포인터를 읽을 수 없음").into())
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
    let game = Game::find()?.ok_or(tr!("게임이 실행 중이 아닙니다"))?;
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
            .ok_or(tr!("주인공의 인벤토리를 찾지 못함"))?;
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
            return Err(tr!("주인공 위치를 쓰지 못함").into());
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
        let p = n.field(m, settings, "Pauser").ok_or(tr!("WorldSettings.Pauser 없음"))?;
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
        // The save objects: the quest pass finds them as it walks every object (a new
        // one each time the game saves — then the knowledge is read again at once);
        // until its first pass, a search of its own (~0.4 s).
        let seen = g.quests.saves();
        if !seen.is_empty() && seen != g.saves.as_slice() {
            g.saves = seen.to_vec();
            g.saves_read = Some(Instant::now());
            g.knowledge_read = None;
        } else if g.saves.is_empty() || seen.is_empty() && g.saves_read.is_none_or(|t| t.elapsed() >= SAVES_EVERY) {
            g.saves = g.objects.as_ref().unwrap().of_class(m, n, "CharlieSaveGame");
            g.saves_read = Some(Instant::now());
        }
        if g.knowledge.is_none() || g.knowledge_read.is_none_or(|t| t.elapsed() >= KNOWLEDGE_EVERY) {
            let save = knowledge::current(n, m, &g.saves).ok_or(tr!("세이브 상태를 찾지 못함"))?;
            g.save = save;
            g.knowledge = Some(knowledge::read(n, m, save).ok_or(tr!("세이브 상태를 읽지 못함"))?);
            g.knowledge_read = Some(Instant::now());
            // The same by name, for the survey.
            let g = &mut *g;
            let k = g.knowledge.as_ref().unwrap();
            let mut name = |i: u32| g.name_cache.entry(i).or_insert_with(|| n.get(m, i).unwrap_or_default()).clone();
            g.known_facts = k.facts.iter().map(|&i| name(i)).collect();
            crate::i18n::set_known(&g.known_facts);
            g.known_tags = k.tags.iter().map(|&i| name(i)).collect();
            g.held = self.held_items();
            g.saved = saved_guids(m, n, save);
            g.fact_keys = g.quests.fact_keys();
        }
        let actors = self.scanner.borrow().actors_offset().ok_or(tr!("아직 액터를 훑지 않음"))?;
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
