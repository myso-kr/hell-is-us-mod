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
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

mod attached;
mod snapshot;

pub use attached::{attach, saved_guids, Attached};
pub use snapshot::{Snapshot, SLOTS};

/// A teleport lands this far above the saved spot (cm), so it does not start in the
/// ground.
const LIFT: f64 = 50.0;
/// A teleport to a place followed lands this far short of it, toward the hero, and this
/// far above it (cm): not inside what stands there (a chest, a person), and on its floor.
const SHORT_OF: f64 = 150.0;
const ABOVE: f64 = 120.0;

/// What the journal and the survey give, worked out once a second (`DERIVE_EVERY`).
struct Derived {
    journal: Arc<Vec<crate::quests::Quest>>,
    needs: Arc<Vec<(String, Vec<crate::survey::Need>)>>,
    handovers: Arc<Vec<crate::survey::Need>>,
    deadlines: Arc<Vec<crate::missables::Deadline>>,
    collection: Arc<Vec<crate::survey::Collect>>,
    stories: Arc<Vec<crate::survey::Need>>,
    secret_totals: [usize; 3],
    vaults: Arc<Vec<crate::tables::VaultNote>>,
    lore_known: usize,
    hollows: Arc<Vec<crate::tables::Hollows>>,
    puzzles: Arc<Vec<crate::puzzles::Puzzle>>,
    catalogue: Arc<Vec<(crate::survey::Placed, bool)>>,
    slot_puzzles: Arc<Vec<crate::slots::SlotPuzzle>>,
    locks: Arc<Vec<crate::survey::Lock>>,
    clues: Arc<crate::clues::Clues>,
    budget: Arc<crate::budget::Budget>,
    /// Every good deed's, mystery's and timeloop's tag prefix and title, begun or not.
    secret_titles: Arc<Vec<(String, String)>>,
}

/// Puzzles this near the hero (cm) are read and shown.
const PUZZLE_REACH: f32 = 4_000.0;

const DERIVE_EVERY: Duration = Duration::from_secs(1);

pub struct Engine {
    derived: Option<(Instant, Derived)>,
    attached: Option<Attached>,
    checked: Option<Instant>,
    originals: Originals,
    /// What the cheats past the hero overwrote (extras.rs).
    extras: Extras,
    active: Vec<Active>,
    notice: Option<String>,
    slots: [Option<(String, [f64; 3])>; SLOTS],
    /// Where the hero stood before teleporting to a place followed: a slot of its own, kept
    /// through further teleports until gone back to.
    before: Option<(String, [f64; 3])>,
    /// A filming take rolling (film.rs): dropped, it stops.
    take: Option<crate::film::Take>,
    /// Keep the game running when its window loses focus (the panel's setting): the game's
    /// own option is switched off while the panel runs, and back on after if it was.
    pub keep_running: bool,
    /// Where that option was switched off by the panel, to switch it back on; when last looked.
    unpaused: Option<u64>,
    focus_checked: Option<Instant>,
}

/// How often to look for the game, or check it is still the same process. Listing
/// processes is the one expensive thing a step does.
const RECHECK: Duration = Duration::from_secs(2);

