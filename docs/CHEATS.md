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
| `lymbic` | 전투 | Lymbic.`LymbicEnergy` ← `LymbicEnergyMax` | refill | 700 | not yet tried |
| `lymbic_cost` | 전투 | Player.`LymbicCostModifierCoefficient` = 0 | fixed | 1 | not yet tried (likely fails, see below) |
| `skill_cooldown` | 전투 | Player.`AbilityCooldownModifierCoefficient` = 0.05 | fixed | 1 | not yet tried (likely fails) |
| `speed` | 이동 | movement component `MaxWalkSpeed` (a field) | slider 300–2000 | 450 | **works** |
| `hero_time` | 이동 | hero `CustomTimeDilation` (a field) | slider 1–3 | 1 | **works** |

## What does not work from outside, and why

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
