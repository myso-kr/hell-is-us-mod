//! The "now" page (.spec/JOURNEY.md §3): one screen that answers "what now?" — where the
//! last session left off, what is about to be missed, what is left in this region, and
//! where else a trip pays. Counts and pointers only; the details stay on their own pages.

use super::super::theme::INLINE;
use super::*;
use crate::actors::Sub;
use eframe::egui::Color32;

impl Panel {
    /// The cards this page shows under the hero: "previously" only while there is one.
    pub(super) fn now_cards(&self) -> usize {
        2 + self.previously_shown() as usize
    }

    fn previously_shown(&self) -> bool {
        self.previous.as_ref().is_some_and(|p| crate::session::now().saturating_sub(p.when) >= PREVIOUSLY_AFTER)
            && !self.previous_dismissed
    }

    pub(super) fn now_tab(&mut self, t: &mut Tui, state: &mut crate::minimap::MapState, snap: Option<&Snapshot>) {
        self.hero(t, snap);
        let cols = self.columns;
        let first = !self.previously_shown() as usize;
        tw::masonry(t, "now", cols, self.now_cards(), |t, i| match i + first {
            0 => self.previously_card(t, snap),
            1 => self.before_card(t, state, snap),
            _ => self.regions_card(t, snap),
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
        if open.is_some() {
            self.tool = open;
        }
    }

    /// Where the last session left off (§3.8): when, where, the quest followed and what
    /// it is waiting on now.
    fn previously_card(&mut self, t: &mut Tui, snap: Option<&Snapshot>) {
        let Some(prev) = self.previous.clone() else { return };
        let journal = snap.map(|s| s.journal.clone()).unwrap_or_default();
        card(t, tr!("PREVIOUSLY"), |t| {
            let ago = ago(crate::session::now().saturating_sub(prev.when));
            text(t, trf!("LAST_PLAYED_AGO_IN", ago = ago, place = crate::i18n::place(&prev.world)));
            match prev.quest.as_deref().map(|k| journal.iter().find(|q| q.key == k)) {
                None => note(t, tr!("NO_QUEST_WAS_FOLLOWED")),
                Some(None) => note(t, tr!("THAT_QUEST_IS_NO_LONGER_OPEN")),
                Some(Some(q)) => {
                    t.style(tw::row(INLINE)).add(|t| {
                        w(t, |ui| crate::ui::svg::quest(ui, q.kind, 18.0));
                        text(t, trf!("YOU_WERE_FOLLOWING", quest = q.name));
                    });
                    for (subject, last) in q.leads.iter().take(3) {
                        let line = match last {
                            Some(l) => format!("{subject}: {l}"),
                            None => subject.clone(),
                        };
                        note(t, line);
                    }
                }
            }
            if w(t, |ui| ui.button(tr!("GOT_IT"))).clicked() {
                self.previous_dismissed = true;
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
                            state.target = None;
                            state.chosen = false;
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

    /// Every other region with something left (§3.4) as bars, most to do there now
    /// first: what can be done now (hand-overs, locks the rods held open) and what is
    /// left, to one scale.
    fn regions_card(&mut self, t: &mut Tui, snap: Option<&Snapshot>) {
        let here = snap.and_then(|s| s.world.clone()).map(|w| crate::survey::Survey::world_of(&w).to_string());
        let rows = crate::ledger::by_trip(snap.map(ledger).unwrap_or_default());
        card(t, tr!("OTHER_REGIONS_CARD"), |t| {
            let others: Vec<(String, usize, usize)> = rows
                .iter()
                .filter(|r| Some(r.world.as_str()) != here.as_deref())
                .map(|r| (crate::i18n::place(&r.world), r.doable(), r.total()))
                .collect();
            if others.is_empty() {
                note(t, tr!("NOTHING_LEFT_ELSEWHERE"));
                return;
            }
            block(t, |ui| tw::bars(ui, &others, (tr!("LEGEND_DOABLE"), tr!("LEGEND_LEFT"))));
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
