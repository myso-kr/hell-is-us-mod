//! The loop every front end drives: attach to the game, watch the hero gate, hold
//! the active toggles, read the values. The CLI's `hold` and the overlay are both a
//! thin layer over `Engine::step`.

use crate::actors::{Scanner, Thing};
use crate::anchors::{self, Anchors};
use crate::attr::{Attr, Session};
use crate::cheats::{self, Active};
use crate::game::locate;
use crate::game::process::Game;
use crate::geometry::{Footprint, Geometry};
use crate::hold::{self, Originals};
use crate::player::{self, Chain};
use std::cell::RefCell;
use std::sync::Arc;
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
        geometry: RefCell::default(),
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
        Ok(s)
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
}

impl Snapshot {
    pub fn value(&self, a: Attr) -> Option<f32> {
        self.values.iter().find(|(x, _)| *x == a).and_then(|(_, v)| *v)
    }
}

pub struct Engine {
    attached: Option<Attached>,
    checked: Option<Instant>,
    originals: Originals,
    active: Vec<Active>,
    notice: Option<String>,
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
        };
        if let Err(e) = self.refresh() {
            snap.game = Err(e.clone());
            snap.gate = Err(e);
        } else {
            let a = self.attached.as_ref().unwrap();
            snap.game = Ok((a.game.pid, a.version.clone()));
            snap.gate = a.gate();
            snap.pose = a.pose().ok();
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
            }
        }
        snap.active = self.active.clone();
        snap.pending = self.originals.len();
        snap.originals = self.originals.entries();
        snap.notice = self.notice.take().or(snap.notice);
        snap
    }

    pub fn set(&mut self, name: &str, v: f32) -> Result<(), String> {
        let a = self.attached()?;
        a.gate()?;
        cheats::set_value(&a.session()?, name, v)
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
        if drop.is_empty() {
            return Ok(());
        }
        let a = self.attached.as_ref().unwrap();
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
