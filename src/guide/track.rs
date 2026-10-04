//! Following several things at once: the auto guide's pick and up to `MAX` chosen by the
//! player: places (a quest goal, a puzzle's groove, a lock, a pin) and quests, each with its
//! own route, colour and ring in the game's view. A quest followed goes to its own next goal
//! as the auto guide does, and on to the next once one is done. One thing is in focus: the
//! compass's distance, the tracker's "no way through" and the cycle key are about it. The
//! Guide page keeps the list (.spec/GUIDE.md §7).

use crate::goals::{Gate, Goal, Tier};
use crate::minimap::{MapState, Point};
use crate::quests::Quest;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

/// At most this many followed by hand: as many colours as can be told apart on the maps,
/// and routes worked out in turn.
pub const MAX: usize = 5;

/// Their colours, by `Track::colour`: none a goal tier's (the auto guide's pick keeps its
/// tier's colour).
pub const COLOURS: [[u8; 3]; MAX] = [[90, 200, 255], [255, 120, 200], [140, 230, 120], [255, 165, 70], [185, 145, 255]];

/// A goal followed whose goal is gone this long is let go: taken, talked to, done. Not at
/// once: a goal changes hands as its actor loads (the survey's place, then the live one).
const GONE: Duration = Duration::from_secs(3);

/// A goal this close (cm) to where a place followed stands is that place, under the id it
/// has now.
const SAME_PLACE: f32 = 200.0;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Track {
    /// What it is known by: a place's goal (a live actor's id, a survey place's, a pin's, or
    /// the place's own), or a quest's own id (`quest_id`).
    pub id: u64,
    /// The survey's world (`Survey::world_of`); a quest's is any.
    pub world: String,
    pub at: Point,
    /// The place's name, or the quest's.
    pub label: String,
    /// A place that is no goal (a groove, a lock, a vault's door): shown as a goal of its own
    /// and kept until done or let go. A goal is let go with the goal.
    pub place: bool,
    /// Its colour: an index into `COLOURS`.
    pub colour: u8,
    /// A quest followed, by its journal key: its goal is the one picked for it each frame.
    pub quest: Option<String>,
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

    /// A quest's own id (bit 60).
    pub fn quest_id(key: &str) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        ("quest", key).hash(&mut h);
        (h.finish() & !(15 << 60)) | 1 << 60
    }

    /// A quest to follow.
    pub fn quest(key: &str, name: &str) -> Track {
        Track { id: Track::quest_id(key), label: name.to_string(), quest: Some(key.to_string()), ..Default::default() }
    }

    pub fn rgb(&self) -> [u8; 3] {
        COLOURS[self.colour as usize % MAX]
    }

    /// Its name to show: a leading key (`RECORDS`, a collectible's kind, as some pages gave
    /// it before) in words.
    pub fn shown(&self) -> String {
        match self.label.split_once(' ') {
            Some((head, rest)) if head.len() > 1 && head.bytes().all(|b| b.is_ascii_uppercase() || b == b'_') => {
                format!("{} {rest}", crate::i18n::text(head))
            }
            _ => self.label.clone(),
        }
    }
}

/// One thing followed with a goal to go to now, as the maps, the compass and the rings
/// draw it.
#[derive(Clone, Debug, PartialEq)]
pub struct Followed {
    /// The goal.
    pub id: u64,
    /// The track (`None`: the auto guide's pick, in its tier's colour).
    pub track: Option<u64>,
    pub colour: Option<[u8; 3]>,
    pub focus: bool,
}

impl MapState {
    fn focus_track(&self) -> Option<&Track> {
        self.focus.and_then(|f| self.tracks.iter().find(|t| t.id == f))
    }

    /// The goal a track is at now: a place's own, or the one picked for a quest.
    pub fn goal_of(&self, t: &Track) -> Option<u64> {
        match &t.quest {
            Some(_) => self.resolved.get(&t.id).copied(),
            None => Some(t.id),
        }
    }

    /// The goal in focus: a track's, or else the auto guide's pick.
    pub fn focused(&self) -> Option<u64> {
        match self.focus_track() {
            Some(t) => self.goal_of(t),
            None => self.auto,
        }
    }

    /// Whether the track `id` is the one in focus (`None`: the auto guide).
    pub fn in_focus(&self, track: Option<u64>) -> bool {
        self.focus_track().map(|t| t.id) == track
    }

    /// Everything followed that has a goal now: the auto guide's pick, then the tracks,
    /// oldest first; a goal once.
    pub fn followed(&self) -> Vec<Followed> {
        let mut out: Vec<Followed> = Vec::new();
        let focus = self.focus_track().map(|t| t.id);
        if let Some(a) = self.auto {
            out.push(Followed { id: a, track: None, colour: None, focus: focus.is_none() });
        }
        for t in &self.tracks {
            let Some(goal) = self.goal_of(t) else { continue };
            let f = Followed { id: goal, track: Some(t.id), colour: Some(t.rgb()), focus: focus == Some(t.id) };
            // A goal followed twice (the auto guide's and a track's): drawn as the track.
            match out.iter_mut().find(|x| x.id == goal) {
                Some(x) if x.track.is_none() => *x = Followed { focus: x.focus || f.focus, ..f },
                Some(_) => {}
                None => out.push(f),
            }
        }
        out
    }

