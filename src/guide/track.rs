//! Following several places at once: the auto guide's pick and up to `MAX` places the
//! player chose (a quest goal, a puzzle's groove, a lock, a pin), each with its own route,
//! colour and ring in the game's view. One is in focus: the compass's distance, the
//! tracker's "no way through" and the cycle key are about it. The Guide page keeps the
//! list (.spec/GUIDE.md §"Following several places").

use crate::goals::{Gate, Goal, Tier};
use crate::minimap::{MapState, Point};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// At most this many places followed by hand: as many colours as can be told apart on the
/// maps, and routes worked out in turn.
pub const MAX: usize = 5;

/// The places' colours, by `Track::colour`: none a goal tier's (the auto guide's pick keeps
/// its tier's colour).
pub const COLOURS: [[u8; 3]; MAX] = [[90, 200, 255], [255, 120, 200], [140, 230, 120], [255, 165, 70], [185, 145, 255]];

/// A goal followed whose goal is gone this long is let go: taken, talked to, done. Not at
/// once: a goal changes hands as its actor loads (the survey's place, then the live one).
const GONE: Duration = Duration::from_secs(3);

/// A goal this close (cm) to where a place followed stands is that place, under the id it
/// has now.
const SAME_PLACE: f32 = 200.0;

#[derive(Clone, Debug, PartialEq)]
pub struct Track {
    /// The goal followed: a live actor's id, a survey place's, a pin's, or the place's own.
    pub id: u64,
    /// The survey's world (`Survey::world_of`).
    pub world: String,
    pub at: Point,
    pub label: String,
    /// A place that is no goal (a groove, a lock, a vault's door): shown as a goal of its own
    /// and kept until done or let go. A goal is let go with the goal.
    pub place: bool,
    /// Its colour: an index into `COLOURS`.
    pub colour: u8,
}

impl Track {
    /// The id a place gets when it is no goal, or read back from the file: from its world
    /// and where it is (bit 61, apart from the survey's 63 and the pins' 62).
    pub fn place_id(world: &str, at: Point) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        (world, at.map(|v| v.round() as i32)).hash(&mut h);
        (h.finish() & !(7 << 61)) | 1 << 61
    }

    pub fn rgb(&self) -> [u8; 3] {
        COLOURS[self.colour as usize % MAX]
    }
}

/// One thing followed, as the maps, the compass and the rings draw it.
#[derive(Clone, Debug, PartialEq)]
pub struct Followed {
    pub id: u64,
    /// `None`: the auto guide's pick, in its tier's colour.
    pub colour: Option<[u8; 3]>,
    pub focus: bool,
}

impl MapState {
    /// What is in focus: a place followed, or else the auto guide's pick.
    pub fn focused(&self) -> Option<u64> {
        self.focus.filter(|f| self.tracks.iter().any(|t| t.id == *f)).or(self.auto)
    }

    /// Everything followed: the auto guide's pick (unless it is a place followed too), then
    /// the places, oldest first.
    pub fn followed(&self) -> Vec<Followed> {
        let focus = self.focused();
        let auto = self.auto.filter(|a| !self.tracks.iter().any(|t| t.id == *a));
        auto.map(|id| Followed { id, colour: None, focus: focus == Some(id) })
            .into_iter()
            .chain(self.tracks.iter().map(|t| Followed { id: t.id, colour: Some(t.rgb()), focus: focus == Some(t.id) }))
            .collect()
    }

    pub fn is_followed(&self, id: u64) -> bool {
        self.auto == Some(id) || self.tracks.iter().any(|t| t.id == id)
    }

    /// Follow `t`; already followed (the same goal, or the same place), bring it into
    /// focus; already in focus, let it go. The oldest goes past `MAX`.
    pub fn follow(&mut self, t: Track) {
        let same = |x: &Track| x.id == t.id || x.world == t.world && flat(x.at, t.at) < SAME_PLACE;
        if let Some(i) = self.tracks.iter().position(same) {
            let id = self.tracks[i].id;
            if self.focused() == Some(id) {
                self.unfollow(id);
            } else {
                self.focus = Some(id);
            }
            return;
        }
        if self.tracks.len() >= MAX {
            self.tracks.remove(0);
        }
        let free = (0..MAX as u8).find(|c| !self.tracks.iter().any(|x| x.colour == *c)).unwrap_or(0);
        self.focus = Some(t.id);
        self.tracks.push(Track { colour: free, ..t });
        self.route = true;
        self.dirty = true;
    }

