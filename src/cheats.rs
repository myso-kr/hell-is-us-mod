//! Every cheat, as one row of a table. The CLI's `set`/`hold`, the panel's tabs and
//! the restore record are all read off it, so adding a cheat is adding a row.
//!
//! Each row names the attributes it writes by the game's own names (attr.rs looks
//! them up at run time). What a row overwrites with a chosen or fixed value is put
//! back when it is switched off; what it only refills (health to its maximum) leaves
//! nothing to put back.

use crate::attr::{attr, Attr, Session};

// The attributes the table uses — set class, property — as the game names them on
// Steam build 24045435 (`hiumod list` prints every one there is). A set may be `*`
// for "whichever set has it" (attr.rs); every one here is pinned, so a later build
// that adds a same-named attribute elsewhere cannot move a cheat.
mod a {
    use super::{attr, Attr};
    const E: &str = "EnduranceAttributeSet";
    const L: &str = "LymbicAttributeSet";
    const P: &str = "PlayerAttributeSet";
    // In Hell Is Us the health bar is the cap on stamina: damage lowers the cap.
    pub const ENDURANCE: Attr = attr(E, "Endurance");
    pub const ENDURANCE_CAP: Attr = attr(E, "EnduranceCap");
    pub const ENDURANCE_MAX: Attr = attr(E, "EnduranceMax");
    pub const LYMBIC: Attr = attr(L, "LymbicEnergy");
    pub const LYMBIC_MAX: Attr = attr(L, "LymbicEnergyMax");
    pub const LYMBIC_COST: Attr = attr(P, "LymbicCostModifierCoefficient");
    pub const SKILL_COOLDOWN: Attr = attr(P, "AbilityCooldownModifierCoefficient");

    // Not attributes: plain floats on the hero and its movement component, written
    // straight (engine.rs adds them to the session once their owner's class checks).
    pub const WALK_SPEED: Attr = attr(super::MOVEMENT, "MaxWalkSpeed");
    pub const TIME: Attr = attr(super::HERO, "CustomTimeDilation");
}

/// Labels for the plain-field owners, in place of an attribute set's name.
pub const HERO: &str = "Hero";
pub const MOVEMENT: &str = "Movement";
/// The plain fields read off each owner.
pub const HERO_FIELDS: &[&str] = &["CustomTimeDilation"];
pub const MOVEMENT_FIELDS: &[&str] = &["MaxWalkSpeed"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    Survival,
    Combat,
    Movement,
}

impl Group {
    pub const ALL: [Group; 3] = [Group::Survival, Group::Combat, Group::Movement];

