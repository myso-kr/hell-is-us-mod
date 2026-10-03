//! The clues page (.spec/JOURNEY.md §3.2): the Datapad's facts the hero knows, searched
//! by a word, and gathered for the quest being followed — by the entry they are about.

use super::*;
use egui_taffy::taffy::{self, prelude::length, Style};

/// How many entries a search shows, and lines of each.
const SHOWN: usize = 12;
const LINES: usize = 4;

impl Panel {
    /// Search every known fact and the items held, in the game's language.
    pub(super) fn find_clue_card(&mut self, t: &mut Tui, snap: Option<&Snapshot>) {
        let clues = snap.map(|s| s.clues.clone()).unwrap_or_default();
        card(t, tr!("FIND_A_CLUE"), |t| {
            if !clues.text {
                note(t, tr!("CLUE_TEXT_NOT_READ_YET"));
                return;
            }
            // The search leads the page: a taller field than a form row's.
            block(t, |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.clue_query)
                        .hint_text(tr!("SEARCH_CLUES_HINT"))
                        .font(egui::FontId::proportional(15.0))
                        .margin(egui::Margin::symmetric(10, 7))
                        .desired_width(f32::INFINITY),
                )
            });
            let query = self.clue_query.trim().to_string();
            if query.is_empty() {
                // What there is to search, as two tiles; pressing them does nothing more.
                let facts: usize = clues.subjects.iter().map(|s| s.lines.len()).sum();
                let tiles = [
                    (Sub::Lore, clues.subjects.len(), tr!("STAT_DATAPAD_ENTRIES")),
                    (Sub::Research, facts, tr!("STAT_FACTS_KNOWN")),
                ];
                t.style(tw::grid(2, super::super::theme::INLINE)).add(|t| {
                    for (sort, value, label) in tiles {
                        let icon = |ui: &mut egui::Ui| {
                            crate::ui::svg::sort(ui, sort, 14.0);
                        };
                        tw::stat(t, icon, &value.to_string(), label, None);
                    }
                });
                return;
            }
            let found: Vec<(&crate::clues::Subject, Vec<&str>)> =
                clues.subjects.iter().map(|s| (s, s.matching(&query))).filter(|(_, lines)| !lines.is_empty()).collect();
            let q = query.to_lowercase();
            let items: Vec<&str> =
                clues.items.iter().filter(|i| i.to_lowercase().contains(&q)).map(String::as_str).collect();
            if found.is_empty() && items.is_empty() {
                note(t, tr!("NO_CLUE_MATCHES"));
            }
            for (s, lines) in found.iter().take(SHOWN) {
                tw::item(t, |t| {
                    // The entry as a header, how many of its lines match in a chip.
                    t.style(tw::row(super::super::theme::INLINE)).add(|t| {
                        text(t, RichText::new(&s.name).strong().color(super::super::theme::TITLE));
                        tw::chip(t, lines.len().to_string(), tw::Tone::Quiet);
                    });
                    for l in lines.iter().take(LINES) {
                        note(t, *l);
                    }
                    if lines.len() > LINES {
                        note(t, trf!("MORE_LINES", count = lines.len() - LINES));
                    }
                });
            }
            if found.len() > SHOWN {
                note(t, trf!("MORE_ENTRIES", count = found.len() - SHOWN));
            }
            if !items.is_empty() {
                note(t, trf!("HELD_ITEMS_MATCHING", items = items.join(" · ")));
            }
        });
    }

    /// The entries of the quest being followed: press one for what is known of it.
    pub(super) fn quest_clues_card(&mut self, t: &mut Tui, state: &crate::minimap::MapState, snap: Option<&Snapshot>) {
        let clues = snap.map(|s| s.clues.clone()).unwrap_or_default();
        let journal = snap.map(|s| s.journal.clone()).unwrap_or_default();
        let followed = crate::quests::followed(&journal, state.quest.as_deref()).cloned();
        let title = match &followed {
            Some(q) => trf!("CLUES_FOR_QUEST", quest = q.name),
            None => tr!("CLUES_FOR_THE_FOLLOWED_QUEST").to_string(),
        };
        card(t, &title, |t| {
            let Some(q) = followed else {
                note(t, tr!("NO_QUEST_FOLLOWED_CLUES"));
                return;
            };
            let mine: Vec<&crate::clues::Subject> =
                clues.subjects.iter().filter(|s| s.quests.contains(&q.name)).collect();
            if mine.is_empty() {
                note(t, tr!("NO_DATAPAD_ENTRIES_FOR_THIS_QUEST"));
                return;
            }
            for s in mine {
                let open = self.clue_open.as_deref() == Some(s.unit.as_str());
                let icon = |ui: &mut egui::Ui| {
                    crate::ui::svg::sort(ui, Sub::Lore, 14.0);
                };
                // The fact count at the line's end; how to use the line is its hover text.
                let count = s.lines.len().to_string();
                let end = |t: &mut Tui| {
                    w(t, |ui| {
                        tw::pill(ui, count, tw::Tone::Quiet).on_hover_text(tr!("PRESS_AN_ENTRY_FOR_WHAT_IS_KNOWN"))
                    });
                };
                if tw::line(t, Some(open), icon, s.name.as_str(), end) {
                    self.clue_open = if open { None } else { Some(s.unit.clone()) };
                }
                if open {
                    indented(t, |t| {
                        for l in &s.lines {
                            note(t, l.as_str());
                        }
                    });
                }
            }
        });
    }
}

/// `pl-6 gap-0.5`: an opened entry's lines, set in under its line's text.
fn indented(t: &mut Tui, body: impl FnOnce(&mut Tui)) {
    let pad =
        taffy::Rect { left: length(22.0_f32), right: length(0.0_f32), top: length(0.0_f32), bottom: length(0.0_f32) };
    t.style(Style { padding: pad, ..tw::col(2.0) }).add(body);
}