    pub fn unfollow(&mut self, id: u64) {
        self.tracks.retain(|t| t.id != id);
        if self.focus == Some(id) {
            self.focus = self.tracks.last().map(|t| t.id);
        }
        self.dirty = true;
    }

    pub fn unfollow_all(&mut self) {
        self.tracks.clear();
        self.focus = None;
        self.dirty = true;
    }

    /// The cycle key with two or more followed: the next in focus. Whether it did.
    pub fn cycle_focus(&mut self) -> bool {
        let all = self.followed();
        if all.len() < 2 {
            return false;
        }
        let i = all.iter().position(|f| f.focus).map_or(0, |i| (i + 1) % all.len());
        // The auto guide's pick in focus is no focus at all.
        self.focus = Some(all[i].id).filter(|id| self.tracks.iter().any(|t| t.id == *id));
        true
    }

    /// Each frame, with the goals of now in `world`: a place followed takes the id of the
    /// goal standing there now (its actor loaded or unloaded: the same place under another
    /// id), and a goal followed whose goal has been gone `GONE` is let go. `missing` keeps
    /// since when, between frames.
    pub fn relink(&mut self, goals: &[Goal], world: &str, missing: &mut HashMap<u64, Instant>) {
        let world = crate::survey::Survey::world_of(world);
        let mut gone = Vec::new();
        for t in self.tracks.iter_mut().filter(|t| t.world == world) {
            if goals.iter().any(|g| g.id == t.id) {
                missing.remove(&t.id);
                continue;
            }
            let here = goals
                .iter()
                .filter(|g| flat(g.at, t.at) < SAME_PLACE)
                .min_by(|a, b| flat(a.at, t.at).total_cmp(&flat(b.at, t.at)));
            match here {
                Some(g) => {
                    if self.focus == Some(t.id) {
                        self.focus = Some(g.id);
                    }
                    missing.remove(&t.id);
                    t.id = g.id;
                }
                None if !t.place && missing.entry(t.id).or_insert_with(Instant::now).elapsed() >= GONE => {
                    gone.push(t.id);
                }
                None => {}
            }
        }
        for id in gone {
            missing.remove(&id);
            self.unfollow(id);
        }
    }

    /// The places followed in `world` (no goal: a groove, a lock), as goals: what the maps,
    /// the compass and the route take them for. A goal followed is only ever its goal: when
    /// that is gone, so is it.
    pub fn place_goals(&self, goals: &[Goal], world: &str) -> Vec<Goal> {
        let world = crate::survey::Survey::world_of(world);
        self.tracks
            .iter()
            .filter(|t| t.place && t.world == world && !goals.iter().any(|g| g.id == t.id))
            .map(|t| Goal {
                tier: Tier::Clue,
                id: t.id,
                label: t.label.clone(),
                detail: String::new(),
                at: t.at,
                quests: vec![],
                tags: vec![],
                keys: vec![],
                gate: Gate::Open,
                named: true,
            })
            .collect()
    }

    /// Let go of the places followed at `at` (within 1 m): a choice puzzle's groove, once
    /// its set is done.
    pub fn done_at(&mut self, world: &str, at: Point) {
        let world = crate::survey::Survey::world_of(world);
        let done: Vec<u64> = self
            .tracks
            .iter()
            .filter(|t| t.place && t.world == world && flat(t.at, at) < 100.0)
            .map(|t| t.id)
            .collect();
        for id in done {
            self.unfollow(id);
        }
    }

    /// The `track` lines of `minimap.txt`, and which is in focus.
    pub fn render_tracks(&self) -> String {
        let mut out = String::new();
        for t in &self.tracks {
            let label = t.label.replace(['\n', '\r'], " ");
            out += &format!(
                "track {} {} {} {} {} {} {label}\n",
                t.world, t.at[0], t.at[1], t.at[2], t.place as u8, t.colour
            );
        }
        if let Some(i) = self.focus.and_then(|f| self.tracks.iter().position(|t| t.id == f)) {
            out += &format!("track_focus {i}\n");
        }
        out
    }