    /// Whether a goal is followed, by anything.
    pub fn is_followed(&self, goal: u64) -> bool {
        self.auto == Some(goal) || self.tracks.iter().any(|t| self.goal_of(t) == Some(goal))
    }

    /// The track following a quest, if one does.
    pub fn quest_track(&self, key: &str) -> Option<&Track> {
        self.tracks.iter().find(|t| t.quest.as_deref() == Some(key))
    }

    /// The track that is `t` already: the same goal, the same place, the same quest.
    fn same_as(&self, t: &Track) -> Option<u64> {
        let same = |x: &&Track| match (&x.quest, &t.quest) {
            (Some(a), Some(b)) => a == b,
            (None, None) => x.id == t.id || x.world == t.world && flat(x.at, t.at) < SAME_PLACE,
            _ => false,
        };
        self.tracks.iter().find(same).map(|x| x.id)
    }

    /// The Follow button: follow `t` (in focus), or, followed already, let it go.
    pub fn toggle(&mut self, t: Track) {
        match self.same_as(&t) {
            Some(id) => self.unfollow(id),
            None => self.follow(t),
        }
    }

    /// Follow `t` and bring it into focus, whether followed already or not; a place
    /// followed already moves to where `t` is (a puzzle's set, then its right groove).
    pub fn ensure(&mut self, t: Track) {
        match self.same_as(&t) {
            Some(id) => {
                if let Some(x) = self.tracks.iter_mut().find(|x| x.id == id && x.place) {
                    x.at = t.at;
                    x.label = t.label;
                    self.dirty = true;
                }
                self.focus = Some(id);
            }
            None => self.follow(t),
        }
    }

    /// The colour of the track following `goal` (or the place `goal`), if one does: what a
    /// Follow button shows.
    pub fn track_colour(&self, goal: u64) -> Option<[u8; 3]> {
        self.tracks.iter().find(|t| t.id == goal || self.goal_of(t) == Some(goal)).map(Track::rgb)
    }

    /// The quest in focus, by its key: the panel's needs, clues and the quest tracker are
    /// about it; `None`, the main story.
    pub fn focused_quest(&self) -> Option<&str> {
        self.focus_track().and_then(|t| t.quest.as_deref())
    }

    /// Follow `t`; already followed (the same goal, the same place, the same quest), bring
    /// it into focus; already in focus, let it go. The oldest goes past `MAX`.
    pub fn follow(&mut self, t: Track) {
        if let Some(id) = self.same_as(&t) {
            if self.focus == Some(id) {
                self.unfollow(id);
            } else {
                self.focus = Some(id);
            }
            return;
        }
        if self.tracks.len() >= MAX {
            let oldest = self.tracks[0].id;
            self.unfollow(oldest);
        }
        let free = (0..MAX as u8).find(|c| !self.tracks.iter().any(|x| x.colour == *c)).unwrap_or(0);
        self.focus = Some(t.id);
        self.tracks.push(Track { colour: free, ..t });
        self.route = true;
        self.dirty = true;
    }

    pub fn unfollow(&mut self, id: u64) {
        self.tracks.retain(|t| t.id != id);
        self.resolved.remove(&id);
        if self.focus == Some(id) {
            self.focus = self.tracks.last().map(|t| t.id);
        }
        self.dirty = true;
    }

    /// Let go of a track that ended by itself (reached, done), noting it for the journey.
    fn ended(&mut self, id: u64) {
        if let Some(t) = self.tracks.iter().find(|t| t.id == id) {
            let label = t.shown();
            self.done.push((label, std::time::SystemTime::now()));
            if self.done.len() > 20 {
                self.done.remove(0);
            }
        }
        self.unfollow(id);
    }

    pub fn unfollow_all(&mut self) {
        self.tracks.clear();
        self.resolved.clear();
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
        self.focus = all[i].track;
        true
    }

    /// Each frame: each quest followed goes to its own next goal, as the auto guide does
    /// for the story (target.rs `next_goal`); done or failed, it is let go. A quest with
    /// nothing of it here has no goal for now.
    pub fn resolve_quests(&mut self, goals: &[Goal], journal: &[Quest], here: Point, blocked: &HashSet<u64>) {
        use crate::quests::Status;
        let mut done = Vec::new();
        // A quest's name as the journal has it now (one read back from an older file has
        // its key only).
        for t in self.tracks.iter_mut().filter(|t| t.quest.is_some()) {
            if let Some(q) = journal.iter().find(|q| Some(&q.key) == t.quest.as_ref()).filter(|q| q.name != t.label) {
                t.label = q.name.clone();
            }
        }
        for t in self.tracks.iter().filter(|t| t.quest.is_some()) {
            let Some(q) = journal.iter().find(|q| Some(&q.key) == t.quest.as_ref()) else { continue };
            if matches!(q.status, Status::Completed | Status::Failed) {
                done.push(t.id);
                continue;
            }
            let now = self.resolved.get(&t.id).copied();
            match crate::guide::target::next_goal(goals, here, Some(q), journal, blocked, &self.skipped, now) {
                Some(g) => self.resolved.insert(t.id, g),
                None => self.resolved.remove(&t.id),
            };
        }
        for id in done {
            self.ended(id);
        }
    }

