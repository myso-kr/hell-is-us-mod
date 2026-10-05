# Cheats

Every cheat is one row of `CHEATS` in `src/cheat/cheats.rs`. A row names what it
writes by set class and property name, pinned on build 24045435. Cheats whose
targets are not the hero (enemies, the inventory, the hero's team) are carried out
by `src/cheat/extras.rs`; the position slots live in `src/engine/` and are not a
row of the table.

As of 2026-10-03, every row except the three in the Combat tab that touch Lymbic
energy and cooldowns has been seen working in play. Those three carry an
"unverified" badge in the panel.

## How a cheat becomes verified

"Verified" means a player turned the cheat on, saw the effect in play, and the
panel's debug tab showed the value holding (green). The debug tab records the
outcome in `verify.txt` in the mod's data folder (`src/infra/verify.rs`), one
`id ok` or `id fail` per line, and `doctor` prints it beside each cheat. The file
is a report, not a switch: the row's `verified` flag is set to `true` by hand
after someone reads the report.

## The table

Tab names are the panel's English labels. "Game default" is the value the game
holds on build 24045435 before the cheat touches it.

| id | Tab | Writes | Kind | Game default | Seen in play |
|---|---|---|---|---|---|
| `god` | Survival | Endurance.`EnduranceCap` ← `EnduranceMax` | refill | 5000 / 5000 | works |
| `stamina` | Survival | Endurance.`Endurance` ← `EnduranceCap` | refill | 5000 | works |
| `ghost` | Survival | hero `TeamID` and `Faction` (CharlieCharacter, 1/1) set to the enemies' values (2/2); put back when off | toggle | 1 / 1 | works, as a ghost for getting past (2026-10-03): enemies ignore the hero and the hero's blows do not land. See [CHEATS-RESEARCH.md §6](CHEATS-RESEARCH.md) |
| `lymbic` | Combat | Lymbic.`LymbicEnergy` ← `LymbicEnergyMax` | refill | 700 | not yet tried |
| `lymbic_cost` | Combat | Player.`LymbicCostModifierCoefficient` = 0 | fixed | 1 | not yet tried; likely fails (see below) |
| `skill_cooldown` | Combat | Player.`AbilityCooldownModifierCoefficient` = 0.05 | fixed | 1 | not yet tried; likely fails |
| `enemy_time` | Combat | every live enemy's `CustomTimeDilation`, put back per enemy | slider 0.05–1 (default 0.3) | 1 | works (2026-10-03) |
| `frail` | Combat | every live enemy's `HealthAttributeSet.Health` held at 1, put back per enemy | toggle | 680–1130 (Tier 1) | works (2026-10-03) |
| `speed` | Movement | movement component `MaxWalkSpeed` (a field) | slider 300–2000 (default 900) | 450 | works |
| `hero_time` | Movement | hero `CustomTimeDilation` (a field) | slider 1–3 (default 1.5) | 1 | works |
| `game_speed` | Movement | WorldSettings `TimeDilation` (a field) | slider 0.2–3 (default 0.5) | 1 | works (2026-10-03) |
| position slots | Movement | hero root `RelativeLocation` and `ComponentToWorld` translation, movement `Velocity` set to 0. 5 slots, same world only | action | — | works (2026-10-03) |
| `stock` | Items | inventory stacks of `CharlieInventoryUseable*`: the count (item `ItemData` + 8, native) kept at no less than when first seen, and never below 2 | toggle | — | works (2026-10-03). The game's own displayed number may lag until the inventory is reopened |
| `shards` | Items | inventory stacks of `CharlieInventoryShardItem`: count written once, up to 990 (default 900) | set (button) | — | works (2026-10-03). At the game's own maximum of 999 a stack takes no more, so pickups stop and no "acquired" notice shows; hence the 990 cap |
| `weapon_xp` | Items | weapon items (native, 24045435): total `+0x13C` and within-level `+0x140` each grow by what the game grants × (multiplier − 1). Level `+0x130`, cap `+0x138` and next threshold `+0x148` are checked first; levelling is left to the game | slider 1–10 (default 3) | 1 | works (2026-10-03). ×3 on a sword kill: total 265 → 580 (105 granted, 315 added), levelled on the next kill |

The slider defaults in parentheses are the mod's starting values, not the game's.

A "field" is a plain property rather than a GAS attribute. It is written only after
its owner's class has been checked.

## What does not work from outside, and why

**Ignoring hits** (2026-10-03). Neither the engine's `bCanBeDamaged` nor clearing
`bGenerateOverlapEvents` on the hero's components stopped a blow. The game's
invincibility is decided by gameplay tags, and enemy hits are not plain overlaps;
data writes reach neither. The `Untouchable` effect is still in the code but has no
row. Details in [CHEATS-RESEARCH.md §6](CHEATS-RESEARCH.md).

**Coefficients and weapon stats** (2026-10-02, two test rounds). These attributes
held their value in memory but had no effect in play:

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
the effect executes; writing the attribute set's memory reaches neither. Those
effects need to be applied inside the game, as a GameplayEffect, which is Phase 1
(see [CHEATS-RESEARCH.md §4 C](CHEATS-RESEARCH.md) and `DECISIONS.md` D12).

`poise` was dropped as well: every attribute in `PoiseAttributeSet` is 0 on the
hero.

## Out of scope

Anything that unlocks achievements, and anything that hides the tool from the game.

## Teleport to what is followed (2026-10-05)

The Movement page lists what is followed (the auto guide's pick and each track that has a goal in
this region) with a Teleport button each. It uses the same write as the saved positions
(`Attached::teleport`: the root component's location and its `ComponentToWorld`, the velocity
cleared), landing `SHORT_OF` (150 cm) short of the target on the hero's side and `ABOVE` (120 cm)
over it: not inside what stands there (a chest, a person), and dropping onto its floor. Only in the
hero's region: another region's place has no ground loaded to land on. Confirmed in play by the user
(2026-10-05).

A closed-off target can trap the hero. At first the card said to save a position first; since
2026-10-05 (the user's suggestion) the engine keeps where the hero stood before a teleport in a slot of
its own (`Engine::before`; the first of a run of teleports, so going back is to where it started), and
the card's header shows "Back to last position" from a teleport until it is pressed
(`Request::GoBack`; the slot is emptied either way).