    /// A `track` line read back: under its place's id until `relink` finds its goal.
    pub fn parse_track(&mut self, f: &[&str]) {
        let [world, x, y, z, place, colour, label @ ..] = f else { return };
        let (Ok(x), Ok(y), Ok(z), Ok(colour)) = (x.parse::<f32>(), y.parse::<f32>(), z.parse::<f32>(), colour.parse())
        else {
            return;
        };
        if self.tracks.len() >= MAX || ![x, y, z].iter().all(|v| v.is_finite()) {
            return;
        }
        let at = [x, y, z];
        self.tracks.push(Track {
            id: Track::place_id(world, at),
            world: world.to_string(),
            at,
            label: label.join(" "),
            place: *place == "1",
            colour,
        });
    }
}

fn flat(a: Point, b: Point) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn goal(id: u64, x: f32) -> Goal {
        Goal {
            tier: Tier::Quest,
            id,
            label: String::new(),
            detail: String::new(),
            at: [x, 0.0, 0.0],
            quests: vec![],
            tags: vec![],
            keys: vec![],
            gate: Gate::Open,
            named: true,
        }
    }

    fn track(id: u64, x: f32, place: bool) -> Track {
        Track { id, world: "W".into(), at: [x, 0.0, 0.0], label: format!("t{id}"), place, colour: 0 }
    }

    #[test]
    fn following_adds_focuses_then_lets_go() {
        let mut s = MapState::default();
        s.follow(track(1, 0.0, false));
        s.follow(track(2, 5000.0, true));
        assert_eq!(s.focused(), Some(2));
        assert_ne!(s.tracks[0].colour, s.tracks[1].colour, "each its own colour");
        s.follow(track(1, 0.0, false));
        assert_eq!(s.focused(), Some(1), "followed again: in focus");
        s.follow(track(1, 0.0, false));
        assert_eq!(s.tracks.len(), 1, "in focus and followed again: let go");
        for i in 10..20 {
            s.follow(track(i, i as f32 * 1000.0, true));
        }
        assert_eq!(s.tracks.len(), MAX, "the oldest go");
    }

    #[test]
    fn the_cycle_key_moves_the_focus_through_everything_followed() {
        let mut s = MapState { auto: Some(9), ..MapState::default() };
        assert!(!s.cycle_focus(), "one followed: the key steps the auto guide instead");
        s.follow(track(1, 0.0, false));
        assert_eq!(s.focused(), Some(1));
        assert!(s.cycle_focus());
        assert_eq!(s.focused(), Some(9), "back to the auto guide's pick");
        assert!(s.cycle_focus());
        assert_eq!(s.focused(), Some(1));
    }

    #[test]
    fn a_place_takes_the_goal_standing_there_and_a_gone_goal_is_let_go() {
        let mut s = MapState::default();
        s.follow(track(Track::place_id("W", [100.0, 0.0, 0.0]), 100.0, false));
        s.follow(track(7, 9000.0, true));
        let mut missing = HashMap::new();
        s.relink(&[goal(42, 150.0)], "W_Root_WP", &mut missing);
        assert_eq!(s.tracks[0].id, 42, "read back from the file: its goal found by place");
        s.relink(&[], "W_Root_WP", &mut missing);
        assert_eq!(s.tracks.len(), 2, "not at once");
        *missing.get_mut(&42).unwrap() -= GONE;
        s.relink(&[], "W_Root_WP", &mut missing);
        assert_eq!(s.tracks.iter().map(|t| t.id).collect::<Vec<_>>(), [7], "the goal is gone; the place stays");
        assert_eq!(s.place_goals(&[], "W_Root_WP").len(), 1);
        s.done_at("W", [9050.0, 0.0, 0.0]);
        assert!(s.tracks.is_empty(), "its groove's set done: let go");
    }

    #[test]
    fn tracks_are_kept_by_place_across_runs() {
        let mut s = MapState::default();
        s.follow(Track { label: "Ceramic Flower, groove".into(), ..track(5, 120.0, true) });
        s.follow(track(6, 900.0, false));
        s.follow(track(5, 120.0, true));
        let text = s.render_tracks();
        let mut back = MapState::default();
        for line in text.lines() {
            let f: Vec<&str> = line.split_whitespace().collect();
            match f[..] {
                ["track", ref rest @ ..] => back.parse_track(rest),
                ["track_focus", i] => {
                    back.focus = i.parse::<usize>().ok().and_then(|i| back.tracks.get(i)).map(|t| t.id)
                }
                _ => {}
            }
        }
        assert_eq!(back.tracks.len(), 2);
        assert_eq!(back.tracks[0].label, "Ceramic Flower, groove");
        assert!(back.tracks[0].place && !back.tracks[1].place);
        assert_eq!(back.focused(), Some(back.tracks[0].id));
    }
}