impl Engine {
    pub fn new() -> Result<Engine, String> {
        Ok(Engine {
            derived: None,
            attached: None,
            checked: None,
            originals: Originals::load(&hold::default_path())?,
            extras: Extras::new(),
            slots: Default::default(),
            before: None,
            take: None,
            keep_running: true,
            unpaused: None,
            focus_checked: None,
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
            return if self.attached.is_some() { Ok(()) } else { Err(tr!("THE_GAME_IS_NOT_RUNNING").into()) };
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
                self.notice = Some(tr!("THE_GAME_EXITED_CHEATS_OFF").into());
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
            pose_src: None,
            world: None,
            things: Arc::default(),
            haze_links: Default::default(),
            footprints: Arc::default(),
            goals: Arc::default(),
            collection: Default::default(),
            stories: Default::default(),
            secret_totals: [0; 3],
            vaults: Default::default(),
            lore_known: 0,
            hollows: Default::default(),
            puzzles: Default::default(),
            catalogue: Default::default(),
            slot_puzzles: Default::default(),
            locks: Default::default(),
            clues: Default::default(),
            budget: Default::default(),
            deadlines: Default::default(),
            handovers: Default::default(),
            needs: Default::default(),
            nav: Default::default(),
            journal: Default::default(),
            paused: false,
            obstacles: Arc::default(),
            exits: Arc::default(),
            logic_puzzles: Arc::default(),
            doors: Arc::default(),
            door_here: None,
            slots: self.slots.clone(),
        };
        if let Err(e) = self.refresh() {
            snap.game = Err(e.clone());
            snap.gate = Err(e);
        } else {
            let a = self.attached.as_ref().unwrap();
            snap.game = Ok((a.game.pid, a.version.clone()));
            let head = crate::prof::span("head");
            snap.gate = a.gate();
            snap.pose = a.pose().ok();
            snap.pose_src = a.chain().ok().and_then(|c| c.pose_source(&a.game, &a.anchors).ok());
            snap.paused = a.paused().unwrap_or(false);
            snap.world = a.chain().ok().and_then(|c| c.world(&a.game, &a.anchors).ok());
            drop(head);
            // A closed gate pauses the toggles rather than ending them: it closes on
            // every loading screen, and the player expects god mode to survive one.
            if snap.gate.is_ok() {
                let t = crate::prof::span("things");
                match a.things() {
                    Ok(t) => {
                        snap.things = Arc::new(t);
                        snap.footprints = a.footprints();
                        let links = a.haze_links();
                        // Logged as the count changes: a fight with a Haze shows here.
                        static LAST: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
                        if LAST.swap(links.len(), std::sync::atomic::Ordering::Relaxed) != links.len() {
                            crate::logfile::line(&format!("haze links: {}", links.len()));
                        }
                        snap.haze_links = Arc::new(links);
                    }
                    Err(e) => snap.notice = Some(trf!("MINIMAP_ERROR", e = e)),
                }
                drop(t);
                let t = crate::prof::span("goals");
                match a.goals() {
                    Ok((g, _)) => {
                        let mut goals = g;
                        // What follows from the journal and the survey changes with the
                        // knowledge (read every 2 s): worked out once a second, shared.
                        if self.derived.as_ref().is_none_or(|(at, _)| at.elapsed() >= DERIVE_EVERY) {
                            let _t = crate::prof::span("derive");
                            let journal = Arc::new(a.journal());
                            let (collection, stories) =
                                snap.world.as_deref().map(|w| a.collection(w)).unwrap_or_default();
                            let (vaults, lore_known) = a.vaults();
                            let here = snap.pose.map(|(p, _)| [p[0] as f32, p[1] as f32, p[2] as f32]);
                            let catalogue = a.catalogue();
                            let puzzles = here.map(|h| a.puzzles(h, PUZZLE_REACH)).unwrap_or_default();
                            let slot_puzzles = Arc::new(a.slot_puzzles(&catalogue, &puzzles));
                            let d = Derived {
                                vaults: Arc::new(vaults),
                                lore_known,
                                hollows: Arc::new(a.hollows()),
                                catalogue: Arc::new(catalogue),
                                slot_puzzles,
                                locks: Arc::new(a.locks()),
                                clues: Arc::new(a.clues()),
                                budget: Arc::new(a.budget()),
                                puzzles: Arc::new(puzzles),
                                needs: Arc::new(a.needs(&journal)),
                                handovers: Arc::new(a.handovers()),
                                deadlines: Arc::new(crate::missables::deadlines(&journal, &a.deeds())),
                                collection: Arc::new(collection),
                                stories: Arc::new(stories),
                                secret_totals: crate::quests::Kind::SECRETS.map(|(k, _)| a.secret_total(k)),
                                secret_titles: Arc::new(a.secret_titles()),
                                journal,
                            };
                            self.derived = Some((Instant::now(), d));
                        }
                        if let Some((_, d)) = &self.derived {
                            snap.journal = d.journal.clone();
                            snap.needs = d.needs.clone();
                            snap.handovers = d.handovers.clone();
                            snap.deadlines = d.deadlines.clone();
                            snap.collection = d.collection.clone();
                            snap.stories = d.stories.clone();
                            snap.secret_totals = d.secret_totals;
                            snap.vaults = d.vaults.clone();
                            snap.lore_known = d.lore_known;
                            snap.hollows = d.hollows.clone();
                            snap.puzzles = d.puzzles.clone();
                            snap.catalogue = d.catalogue.clone();
                            snap.slot_puzzles = d.slot_puzzles.clone();
                            snap.locks = d.locks.clone();
                            snap.clues = d.clues.clone();
                            snap.budget = d.budget.clone();
                        }
                        // Triggers the game's text did not name, named after the good deed,
                        // mystery or timeloop their tags say (begun or not).
                        if let Some((_, d)) = &self.derived {
                            crate::goals::name_by_secrets(&mut goals, &d.secret_titles);
                        }
                        // Then after the place the trigger's name holds, in the game's text.
                        let world = snap.world.as_deref().map(crate::survey::Survey::world_of);
                        crate::goals::name_by_place(&mut goals, world, crate::i18n::location, crate::i18n::region);
                        // Shared, not copied: the overlay and the panel's cards take it
                        // every frame.
                        snap.goals = Arc::new(goals);
                        snap.obstacles = a.obstacles();
                        snap.exits = Arc::new(snap.world.as_deref().map(|w| a.exits(w)).unwrap_or_default());
                        snap.logic_puzzles = Arc::new(a.logic_puzzles());
                        snap.doors = Arc::new(a.doors());
                        snap.door_here = a.door_here();
                        snap.nav = a.nav();
                    }
                    Err(e) => snap.notice = Some(trf!("MINIMAP_ERROR", e = e)),
                }
                drop(t);
                let _t = crate::prof::span("hold");
                match a.session() {
                    Err(e) => snap.notice = Some(e),
                    Ok(s) => {
                        match hold::tick(&s, &self.active, &mut self.originals) {
                            Ok(errors) if errors.is_empty() => {}
                            Ok(errors) => snap.notice = Some(errors.join("; ")),
                            Err(e) => {
                                self.active.clear();
                                self.notice = Some(trf!("COULD_NOT_RECORD_THE_ORIGINAL_VALUES", e = e));
                            }
                        }
                        snap.values = cheats::attributes().into_iter().map(|a| (a, s.current(a).ok())).collect();
                    }
                }
                // The game's pause on losing focus, every 2 s: off while asked, back after.
                if self.focus_checked.is_none_or(|t| t.elapsed() >= Duration::from_secs(2)) {
                    self.focus_checked = Some(Instant::now());
                    if let Some(at) = a.pause_on_focus_lost() {
                        let now = crate::mem::read_u64(&a.game, at).map(|v| (v & 0xFF) as u8);
                        if self.keep_running && now == Some(1) && a.game.write(at, &[0]) {
                            self.unpaused = Some(at);
                        } else if !self.keep_running && self.unpaused == Some(at) {
                            a.game.write(at, &[1]);
                            self.unpaused = None;
                        }
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
                return Err(trf!("SLOT_RANGE", name = name, max = max));
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
        *self.slots.get_mut(i).ok_or(tr!("NO_SUCH_SLOT"))? = Some((world, p));
        Ok(p)
    }

    /// To a place followed, in the hero's world only: a step short of it on the hero's
    /// side, a little above it (`SHORT_OF`, `ABOVE`).
    pub fn teleport_to(&mut self, world: &str, at: [f32; 3]) -> Result<(), String> {
        self.teleport_near(world, at, true)
    }

    /// To the spot `at` itself (a floor picked on the 3D map), not short of it.
    pub fn teleport_here(&mut self, world: &str, at: [f32; 3]) -> Result<(), String> {
        self.teleport_near(world, at, false)
    }

    /// `short`: land `SHORT_OF` before `at` on the hero's side (a place followed: the thing
    /// itself is often in a wall or on a table); else on `at`.
    fn teleport_near(&mut self, world: &str, at: [f32; 3], short: bool) -> Result<(), String> {
        self.refresh()?;
        let a = self.attached.as_ref().unwrap();
        a.gate()?;
        let here = a.chain()?.world(&a.game, &a.anchors)?;
        let of = crate::survey::Survey::world_of;
        if of(&here) != of(world) {
            return Err(trf!("TARGET_IN_ANOTHER_REGION", world = crate::i18n::place(of(world))));
        }
        let (p, _) = a.pose()?;
        let at = [at[0] as f64, at[1] as f64, at[2] as f64];
        let (dx, dy) = (p[0] - at[0], p[1] - at[1]);
        let d = dx.hypot(dy);
        let back = if short && d > 1.0 { SHORT_OF.min(d) / d } else { 0.0 };
        a.teleport([at[0] + dx * back, at[1] + dy * back, at[2] + ABOVE])?;
        // Where it stood, for going back (the first of a run of teleports: where it came from).
        self.before.get_or_insert((here, p));
        Ok(())
    }

    /// Roll a filming take (film.rs): the hero walked along `plan` by the stick's input, the
    /// camera turned as it says. A take already rolling is stopped first.
    pub fn film(
        &mut self,
        plan: crate::film::Plan,
        state: std::sync::Arc<std::sync::Mutex<crate::film::State>>,
        ended: std::sync::Arc<std::sync::Mutex<Option<Instant>>>,
    ) -> Result<(), String> {
        self.take = None;
        if plan.path.len() < 2 {
            return Err(tr!("FILM_NO_ROUTE").into());
        }
        self.refresh()?;
        let a = self.attached.as_ref().unwrap();
        a.gate()?;
        let (m, n) = (&a.game, &a.anchors.names);
        let chain = a.chain()?;
        let hero = chain.hero(m, &a.anchors)?;
        let pc = chain.controller(m, &a.anchors)?;
        let input =
            n.field(m, hero, "ControlInputVector").ok_or_else(|| trf!("NO_PROPERTY", name = "ControlInputVector"))?;
        // The exploration camera's own settings, which the game reads every frame.
        let config =
            n.follow(m, pc, "PlayerCameraManager").ok().and_then(|pcm| n.follow(m, pcm, "ExplorationConfig").ok());
        let setting = |name: &str| config.and_then(|c| n.field(m, c, name).map(|p| c + p.offset as u64));
        let wiring = crate::film::Wiring {
            input: hero + input.offset as u64,
            rotation: pc + chain.rotation,
            pose: chain.pose_source(m, &a.anchors)?,
            distance: setting("DefaultDistanceFromPlayer"),
            fov: setting("FieldOfView"),
        };
        self.take = Some(crate::film::roll(plan, wiring, state, ended));
        Ok(())
    }

    /// Whether a take is rolling now.
    pub fn rolling(&self) -> bool {
        self.take.is_some()
    }

    /// Stop the take rolling, if one is.
    pub fn cut(&mut self) {
        if let Some(t) = self.take.take() {
            t.stop();
        }
    }

    /// Back to where the hero stood before teleporting to a place followed; the slot is
    /// emptied either way.
    pub fn go_back(&mut self) -> Result<(), String> {
        self.refresh()?;
        let a = self.attached.as_ref().unwrap();
        a.gate()?;
        let (world, p) = self.before.take().ok_or(tr!("NOTHING_TO_GO_BACK_TO"))?;
        if a.chain()?.world(&a.game, &a.anchors)? != world {
            return Err(trf!("SAVED_IN_ANOTHER_REGION", world = crate::i18n::place(&world)));
        }
        a.teleport([p[0], p[1], p[2] + LIFT])
    }

    /// Back to slot `i` — only in the world it was saved in.
    pub fn load_position(&mut self, i: usize) -> Result<(), String> {
        self.refresh()?;
        let a = self.attached.as_ref().unwrap();
        a.gate()?;
        let (world, p) = self.slots.get(i).cloned().flatten().ok_or(tr!("NOTHING_SAVED_IN_THAT_SLOT"))?;
        if a.chain()?.world(&a.game, &a.anchors)? != world {
            return Err(trf!("SAVED_IN_ANOTHER_REGION", world = crate::i18n::place(&world)));
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
            Err(trf!("COULD_NOT_RESTORE", what = failed.join("; ")))
        }
    }

    /// Switch everything off and put back every original on record. Only while the
    /// hero is in play: the record names attributes, and only the hero's are ours.
    pub fn stop(&mut self) -> Result<(), String> {
        self.cut();
        // the game's own pause on losing focus, as the player had it
        if let (Some(at), Some(a)) = (self.unpaused.take(), self.attached.as_ref()) {
            if a.pause_on_focus_lost() == Some(at) {
                a.game.write(at, &[1]);
            }
        }
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
            Err(trf!("COULD_NOT_RESTORE_RUN_HIUMOD_RESTORE", what = failed.join("; ")))
        }
    }
}
