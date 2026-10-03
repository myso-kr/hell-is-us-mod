//! The minimap's icons: one SVG per finer sort (`actors::Sub`) in `assets/icons/`,
//! named after the sort's id (`enemy.feral` → `enemy_feral.svg`), built into the
//! binary and rasterised once, at the size the map draws them, by resvg. Edit the
//! files to change an icon; nothing about their look is in the code.
//!
//! Each becomes a small premultiplied `0xAARRGGBB` bitmap — the canvas's own pixel
//! format — so drawing one is a blend per pixel and nothing more.

use crate::actors::{Kind, Sub};
use crate::minimap::PinKind;

/// A rasterised icon, `size` px square.
pub struct Icon {
    pub size: usize,
    pub px: Vec<u32>,
}

/// A sort's SVG.
pub fn source(s: Sub) -> &'static str {
    use Sub::*;
    match s {
        Feral => include_str!("../../assets/icons/enemy_feral.svg"),
        Primeval => include_str!("../../assets/icons/enemy_primeval.svg"),
        Negator => include_str!("../../assets/icons/enemy_negator.svg"),
        Protector => include_str!("../../assets/icons/enemy_protector.svg"),
        OtherEnemy => include_str!("../../assets/icons/enemy_other.svg"),
        Medicine => include_str!("../../assets/icons/item_medicine.svg"),
        Food => include_str!("../../assets/icons/item_food.svg"),
        Consumable => include_str!("../../assets/icons/item_consumable.svg"),
        Weapon => include_str!("../../assets/icons/item_weapon.svg"),
        Gear => include_str!("../../assets/icons/item_gear.svg"),
        Skill => include_str!("../../assets/icons/item_skill.svg"),
        DroneModule => include_str!("../../assets/icons/item_drone.svg"),
        Research => include_str!("../../assets/icons/item_research.svg"),
        Lore => include_str!("../../assets/icons/item_lore.svg"),
        Quest => include_str!("../../assets/icons/item_quest.svg"),
        Stash => include_str!("../../assets/icons/item_stash.svg"),
        OtherItem => include_str!("../../assets/icons/item_other.svg"),
        Loot => include_str!("../../assets/icons/loot.svg"),
        Npc => include_str!("../../assets/icons/npc.svg"),
        Door => include_str!("../../assets/icons/interact_door.svg"),
        LymbicLock => include_str!("../../assets/icons/interact_lock.svg"),
        Translation => include_str!("../../assets/icons/interact_translation.svg"),
        SavePoint => include_str!("../../assets/icons/save.svg"),
        EnemyGroup => include_str!("../../assets/icons/enemy_group.svg"),
        Puzzle => include_str!("../../assets/icons/interact_puzzle.svg"),
        Vault => include_str!("../../assets/icons/interact_vault.svg"),
    }
}

/// A map pin's SVG (`assets/pins/<word>.svg`).
pub fn pin_source(k: PinKind) -> &'static str {
    match k {
        PinKind::LockedDoor => include_str!("../../assets/pins/locked_door.svg"),
        PinKind::LockedChest => include_str!("../../assets/pins/locked_chest.svg"),
        PinKind::LymbicLock => include_str!("../../assets/pins/lymbic_lock.svg"),
        PinKind::Puzzle => include_str!("../../assets/pins/puzzle.svg"),
        PinKind::Code => include_str!("../../assets/pins/code.svg"),
        PinKind::KeyNeeded => include_str!("../../assets/pins/key_needed.svg"),
        PinKind::ItemLater => include_str!("../../assets/pins/item_later.svg"),
        PinKind::Merchant => include_str!("../../assets/pins/merchant.svg"),
        PinKind::Npc => include_str!("../../assets/pins/npc.svg"),
        PinKind::Quest => include_str!("../../assets/pins/quest.svg"),
        PinKind::Danger => include_str!("../../assets/pins/danger.svg"),
        PinKind::Boss => include_str!("../../assets/pins/boss.svg"),
        PinKind::Timeloop => include_str!("../../assets/pins/timeloop.svg"),
        PinKind::Save => include_str!("../../assets/pins/save.svg"),
        PinKind::Shortcut => include_str!("../../assets/pins/shortcut.svg"),
        PinKind::Ladder => include_str!("../../assets/pins/ladder.svg"),
        PinKind::DeadEnd => include_str!("../../assets/pins/dead_end.svg"),
        PinKind::Water => include_str!("../../assets/pins/water.svg"),
        PinKind::View => include_str!("../../assets/pins/view.svg"),
        PinKind::Treasure => include_str!("../../assets/pins/treasure.svg"),
        PinKind::Note => include_str!("../../assets/pins/note.svg"),
        PinKind::Home => include_str!("../../assets/pins/home.svg"),
        PinKind::Question => include_str!("../../assets/pins/question.svg"),
        PinKind::Mark => include_str!("../../assets/pins/mark.svg"),
    }
}

