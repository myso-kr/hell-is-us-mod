//! The Settings page: what the mod may show and change, each agreed to on its own
//! (.spec/CONSENT.md). Hell Is Us is made to be found out without a map or answers; the
//! mod adds only what the player chose, and until they choose, nothing.

use super::Panel;
use crate::settings::Consent;
use crate::ui::tw::{self, card, note, text};
use eframe::egui::RichText;
use egui_taffy::{Tui, TuiBuilderLogic};

/// Each consent as a question to the player (JOURNEY.md §1, what players run into): its
/// bit, the question, the context and what turns on, what it costs the game, the answer.
const KINDS: [Ask; 4] = [
    (Consent::MAP, "CONSENT_MAP", "CONSENT_MAP_WHAT", "CONSENT_MAP_COST", "CONSENT_MAP_YES"),
    (Consent::PLACES, "CONSENT_PLACES", "CONSENT_PLACES_WHAT", "CONSENT_PLACES_COST", "CONSENT_PLACES_YES"),
    (Consent::ANSWERS, "CONSENT_ANSWERS", "CONSENT_ANSWERS_WHAT", "CONSENT_ANSWERS_COST", "CONSENT_ANSWERS_YES"),
    (Consent::CHEATS, "CONSENT_CHEATS", "CONSENT_CHEATS_WHAT", "CONSENT_CHEATS_COST", "CONSENT_CHEATS_YES"),
];

type Ask = (u8, &'static str, &'static str, &'static str, &'static str);

impl Panel {
    /// The page, as a survey: why it asks across the top, then the four questions in a
    /// grid of cards all as wide and as tall as each other (`grid-cols-2 auto-rows-fr
    /// items-stretch`), not the masonry of the other pages.
    pub(super) fn settings_tab(&mut self, t: &mut Tui) {
        use egui_taffy::taffy::{prelude::fr, AlignItems, Style};
        self.consent_intro(t);
        let grid =
            Style { align_items: Some(AlignItems::Stretch), grid_auto_rows: vec![fr(1.0_f32)], ..tw::grid(2, tw::GAP) };
        t.style(grid).add(|t| {
            for ask in KINDS {
                self.consent_card(t, ask);
            }
        });
    }

    fn consent_intro(&mut self, t: &mut Tui) {
        card(t, tr!("CONSENT_TITLE"), |t| {
            text(t, RichText::new(tr!("CONSENT_INTRO")));
            if self.consent.is_none() {
                // A sentence, wrapped: as a chip it ran past the card.
                text(t, RichText::new(tr!("CONSENT_NOT_CHOSEN")).color(super::super::theme::WAIT));
            }
            t.style(tw::row(super::super::theme::INLINE)).add(|t| {
                if tw::w(t, |ui| ui.button(tr!("CONSENT_ALL_ON"))).clicked() {
                    self.set_consent(Consent(Consent::ALL));
                }
                if tw::w(t, |ui| ui.button(tr!("CONSENT_ALL_OFF"))).clicked() {
                    self.set_consent(Consent(0));
                }
            });
        });
    }

    fn consent_card(&mut self, t: &mut Tui, (bit, question, what, cost, yes): Ask) {
        card(t, crate::i18n::tr(question), |t| {
            text(t, RichText::new(crate::i18n::tr(what)));
            note(t, crate::i18n::tr(cost));
            let mut on = self.grants(bit);
            if tw::switch(t, &mut on, crate::i18n::tr(yes)).changed() {
                let now = self.consent.unwrap_or_default().0;
                self.set_consent(Consent(if on { now | bit } else { now & !bit }));
            }
        });
    }
}