    /// Each frame, with the goals of now in `world`: a place followed takes the id of the
    /// goal standing there now (its actor loaded or unloaded: the same place under another
    /// id), and a goal followed whose goal has been gone `GONE` is let go. `missing` keeps
    /// since when, between frames.
    pub fn relink(&mut self, goals: &[Goal], world: &str, missing: &mut HashMap<u64, Instant>) {
        let world = crate::survey::Survey::world_of(world);
        let mut gone = Vec::new();
        for t in self.tracks.iter_mut().filter(|t| t.quest.is_none() && t.world == world) {
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
            self.ended(id);
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
                label: t.shown(),
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
            self.ended(id);
        }
    }

    /// The `track` and `track_quest` lines of `minimap.txt`, and which is in focus.
    pub fn render_tracks(&self) -> String {
        let mut out = String::new();
        for t in &self.tracks {
            let label = t.label.replace(['\n', '\r'], " ");
            out += &match &t.quest {
                Some(key) => format!("track_quest {} {key} {label}\n", t.colour),
                None => format!(
                    "track {} {} {} {} {} {} {label}\n",
                    t.world, t.at[0], t.at[1], t.at[2], t.place as u8, t.colour
                ),
            };
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
            quest: None,
        });
    }

    /// A `track_quest` line read back.
    pub fn parse_quest_track(&mut self, f: &[&str]) {
        let [colour, key, label @ ..] = f else { return };
        let Ok(colour) = colour.parse() else { return };
        if self.tracks.len() < MAX && self.quest_track(key).is_none() {
            self.tracks.push(Track { colour, ..Track::quest(key, &label.join(" ")) });
        }
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
        Track { id, world: "W".into(), at: [x, 0.0, 0.0], label: format!("t{id}"), place, ..Default::default() }
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
        assert_eq!(s.done.len(), 2, "both noted for the journey");
    }

    #[test]
    fn a_quest_followed_goes_to_its_own_next_goal_until_done() {
        use crate::quests::{Kind, Status};
        let deed = |status| Quest {
            key: "d".into(),
            kind: Kind::GoodDeed,
            name: "Watch".into(),
            detail: String::new(),
            status,
            progress: None,
            leads: vec![],
            quest: None,
            tags: Some("Secrets.Facts.GoldenWatch".into()),
        };
        let mut hand_over = goal(4, 3000.0);
        hand_over.tier = Tier::Secret;
        hand_over.tags = vec!["Secrets.Facts.GoldenWatchCompleted".into()];
        let goals = [goal(3, 900.0), hand_over];
        let mut s = MapState { auto: Some(3), ..MapState::default() };
        s.follow(Track::quest("d", "Watch"));
        assert_eq!(s.followed().len(), 1, "no goal for it yet: only the auto guide's");
        let started = [deed(Status::Started)];
        s.resolve_quests(&goals, &started, [0.0; 3], &Default::default());
        assert_eq!(s.focused(), Some(4), "its hand-over, in focus");
        assert_eq!(s.followed().iter().map(|f| f.id).collect::<Vec<_>>(), [3, 4], "the story's and the deed's");
        assert!(s.is_followed(4));
        s.resolve_quests(&goals, &[deed(Status::Completed)], [0.0; 3], &Default::default());
        assert!(s.tracks.is_empty(), "done: let go");
    }

    #[test]
    fn tracks_are_kept_across_runs() {
        let mut s = MapState::default();
        s.follow(Track { label: "Ceramic Flower, groove".into(), ..track(5, 120.0, true) });
        s.follow(track(6, 900.0, false));
        s.follow(Track::quest("Quest02", "Family Legacy"));
        s.follow(track(5, 120.0, true));
        let text = s.render_tracks();
        let mut back = MapState::default();
        for line in text.lines() {
            let f: Vec<&str> = line.split_whitespace().collect();
            match f[..] {
                ["track", ref rest @ ..] => back.parse_track(rest),
                ["track_quest", ref rest @ ..] => back.parse_quest_track(rest),
                ["track_focus", i] => {
                    back.focus = i.parse::<usize>().ok().and_then(|i| back.tracks.get(i)).map(|t| t.id)
                }
                _ => {}
            }
        }
        assert_eq!(back.tracks.len(), 3);
        assert_eq!(back.tracks[0].label, "Ceramic Flower, groove");
        assert!(back.tracks[0].place && !back.tracks[1].place);
        assert_eq!(back.quest_track("Quest02").map(|t| t.label.as_str()), Some("Family Legacy"));
        assert_eq!(back.focused(), Some(back.tracks[0].id));
    }
}