pub fn render(svg: &str, size: usize) -> Result<Icon, String> {
    use resvg::{tiny_skia, usvg};
    let tree = usvg::Tree::from_str(svg, &usvg::Options::default()).map_err(|e| format!("icon: {e}"))?;
    let mut pixmap = tiny_skia::Pixmap::new(size as u32, size as u32).ok_or("icon: empty size")?;
    let s = tree.size();
    let scale = size as f32 / s.width().max(s.height());
    resvg::render(&tree, tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
    // tiny-skia keeps premultiplied RGBA bytes; the canvas wants premultiplied ARGB words.
    let px = pixmap
        .data()
        .chunks_exact(4)
        .map(|c| (c[3] as u32) << 24 | (c[0] as u32) << 16 | (c[1] as u32) << 8 | c[2] as u32)
        .collect();
    Ok(Icon { size, px })
}

/// One icon per sort, in `Sub::ALL` order; and one per map pin kind, a little larger.
pub struct Icons(pub Vec<Icon>, pub Vec<Icon>);

impl Icons {
    pub fn new(size: usize) -> Result<Icons, String> {
        let sorts = Sub::ALL.iter().map(|&s| render(source(s), size)).collect::<Result<_, _>>()?;
        let pin = size + size / 4;
        let pins = PinKind::ALL.iter().map(|&k| render(pin_source(k), pin)).collect::<Result<_, _>>()?;
        Ok(Icons(sorts, pins))
    }

    pub fn get(&self, s: Sub) -> &Icon {
        &self.0[Sub::ALL.iter().position(|&x| x == s).unwrap()]
    }

    pub fn pin(&self, k: PinKind) -> &Icon {
        &self.1[PinKind::ALL.iter().position(|&x| x == k).unwrap()]
    }

    /// A kind's usual icon (its first sort's), for where only the kind is known.
    pub fn of_kind(&self, k: Kind) -> &Icon {
        self.get(*Sub::ALL.iter().find(|s| s.kind() == k).unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_renders_something_opaque_in_the_middle() {
        let icons = Icons::new(16).unwrap();
        for s in Sub::ALL {
            let i = icons.get(s);
            assert_eq!(i.px.len(), 16 * 16);
            let centre = (5..11).flat_map(|y| (5..11).map(move |x| y * 16 + x)).map(|j| i.px[j] >> 24).max().unwrap();
            assert!(centre > 200, "{s:?}: most opaque centre pixel {centre}");
            assert_eq!(i.px[0] >> 24, 0, "{s:?}: corner should be clear");
        }
        assert_eq!(icons.of_kind(Kind::Enemy).px, icons.get(Sub::Feral).px);
        for k in PinKind::ALL {
            let i = icons.pin(k);
            assert!(i.px.iter().any(|p| p >> 24 > 200), "{k:?}: drawn");
        }
    }
}
