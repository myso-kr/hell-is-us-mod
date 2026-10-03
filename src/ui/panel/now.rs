//! The "now" page (.spec/JOURNEY.md §3): one screen that answers "what now?" — where the
//! last session left off, what is about to be missed, what is left in this region, and
//! where else a trip pays. Counts and pointers only; the details stay on their own pages.

use super::super::theme::INLINE;
use super::*;
use crate::actors::Sub;

impl Panel {
    /// The cards this page shows: "previously" only while there is one to show.
    pub(super) fn now_cards(&self) -> usize {
        3 + self.previously_shown() as usize
    }

    fn previously_shown(&self) -> bool {
        self.previous.as_ref().is_some_and(|p| crate::session::now().saturating_sub(p.when) >= PREVIOUSLY_AFTER)
            && !self.previous_dismissed
    }

    pub(super) fn now_tab(&mut self, t: &mut Tui, state: &mut crate::minimap::MapState, snap: Option<&Snapshot>) {
        let cols = self.columns;
        let first = !self.previously_shown() as usize;
        tw::masonry(t, "now", cols, self.now_cards(), |t, i| match i + first {
            0 => self.previously_card(t, snap),
            1 => self.before_card(t, state, snap),
            2 => self.region_card(t, snap),
            _ => self.regions_card(t, snap),
        });
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
                            Some(l) => format!("{subject} — {l}"),
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
                        w(t, |ui| ui.label(RichText::new(tr!("SOON")).color(BAD).small().strong()));
                        let label = format!("{}{}", d.title, if d.started { "" } else { tr!("NOT_STARTED") });
                        if tw::pick(t, state.quest.as_deref() == Some(d.key.as_str()), label) && d.started {
                            state.quest = Some(d.key.clone());
                            state.target = None;
                            state.chosen = false;
                            state.guide_auto = true;
                            state.route = true;
                        }
                    });
                    note(t, format!("{} — {}", d.due.label(), d.what));
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

    /// What is left in the region the hero is in (§3.1), by kind, each a pointer to the
    /// page that has the places.
    fn region_card(&mut self, t: &mut Tui, snap: Option<&Snapshot>) {
        let here = snap.and_then(|s| s.world.clone()).map(|w| crate::survey::Survey::world_of(&w).to_string());
        let rows = snap.map(ledger).unwrap_or_default();
        let title = here.as_deref().map_or(tr!("THIS_REGION").to_string(), crate::i18n::place);
        card(t, &title, |t| {
            let Some(here) = here.as_deref() else {
                note(t, tr!("WAITING_FOR_THE_GAME"));
                return;
            };
            if snap.is_some_and(|s| s.catalogue.is_empty() && s.collection.is_empty()) {
                note(t, tr!("NO_SURVEY_DB_RUN_DOCTOR_SURVEY"));
                return;
            }
            let Some(r) = rows.iter().find(|r| r.world == here) else {
                note(t, tr!("NOTHING_LEFT_HERE_THE_MOD_KNOWS_OF"));
                return;
            };
            note(t, tr!("COUNTS_ONLY_OPEN_A_LINE"));
            let lines: [(Sub, usize, String, Tool); 8] = [
                (Sub::Quest, r.quest, trf!("LEDGER_QUEST_PLACES", count = r.quest), Tool::Quests),
                (Sub::Npc, r.handovers, trf!("LEDGER_HANDOVERS", count = r.handovers), Tool::Quests),
                (Sub::Puzzle, r.puzzles, trf!("LEDGER_PUZZLES", count = r.puzzles), Tool::Puzzles),
                (Sub::LymbicLock, r.locks, trf!("LEDGER_LOCKS", count = r.locks, open = r.openable), Tool::Puzzles),
                (Sub::Vault, r.vaults, trf!("LEDGER_VAULT_DOORS", count = r.vaults), Tool::Puzzles),
                (Sub::Stash, r.collect, trf!("LEDGER_COLLECTIBLES", count = r.collect), Tool::Collect),
                (Sub::EnemyGroup, r.groups, trf!("LEDGER_ENEMY_GROUPS", count = r.groups), Tool::Collect),
                (Sub::Npc, r.stories, trf!("LEDGER_STORIES", count = r.stories), Tool::Collect),
            ];
            for (sort, n, label, page) in lines {
                if n == 0 {
                    continue;
                }
                if tw::pick_with(
                    t,
                    false,
                    |ui| {
                        crate::ui::svg::sort(ui, sort, 18.0);
                    },
                    label,
                ) {
                    self.tool = Some(page);
                }
            }
        });
    }

    /// Every other region with something left (§3.4), most to do there now first: a trip
    /// with the APC that pays.
    fn regions_card(&mut self, t: &mut Tui, snap: Option<&Snapshot>) {
        let here = snap.and_then(|s| s.world.clone()).map(|w| crate::survey::Survey::world_of(&w).to_string());
        let rows = crate::ledger::by_trip(snap.map(ledger).unwrap_or_default());
        card(t, tr!("OTHER_REGIONS_CARD"), |t| {
            let others: Vec<_> = rows.iter().filter(|r| Some(r.world.as_str()) != here.as_deref()).collect();
            if others.is_empty() {
                note(t, tr!("NOTHING_LEFT_ELSEWHERE"));
                return;
            }
            note(t, tr!("SORTED_BY_WHAT_YOU_CAN_DO_THERE"));
            for r in others {
                let line =
                    trf!("TRIP_ROW", place = crate::i18n::place(&r.world), doable = r.doable(), left = r.total());
                let colour = if r.doable() > 0 { super::super::theme::TEXT } else { DIM };
                text(t, RichText::new(line).color(colour));
            }
        });
    }
}

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
