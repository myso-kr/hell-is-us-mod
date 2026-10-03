//! The help page: how to start, what each page is for (press one to open it), the keys
//! as they are set now, and what to do when something is off. Short on purpose: each
//! line says one thing.

use super::*;

impl Panel {
    pub(super) fn help_tab(&mut self, t: &mut Tui, state: &crate::minimap::MapState) {
        let cols = self.columns;
        tw::masonry(t, "help", cols, 4, |t, i| match i {
            0 => start_card(t),
            1 => self.pages_card(t),
            2 => keys_help_card(t, state),
            _ => trouble_card(t),
        });
    }

    /// Every page in one line; pressing it opens the page.
    fn pages_card(&mut self, t: &mut Tui) {
        card(t, tr!("HELP_PAGES"), |t| {
            for tool in Tool::ALL.into_iter().filter(|t| *t != Tool::Help) {
                let line = RichText::new(format!("{}  ·  {}", tool.label(), tool.about()));
                if tw::pick(t, false, line) {
                    self.tool = Some(tool);
                }
            }
            note(t, tr!("HELP_CHEATS_ARE_FOLDED"));
        });
    }
}

/// Numbered lines, the number dim.
fn steps(t: &mut Tui, lines: &[&str]) {
    for (i, l) in lines.iter().enumerate() {
        t.style(tw::row(super::super::theme::INLINE)).add(|t| {
            w(t, |ui| ui.label(RichText::new(format!("{}", i + 1)).color(DIM).strong()));
            text(t, *l);
        });
    }
}

fn start_card(t: &mut Tui) {
    card(t, tr!("HELP_START"), |t| {
        let status = trf!("HELP_STEP_STATUS", gate = tr!("HERO_GATE"));
        let quest = trf!("HELP_STEP_QUEST", quests = tr!("QUESTS"));
        steps(t, &[tr!("HELP_STEP_GAME"), &status, &quest, tr!("HELP_STEP_ANSWERS")]);
    });
}

/// The keys as they are set now; the Map page changes them.
fn keys_help_card(t: &mut Tui, state: &crate::minimap::MapState) {
    card(t, tr!("KEYS"), |t| {
        let rows = [
            ("`".to_string(), tr!("HELP_KEY_PANEL")),
            (format!("F{}", state.toggle_key), tr!("SWITCH_MAP_DISPLAY")),
            (format!("F{}", state.compass_key), tr!("SHOW_OR_HIDE_THE_COMPASS")),
            (format!("F{}", state.cycle_key), tr!("GUIDE_TO_THE_NEXT_GOAL")),
            (format!("F{}", state.marker_key), tr!("PLACE_OR_REMOVE_A_MARKER")),
        ];
        for (key, what) in rows {
            t.style(tw::row(super::super::theme::INLINE)).add(|t| {
                tw::keycap(t, &key);
                text(t, what);
            });
        }
        note(t, trf!("HELP_KEYS_CHANGE_ON_MAP", map = tr!("MAP")));
    });
}

fn trouble_card(t: &mut Tui) {
    let restore = tr!("RESTORE_ALL").split(" ({").next().unwrap_or_default();
    card(t, tr!("HELP_TROUBLE"), |t| {
        for (q, a) in [
            (tr!("HELP_Q_NO_DATA"), trf!("HELP_A_NO_DATA", install = tr!("INSTALL_NET_8"))),
            // The restore button's own label, without its count.
            (tr!("HELP_Q_RESTORE"), trf!("HELP_A_RESTORE", restore = restore)),
            (tr!("HELP_Q_UPDATE"), tr!("HELP_A_UPDATE").to_string()),
            (tr!("HELP_Q_REPORT"), trf!("HELP_A_REPORT", debug = tr!("DEBUG"))),
        ] {
            tw::item(t, |t| {
                text(t, RichText::new(q).strong());
                note(t, a);
            });
        }
    });
}
