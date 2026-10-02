# Cheats

Every cheat is a row of `src/cheats.rs`. A row names the attributes it writes by
set class and property name, pinned on build 24045435. **None has been seen
working yet.**

"Verified" means a player turned the cheat on and saw the effect in play, and the
debug tab showed the value holding (green). It is recorded with ✓ in `verify.txt`,
and then the row's `verified` is set to `true`.

| id | Tab | Writes | Kind | Default on 24045435 | Seen in play |
|---|---|---|---|---|---|
| `god` | 생존 | Endurance.`EnduranceCap` ← `EnduranceMax` | refill | 5000 / 5000 | **works** |
| `stamina` | 생존 | Endurance.`Endurance` ← `EnduranceCap` | refill | 5000 | **works** |
| `ghost` | 생존 | hero `TeamID` and `Faction` (CharlieCharacter, 1/1) set to the enemies' (2/2); put back when off | toggle | 1 / 1 | **works as a ghost for getting past** (2026-10-03): enemies ignore the hero, and the hero's blows do not land. TeamID alone: enemies still attack — Faction decides both |
| `lymbic` | 전투 | Lymbic.`LymbicEnergy` ← `LymbicEnergyMax` | refill | 700 | not yet tried |
| `lymbic_cost` | 전투 | Player.`LymbicCostModifierCoefficient` = 0 | fixed | 1 | not yet tried (likely fails, see below) |
| `skill_cooldown` | 전투 | Player.`AbilityCooldownModifierCoefficient` = 0.05 | fixed | 1 | not yet tried (likely fails) |
| `speed` | 이동 | movement component `MaxWalkSpeed` (a field) | slider 300–2000 | 450 | **works** |
| `hero_time` | 이동 | hero `CustomTimeDilation` (a field) | slider 1–3 | 1 | **works** |
| `game_speed` | 이동 | WorldSettings `TimeDilation` (a field) | slider 0.2–3 | 1 | **works** (2026-10-03) |
| `enemy_time` | 전투 | every live enemy's `CustomTimeDilation` (extras.rs; put back per enemy) | slider 0.05–1 | 1 | **works** (2026-10-03) |
| `frail` | 전투 | every live enemy's `HealthAttributeSet.Health` held at 1 (put back per enemy) | toggle | 680–1130 (Tier 1) | **works** (2026-10-03) |
| `stock` | 아이템 | inventory stacks of `CharlieInventoryUseable*`: count (item `ItemData`+8, native) kept ≥ first seen, ≥ 2 | toggle | — | **works** (2026-10-03) — held from the count when switched on, never below 2; the game's own number may lag until the inventory is reopened |
| `shards` | 아이템 | inventory stacks of `CharlieInventoryShardItem`: count written once, up to 990 (default 900) — at the game's own 999 a stack takes no more, so pickups stop and no acquired notice shows | set (button) | — | **works** (2026-10-03) |
| `weapon_xp` | 아이템 | weapon items (native, 24045435): total `+0x13C` and within-level `+0x140` each grow by what the game grants × (multiplier − 1); level `+0x130`, cap `+0x138`, next `+0x148` checked first; levelling left to the game | slider 1–10 | 1 | **works** (2026-10-03) — ×3 on a sword kill: total 265 → 580 (105 granted, 315 added), levelling on the next kill |
| position slots | 이동 | hero root `RelativeLocation` + `ComponentToWorld` translation, `Velocity` 0 (5 slots, same area only) | action | — | **works** (2026-10-03) |

## What does not work from outside, and why

- **Ignoring hits** (2026-10-03): neither the engine's `bCanBeDamaged` nor clearing `bGenerateOverlapEvents` on the hero's components stopped a blow. The game's invincibility is gameplay tags, and hits are not plain overlaps; data writes do not reach either. Dropped.


These attributes held their value in memory but had no effect in play
(2026-10-02, two test rounds):

- `MovementSpeedModifierCoefficient`
- `MeleeAttackAnimPlayRateMultiplier`
- `DodgeAnimPlayRateMultiplier`
- `EnduranceCostCoefficient`
- `MeleeDamageBoostCoefficient`
- `GlobalHealingBonusCoefficient`
- `GlobalDefenseCoefficient`
- `InvincibilityWindowModifierCoefficient`
- `DroneDockedAbilityCooldownModifierCoefficient`
- `GlobalXPModifierCoefficient`
- `WeaponAttributeSet.WeaponAttackPower`

The pattern: the game reads current resources such as `Endurance` and
`LymbicEnergy` directly, so writing them works. Coefficients and weapon stats feed
GameplayEffect calculations, which read GAS's aggregators or values captured when
the effect executes. Writing the attribute set's memory does not reach either. Those
effects need to be applied inside the game, as a GameplayEffect, which is Phase 1.

`poise` was dropped as well: every attribute in `PoiseAttributeSet` is 0 on the hero.

A "field" is a plain property rather than an attribute. It is written only after
its owner's class has been checked.

Not in scope: anything that unlocks achievements, and anything that hides the tool
from the game.