    /// How settings.txt names it.
    pub fn id(self) -> &'static str {
        match self {
            Group::Survival => "survival",
            Group::Combat => "combat",
            Group::Movement => "movement",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Group::Survival => "생존",
            Group::Combat => "전투",
            Group::Movement => "이동",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Effect {
    /// Hold at this value.
    Fixed(Attr, f32),
    /// Hold at whatever the slider says.
    Chosen(Attr),
    /// Refill to the current value of the second — health to its maximum.
    Fill(Attr, Attr),
}

#[derive(Clone, Copy, Debug)]
pub enum Kind {
    /// A balance, written once. Every cap below the new value is raised first, or
    /// the game clamps the balance back on its next change.
    Set { target: Attr, caps: &'static [Attr], max: f32 },
    /// Held while on.
    Toggle(&'static [Effect]),
    /// Held while on, at a chosen value.
    Slider { effects: &'static [Effect], min: f32, max: f32, default: f32 },
}

pub struct Cheat {
    pub id: &'static str,
    pub group: Group,
    pub label: &'static str,
    pub kind: Kind,
    /// Seen working in game. Unverified rows work the same way, carry a badge in
    /// the panel, and become verified per docs/CHEATS.md — not by assumption.
    pub verified: bool,
}

use Effect::{Chosen, Fill, Fixed};

const fn toggle(
    id: &'static str,
    group: Group,
    label: &'static str,
    effects: &'static [Effect],
    verified: bool,
) -> Cheat {
    Cheat { id, group, label, kind: Kind::Toggle(effects), verified }
}
#[allow(clippy::too_many_arguments)]
const fn slider(
    id: &'static str,
    group: Group,
    label: &'static str,
    effects: &'static [Effect],
    min: f32,
    max: f32,
    default: f32,
    verified: bool,
) -> Cheat {
    Cheat { id, group, label, kind: Kind::Slider { effects, min, max, default }, verified }
}

/// Cooldown multipliers stop at 0.05, not 0: in Minecraft Dungeons II a zero
/// multiplier stalled the recharge timer for good.
const NEAR_ZERO: f32 = 0.05;

// Seen in play on 24045435 (verify.txt, 2026-10-02): what the game reads directly
// works — Endurance, the movement component, time dilation. What it re-derives
// through GAS — every `*Coefficient` tried, weapon attack power — held in memory
// and did nothing, and was dropped (docs/CHEATS.md, .spec/DECISIONS.md D12).
pub const CHEATS: &[Cheat] = &[
    // 생존
    toggle("god", Group::Survival, "체력 유지 (상한을 최대로)", &[Fill(a::ENDURANCE_CAP, a::ENDURANCE_MAX)], true),
    toggle("stamina", Group::Survival, "스태미나 무한", &[Fill(a::ENDURANCE, a::ENDURANCE_CAP)], true),
    // 전투
    toggle("lymbic", Group::Combat, "림빅 에너지 무한", &[Fill(a::LYMBIC, a::LYMBIC_MAX)], false),
    // Coefficients like these held in memory but did nothing for every one tried in
    // play (docs/CHEATS.md); these two are kept only until someone tries them.
    toggle("lymbic_cost", Group::Combat, "림빅 소모 없음", &[Fixed(a::LYMBIC_COST, 0.0)], false),
    toggle("skill_cooldown", Group::Combat, "스킬 쿨다운 없음", &[Fixed(a::SKILL_COOLDOWN, NEAR_ZERO)], false),
    // 이동 — plain fields, not attributes: the values the game actually moves by.
    slider("speed", Group::Movement, "걷기 속도 (기본 450)", &[Chosen(a::WALK_SPEED)], 300.0, 2000.0, 900.0, true),
    // The hero's own time dilation speeds up everything it does — attacks, dodges,
    // movement — and leaves enemies alone.
    slider("hero_time", Group::Movement, "주인공 시간 배속 (기본 1)", &[Chosen(a::TIME)], 1.0, 3.0, 1.5, true),
];

impl Cheat {
    /// What a held cheat does each tick; nothing for one written once.
    pub fn effects(&self) -> &'static [Effect] {
        match self.kind {
            Kind::Toggle(e) | Kind::Slider { effects: e, .. } => e,
            Kind::Set { .. } => &[],
        }
    }
}

pub fn find(id: &str) -> Option<&'static Cheat> {
    CHEATS.iter().find(|c| c.id == id)
}

pub fn ids(pick: impl Fn(&Kind) -> bool) -> String {
    CHEATS.iter().filter(|c| pick(&c.kind)).map(|c| c.id).collect::<Vec<_>>().join(", ")
}

/// Write a balance once.
pub fn set_value(s: &Session, id: &str, v: f32) -> Result<(), String> {
    let Some(Kind::Set { target, caps, max }) = find(id).map(|c| c.kind) else {
        return Err(format!("`{id}` cannot be set — one of: {}", ids(|k| matches!(k, Kind::Set { .. }))));
    };
    if !(0.0..=max).contains(&v) {
        return Err(format!("{id} takes 0..={max}"));
    }
    for &cap in caps {
        if s.current(cap)? < v {
            s.put(cap, v)?;
        }
    }
    s.put(target, v)
}

/// A held cheat that is on, and its slider value if it has one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Active {
    pub cheat: &'static str,
    pub value: f32,
}

impl Active {
    /// `god`, or `speed=1.5` for a slider. A slider without a value takes its default.
    pub fn parse(s: &str) -> Result<Active, String> {
        let (id, value) = match s.split_once('=') {
            Some((id, v)) => (id, Some(v.parse::<f32>().map_err(|_| format!("{v} is not a number"))?)),
            None => (s, None),
        };
        let held = || ids(|k| !matches!(k, Kind::Set { .. }));
        let c = find(id).ok_or_else(|| format!("unknown cheat `{id}` — one of: {}", held()))?;
        match (c.kind, value) {
            (Kind::Toggle(_), None) => Ok(Active { cheat: c.id, value: 0.0 }),
            (Kind::Toggle(_), Some(_)) => Err(format!("{id} takes no value")),
            (Kind::Slider { min, max, default, .. }, v) => {
                let v = v.unwrap_or(default);
                if (min..=max).contains(&v) {
                    Ok(Active { cheat: c.id, value: v })
                } else {
                    Err(format!("{id} takes {min}..={max}"))
                }
            }
            (Kind::Set { .. }, _) => Err(format!("{id} is written once — use `set {id} <value>`, not hold")),
        }
    }

    fn effects(self) -> &'static [Effect] {
        find(self.cheat).map_or(&[], Cheat::effects)
    }

    /// What this overwrites, and so must put back when it stops.
    pub fn restores(self) -> Vec<Attr> {
        self.effects()
            .iter()
            .filter_map(|e| match *e {
                Fixed(a, _) | Chosen(a) => Some(a),
                Fill(..) => None,
            })
            .collect()
    }

    pub fn apply(self, s: &Session) -> Result<(), String> {
        for e in self.effects() {
            match *e {
                Fixed(a, v) => s.put(a, v)?,
                Chosen(a) => s.put(a, self.value)?,
                Fill(a, max) => s.put(a, s.current(max)?)?,
            }
        }
        Ok(())
    }
}

/// Every attribute the table names — for doctor, which says which the game lacks.
pub fn attributes() -> Vec<Attr> {
    let mut v: Vec<Attr> = Vec::new();
    for c in CHEATS {
        match c.kind {
            Kind::Set { target, caps, .. } => {
                v.push(target);
                v.extend(caps);
            }
            Kind::Toggle(e) | Kind::Slider { effects: e, .. } => v.extend(e.iter().flat_map(|e| match *e {
                Fixed(a, _) | Chosen(a) => vec![a],
                Fill(a, b) => vec![a, b],
            })),
        }
    }
    v.sort();
    v.dedup();
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn ids_are_unique_and_lowercase() {
        let ids: HashSet<_> = CHEATS.iter().map(|c| c.id).collect();
        assert_eq!(ids.len(), CHEATS.len());
        assert!(CHEATS.iter().all(|c| c.id.chars().all(|ch| ch.is_ascii_lowercase() || ch == '_')));
    }

    #[test]
    fn ranges_hold_their_defaults() {
        for c in CHEATS {
            if let Kind::Slider { min, max, default, effects } = c.kind {
                assert!(min < max && (min..=max).contains(&default), "{}", c.id);
                assert!(effects.iter().any(|e| matches!(e, Chosen(_))), "{} has a slider that moves nothing", c.id);
            }
        }
    }

    #[test]
    fn every_held_cheat_restores_what_it_overwrites() {
        for c in CHEATS {
            if let Kind::Toggle(e) | Kind::Slider { effects: e, .. } = c.kind {
                let a = Active { cheat: c.id, value: 1.0 };
                for eff in e {
                    if let Fixed(t, _) | Chosen(t) = *eff {
                        assert!(a.restores().contains(&t), "{} does not restore {:?}", c.id, t);
                    }
                }
            }
        }
    }

    #[test]
    fn every_group_has_something() {
        for g in Group::ALL {
            assert!(CHEATS.iter().any(|c| c.group == g), "{g:?}");
        }
    }

    #[test]
    fn parses_and_bounds() {
        assert_eq!(Active::parse("god"), Ok(Active { cheat: "god", value: 0.0 }));
        assert_eq!(Active::parse("speed=1200"), Ok(Active { cheat: "speed", value: 1200.0 }));
        assert_eq!(Active::parse("speed"), Ok(Active { cheat: "speed", value: 900.0 }));
        assert!(Active::parse("speed=50").is_err());
        assert!(Active::parse("speed=5000").is_err());
        assert!(Active::parse("god=1").is_err());
        assert!(Active::parse("fly").is_err());
    }
}
