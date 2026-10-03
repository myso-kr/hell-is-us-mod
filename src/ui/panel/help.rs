//! The help page: how to start, what each page is for (press one to open it), the keys
//! as they are set now, and what to do when something is off. Short on purpose: each
//! line says one thing.

use super::super::theme::{ACCENT, ACCENT_DEEP, INLINE, TEXT, TITLE};
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

    /// Every page in one line, its name then what it is for (dim); pressing it opens the page.
    fn pages_card(&mut self, t: &mut Tui) {
        card(t, tr!("HELP_PAGES"), |t| {
            for tool in Tool::ALL.into_iter().filter(|t| *t != Tool::Help) {
                let pressed = block(t, |ui| {
                    let mut job = egui::text::LayoutJob::default();
                    let (style, font, valign) = (ui.style().clone(), egui::FontSelection::Default, egui::Align::Center);
                    RichText::new(tool.label()).color(TEXT).append_to(&mut job, &style, font.clone(), valign);
                    RichText::new(format!("   {}", tool.about())).color(DIM).append_to(&mut job, &style, font, valign);
                    ui.add(egui::Button::selectable(false, job).wrap_mode(egui::TextWrapMode::Wrap)).clicked()
                });
                if pressed {
                    self.tool = Some(tool);
                }
            }
            note(t, tr!("HELP_CHEATS_ARE_FOLDED"));
        });
    }
}

/// A numbered sequence: each step's number in a round badge, its text beside it.
fn steps(t: &mut Tui, lines: &[&str]) {
    for (i, l) in lines.iter().enumerate() {
        t.style(tw::row(INLINE)).add(|t| {
            w(t, |ui| badge(ui, i + 1));
            text(t, *l);
        });
    }
}

/// A step's number: the digit on a small filled circle in the deep accent.
fn badge(ui: &mut egui::Ui, n: usize) {
    const SIDE: f32 = 20.0;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(SIDE, SIDE), egui::Sense::hover());
    let p = ui.painter();
    p.circle_filled(rect.center(), SIDE / 2.0, ACCENT_DEEP);
    p.circle_stroke(rect.center(), SIDE / 2.0 - 0.5, egui::Stroke::new(1.0, ACCENT.gamma_multiply(0.5)));
    p.text(rect.center(), egui::Align2::CENTER_CENTER, n.to_string(), egui::FontId::proportional(11.5), TITLE);
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
            // Last: the console is for checking things, not for play.
            ("Shift+`".to_string(), tr!("HELP_KEY_CONSOLE")),
        ];
        for (key, what) in rows {
            t.style(tw::row(INLINE)).add(|t| {
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
