//! The "now" page (.spec/JOURNEY.md §3): one screen that answers "what now?" — where the
//! last session left off, what is about to be missed, what is left in this region, and
//! where else a trip pays. Counts and pointers only; the details stay on their own pages.

use super::super::theme::INLINE;
use super::*;
use crate::actors::Sub;
use eframe::egui::Color32;

impl Panel {
    /// The cards under the hero: five, whatever there is (each says when it is empty).
    pub(super) fn now_cards(&self) -> usize {
        5
    }

    /// The page, after the pains players name most (JOURNEY.md §1): the story followed
    /// and where it was left (the Datapad buries the clue; a break loses the thread),
    /// what can be done right here (whom an item goes to), the side stories under way
    /// (forgotten), where a trip pays (no fast travel), what is about to be missed. The
    /// story is two columns wide; the rest one each, in rows of the page's columns.
    pub(super) fn now_tab(&mut self, t: &mut Tui, state: &mut crate::minimap::MapState, snap: Option<&Snapshot>) {
        use egui_taffy::taffy::{AlignItems, Style};
        self.hero(t, snap);
        let cols = self.columns.clamp(1, 3);
        // Cards in a row as tall as each other: no hole beside a short one.
        let grid = Style { align_items: Some(AlignItems::Stretch), ..tw::grid(cols, tw::GAP) };
        t.style(grid).add(|t| {
            tw::span(t, cols.min(2) as u16, |t| self.story_card(t, state, snap));
            tw::span(t, 1, |t| self.doable_card(t, state, snap));
            tw::span(t, 1, |t| self.side_card(t, snap));
            tw::span(t, 1, |t| self.regions_card(t, snap));
            tw::span(t, 1, |t| self.before_card(t, state, snap));
        });
    }

    /// The page's hero (§3.1): a live map round the hero (the overlay draws it), the
    /// region in large type, what can be done here now, and a tile per kind of thing
    /// left — each opens the page that has the places.
    fn hero(&mut self, t: &mut Tui, snap: Option<&Snapshot>) {
        let here = snap.and_then(|s| s.world.clone()).map(|w| crate::survey::Survey::world_of(&w).to_string());
        let rows = snap.map(ledger).unwrap_or_default();
        let row = here.as_deref().and_then(|h| rows.iter().find(|r| r.world == h)).cloned().unwrap_or_default();
        // The overlay's newest map, as a texture once per new frame.
        let fresh = self.shared.hero.lock().unwrap().as_ref().map(|h| h.2);
        if fresh.is_some() && fresh != self.hero_tex.as_ref().map(|h| h.0) {
            if let Some((side, px, n)) = self.shared.hero.lock().unwrap().clone() {
                let bytes: Vec<u8> =
                    px.iter().flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8, (p >> 24) as u8]).collect();
                let image = egui::ColorImage::from_rgba_premultiplied([side, side], &bytes);
                match self.hero_tex.as_mut() {
                    Some((at, tex)) => {
                        tex.set(image, egui::TextureOptions::LINEAR);
                        *at = n;
                    }
                    None => {
                        let tex = t.egui_ctx().load_texture("now-hero", image, egui::TextureOptions::LINEAR);
                        self.hero_tex = Some((n, tex));
                    }
                }
            }
        }
        let tex = self.hero_tex.as_ref().map(|h| h.1.id());
        let mut open = None;
        tw::hero(t, |t| {
            w(t, |ui| match tex {
                Some(id) => {
                    ui.add(egui::Image::new((id, egui::vec2(HERO_MAP, HERO_MAP))));
                }
                None => {
                    let (r, _) = ui.allocate_exact_size(egui::vec2(HERO_MAP, HERO_MAP), egui::Sense::hover());
                    ui.painter().circle_filled(r.center(), HERO_MAP / 2.0 - 8.0, super::super::theme::CONTROL);
                }
            });
            t.style(tw::grow(tw::col(INLINE))).add(|t| {
                let title = here.as_deref().map_or(tr!("WAITING_FOR_THE_GAME").to_string(), crate::i18n::place);
                w(t, |ui| ui.label(RichText::new(title).size(22.0).strong().color(super::super::theme::TITLE)));
                if here.is_none() {
                    return;
                }
                if row.total() == 0 {
                    note(t, tr!("NOTHING_LEFT_HERE_THE_MOD_KNOWS_OF"));
                    return;
                }
                t.style(tw::row(INLINE)).add(|t| {
                    let tone = if row.doable() > 0 { tw::Tone::Accent } else { tw::Tone::Quiet };
                    tw::chip(t, trf!("HERO_DOABLE", count = row.doable()), tone);
                    w(t, |ui| ui.label(RichText::new(trf!("HERO_LEFT", count = row.total())).small().color(DIM)));
                });
                let ok = super::super::theme::OK;
                let locks = if row.locks > 0 { format!("{}/{}", row.openable, row.locks) } else { "0".to_string() };
                let tiles: [(Sub, String, &str, Option<Color32>, Tool); 6] = [
                    (Sub::Quest, row.quest.to_string(), tr!("STAT_QUEST_PLACES"), None, Tool::Quests),
                    (Sub::Npc, row.handovers.to_string(), tr!("STAT_HANDOVERS"), Some(ok), Tool::Quests),
                    (Sub::LymbicLock, locks, tr!("STAT_LOCKS_OPEN"), (row.openable > 0).then_some(ok), Tool::Puzzles),
                    (Sub::Puzzle, row.puzzles.to_string(), tr!("PUZZLES"), None, Tool::Puzzles),
                    (Sub::Stash, row.collect.to_string(), tr!("STAT_COLLECTIBLES"), None, Tool::Collect),
                    (Sub::EnemyGroup, row.groups.to_string(), tr!("STAT_ENEMY_GROUPS"), None, Tool::Collect),
                ];
                t.style(tw::grid(3, INLINE)).add(|t| {
                    for (sort, value, label, lit, page) in tiles {
                        let icon = |ui: &mut egui::Ui| {
                            crate::ui::svg::sort(ui, sort, 14.0);
                        };
                        if tw::stat(t, icon, &value, label, lit) {
                            open = Some(page);
                        }
                    }
                });
            });
        });
        // The page a tile opens, if the player agreed to it (Settings); the Now page itself
        // shows whatever the consent.
        if let Some(page) = open.filter(|p| self.allowed(*p)) {
            self.tool = Some(page);
        }
    }

    /// The story followed (§3.2, §3.8): where the last session left off when it was a
    /// while ago, the quest, how far, its next goal and what was last learned on each of
    /// its leads — so the Datapad need not be searched for the thread.
    fn story_card(&mut self, t: &mut Tui, state: &mut crate::minimap::MapState, snap: Option<&Snapshot>) {
        let journal = snap.map(|s| s.journal.clone()).unwrap_or_default();
        let here = snap.and_then(|s| s.pose).map(|(p, _)| [p[0] as f32, p[1] as f32, p[2] as f32]);
        let goals = snap.map(|s| s.goals.clone()).unwrap_or_default();
        card(t, tr!("STORY_FOLLOWED"), |t| {
            if let Some(prev) = self.previous.clone() {
                let gap = crate::session::now().saturating_sub(prev.when);
                if gap >= PREVIOUSLY_AFTER {
                    let line = trf!("LAST_TIME_LINE", ago = ago(gap), place = crate::i18n::place(&prev.world));
                    text(t, RichText::new(line).small().color(super::super::theme::WAIT));
                }
            }
            let Some(q) = crate::quests::followed(&journal, state.quest.as_deref()).cloned() else {
                note(t, tr!("NO_QUEST_WAS_FOLLOWED"));
                return;
            };
            t.style(tw::row(INLINE)).add(|t| {
                w(t, |ui| crate::ui::svg::quest(ui, q.kind, 20.0));
                w(t, |ui| ui.label(RichText::new(&q.name).size(15.0).strong().color(super::super::theme::TITLE)));
                if let Some((done, all)) = q.progress {
                    w(t, |ui| tw::meter(ui, Some(80.0), done, all));
                    w(t, |ui| ui.label(RichText::new(format!("{done}/{all}")).monospace().small().color(DIM)));
                }
            });
            // The guide's goal now, and how far.
            let goal = state.focused().and_then(|id| goals.iter().find(|g| g.id == id));
            match (goal, here) {
                (Some(g), Some(h)) => {
                    let far = crate::raster::span(h, g.at);
                    text(t, trf!("NEXT_GOAL_LINE", goal = g.label, far = far));
                }
                _ => note(t, tr!("NO_GOAL_YET")),
            }
            for (subject, last) in q.leads.iter().take(4) {
                let line = match last {
                    Some(l) => format!("· {subject}: {l}"),
                    None => format!("· {subject}"),
                };
                note(t, line);
            }
            // The quests followed besides (guide/track.rs), each in its colour with its next
            // goal: press one to bring it into focus.
            let others: Vec<crate::guide::track::Track> = state
                .tracks
                .iter()
                .filter(|x| x.quest.is_some() && x.quest.as_ref() != Some(&q.key))
                .cloned()
                .collect();
            if !others.is_empty() {
                block(t, |ui| tw::group_heading(ui, tr!("ALSO_FOLLOWED"), others.len()));
                for x in &others {
                    let goal = state.goal_of(x).and_then(|id| goals.iter().find(|g| g.id == id));
                    let line = match (goal, here) {
                        (Some(g), Some(h)) => format!("{} → {} ({})", x.label, g.label, crate::raster::span(h, g.at)),
                        _ => format!("{} · {}", x.label, tr!("QUEST_NOT_HERE")),
                    };
                    let [r, g, b] = x.rgb();
                    let on = state.in_focus(Some(x.id));
                    let icon = |ui: &mut egui::Ui| {
                        ui.label(RichText::new("●").color(egui::Color32::from_rgb(r, g, b)));
                    };
                    if tw::line(t, Some(on), icon, line, |_| {}) {
                        state.focus = Some(x.id);
                    }
                }
            }
            if w(t, |ui| ui.button(trf!("GO_TO_PAGE", page = tr!("QUESTS")))).clicked() {
                self.tool = Some(Tool::Quests);
            }
        });
    }

    /// What can be done right here, now (§3.1): the items held that someone here wants,
    /// the Lymbic locks the rods held open, and the nearest places to go. Press one to be
    /// guided there.
    fn doable_card(&mut self, t: &mut Tui, state: &mut crate::minimap::MapState, snap: Option<&Snapshot>) {
        let Some(s) = snap else { return };
        let world = s.world.clone().map(|w| crate::survey::Survey::world_of(&w).to_string()).unwrap_or_default();
        let here = s.pose.map(|(p, _)| [p[0] as f32, p[1] as f32, p[2] as f32]);
        let far = |at: [f32; 3]| here.map_or(String::new(), |h| crate::raster::span(h, at));
        let dist = |at: [f32; 3]| here.map_or(0.0, |h| (at[0] - h[0]).hypot(at[1] - h[1]));
        let goals = s.goals.clone();
        card(t, tr!("DOABLE_NOW"), |t| {
            let mut any = false;
            let mut gives: Vec<&crate::survey::Need> =
                s.handovers.iter().filter(|n| n.world == world && !n.done).collect();
            gives.sort_by(|a, b| dist(a.at).total_cmp(&dist(b.at)));
            for n in gives.iter().take(3) {
                any = true;
                // The person's name without the talk goal's "Talk: " before it.
                let talk = trf!("TALK_GOAL", npc = "");
                let who = n.label.strip_prefix(talk.as_str()).unwrap_or(&n.label);
                let line = trf!("HAND_OVER_LINE", what = n.what, who = who, far = far(n.at));
                if tw::pick(t, state.is_followed(n.id), line) {
                    guide_to(state, &goals, n);
                }
            }
            let mut opens: Vec<&crate::survey::Lock> =
                s.locks.iter().filter(|l| l.world == world && !l.solved && l.rods.iter().all(|r| r.held)).collect();
            opens.sort_by(|a, b| dist(a.at).total_cmp(&dist(b.at)));
            for l in opens.iter().take(2) {
                any = true;
                if tw::pick(t, state.is_followed(l.id), trf!("LOCK_OPENS_NOW_LINE", far = far(l.at))) {
                    let x = crate::survey::Need {
                        world: l.world.clone(),
                        id: l.id,
                        label: String::new(),
                        what: String::new(),
                        at: l.at,
                        done: false,
                    };
                    guide_to(state, &goals, &x);
                }
            }
            let mut near: Vec<&crate::goals::Goal> = goals.iter().collect();
            near.sort_by(|a, b| a.tier.cmp(&b.tier).then(dist(a.at).total_cmp(&dist(b.at))));
            for g in near.iter().take(3) {
                any = true;
                if tw::pick(t, state.is_followed(g.id), format!("{} ({})", g.label, far(g.at))) {
                    follow_goal(state, g, &world);
                }
            }
            if !any {
                note(t, tr!("NOTHING_TO_DO_HERE_NOW"));
            }
        });
    }

    /// The good deeds, mysteries and timeloops begun and not finished (side stories are
    /// forgotten, JOURNEY.md §1): this region's first.
    fn side_card(&mut self, t: &mut Tui, snap: Option<&Snapshot>) {
        use crate::quests::{Kind, Status};
        let journal = snap.map(|s| s.journal.clone()).unwrap_or_default();
        let place = snap.and_then(|s| s.world.clone()).map(|w| crate::i18n::place(crate::survey::Survey::world_of(&w)));
        let mut open: Vec<&crate::quests::Quest> = journal
            .iter()
            .filter(|q| Kind::SECRETS.iter().any(|(k, _)| *k == q.kind) && q.status == Status::Started)
            .collect();
        open.sort_by_key(|q| place.as_ref().is_none_or(|p| !q.detail.contains(p.as_str())));
        card(t, &trf!("SIDE_IN_PROGRESS", count = open.len()), |t| {
            if open.is_empty() {
                note(t, tr!("NO_SIDE_IN_PROGRESS"));
                return;
            }
            for q in open.iter().take(6) {
                tw::item(t, |t| {
                    t.style(tw::row(INLINE)).add(|t| {
                        w(t, |ui| crate::ui::svg::quest(ui, q.kind, 16.0));
                        text(t, q.name.clone());
                    });
                    if !q.detail.is_empty() {
                        note(t, q.detail.clone());
                    }
                });
            }
            if open.len() > 6 {
                note(t, trf!("MORE_N", n = open.len() - 6));
            }
        });
    }

    /// What is about to be missed (§3.5): the good deeds whose story point can come any
    /// time now, and the act 2 keystone order.
    fn before_card(&mut self, t: &mut Tui, state: &mut crate::minimap::MapState, snap: Option<&Snapshot>) {
        use crate::missables::When;
        let list = snap.map(|s| s.deadlines.clone()).unwrap_or_default();
        let journal = snap.map(|s| s.journal.clone()).unwrap_or_default();
        card(t, tr!("BEFORE_YOU_GO_ON"), |t| {
            let soon: Vec<_> = list.iter().filter(|d| d.when == When::Now).collect();
            if soon.is_empty() {
                note(t, tr!("NOTHING_IS_ABOUT_TO_BE_MISSED"));
            }
            for d in &soon {
                tw::item(t, |t| {
                    t.style(tw::row(INLINE)).add(|t| {
                        tw::chip(t, tr!("SOON"), tw::Tone::Bad);
                        let label = format!("{}{}", d.title, if d.started { "" } else { tr!("NOT_STARTED") });
                        if tw::pick(t, state.quest.as_deref() == Some(d.key.as_str()), label) && d.started {
                            state.quest = Some(d.key.clone());
                            state.auto = None;
                            state.held = false;
                            state.guide_auto = true;
                            state.route = true;
                        }
                    });
                    note(t, format!("{}: {}", d.due.label(), d.what));
                });
            }
            if let Some((left, before)) = crate::missables::keystone_advice(&journal, &list) {
                note(t, trf!("KEYSTONES_LEFT_IN_ORDER", keystones = left.join(" → ")));
                if !before.is_empty() {
                    note(t, trf!("BEFORE_THE_NEXT_KEYSTONE", deeds = before.join(", ")));
                }
            }
            let later = list.iter().filter(|d| d.when == When::Later).count();
            if later > 0 {
                note(t, trf!("MORE_WITH_A_LATER_DEADLINE", count = later));
            }
            if w(t, |ui| ui.button(trf!("GO_TO_PAGE", page = tr!("QUESTS")))).clicked() {
                self.tool = Some(Tool::Quests);
            }
        });
    }

    /// Where a trip pays (§3.4; there is no fast travel but the APC): the three other
    /// regions with the most to do now, and what — hand-overs, locks the rods open, what
    /// is left — then the rest as a line.
    fn regions_card(&mut self, t: &mut Tui, snap: Option<&Snapshot>) {
        let here = snap.and_then(|s| s.world.clone()).map(|w| crate::survey::Survey::world_of(&w).to_string());
        let rows = crate::ledger::by_trip(snap.map(ledger).unwrap_or_default());
        card(t, tr!("TRIPS_THAT_PAY"), |t| {
            let others: Vec<&crate::ledger::Region> =
                rows.iter().filter(|r| Some(r.world.as_str()) != here.as_deref() && r.total() > 0).collect();
            if others.is_empty() {
                note(t, tr!("NOTHING_LEFT_ELSEWHERE"));
                return;
            }
            for r in others.iter().take(3) {
                tw::item(t, |t| {
                    t.style(tw::row(INLINE)).add(|t| {
                        text(t, RichText::new(crate::i18n::place(&r.world)).color(super::super::theme::TEXT));
                        if r.doable() > 0 {
                            tw::chip(t, trf!("HERO_DOABLE", count = r.doable()), tw::Tone::Accent);
                        }
                    });
                    note(t, trf!("TRIP_LINE", handovers = r.handovers, locks = r.openable, left = r.total()));
                });
            }
            if others.len() > 3 {
                let rest: Vec<String> = others[3..].iter().map(|r| crate::i18n::place(&r.world)).collect();
                note(t, rest.join(" · "));
            }
        });
    }
}

/// The hero map's side in the panel (px).
const HERO_MAP: f32 = 150.0;

/// How long a gap before the "previously" card shows (s): a quick restart is not a break.
const PREVIOUSLY_AFTER: u64 = 30 * 60;

/// The region ledger from a snapshot's lists.
fn ledger(s: &Snapshot) -> Vec<crate::ledger::Region> {
    crate::ledger::ledger(&crate::ledger::Sources {
        needs: &s.needs,
        handovers: &s.handovers,
        catalogue: &s.catalogue,
        locks: &s.locks,
        collection: &s.collection,
        hollows: &s.hollows,
        stories: &s.stories,
        vaults: &s.vaults,
    })
}

/// "12 min ago", "3 h ago", "2 days ago".
fn ago(secs: u64) -> String {
    match secs {
        0..3600 => trf!("MINUTES_AGO", count = (secs / 60).max(1)),
        3600..86400 => trf!("HOURS_AGO", count = secs / 3600),
        _ => trf!("DAYS_AGO", count = secs / 86400),
    }
}
