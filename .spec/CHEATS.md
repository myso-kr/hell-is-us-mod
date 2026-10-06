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
| `frail` | Combat | every live enemy's `HealthAttributeSet.Health` held at 1, put back per enemy | toggle | 680–1130 (Tier 1), 1580–2630 (Tier 2) | works (2026-10-03; Tier 2 again 2026-10-06) |
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

## Targets that come and go (2026-10-06)

A code review found the enemy cheats' originals kept by address alone: switching enemy time or
frail enemies off wrote each recorded value back to any address still in the enemy list, and frail
wrote to the Health attribute's address recorded when the enemy was first seen. An enemy that died
and whose memory went to another object (another enemy, or anything else) would have been written
over. Now (`cheat/extras.rs`):

- each record carries the actor's class; a record whose actor is no longer listed, or whose class
  changed, is dropped every tick — there is nothing of a gone enemy to put back;
- frail finds the Health attribute again every tick and takes a new original when it is not the
  recorded one;
- release writes only when the actor is listed, of the recorded class, and (frail) still has the
  recorded attribute.

The ghost cheat's originals (one hero: address, TeamID, Faction) are kept in `ghost.txt` in the
data folder while it is on. The next run reads it: with the ghost off, the same hero gets its team
and faction back; another hero (another game) is left alone; the file is removed either way.

## Enemy health share (2026-10-06)

`enemy_health` (slider 0.1–1.0, `Effect::EnemyHealth`, extras.rs) cuts each enemy's
`HealthAttributeSet.Health` (base and current) to the share of its `HealthMax`, once, when first
seen. When the share moves, each enemy's health moves by the same ratio, so damage taken stays
taken: half lost at 10 % is half lost at 100 %. Switching it off (or closing the panel) puts each
back the same way. It does nothing while frail is on or its records are being put back.

The first version cut from the health it found and never healed back, on the grounds that
enemies were mid-fight. In play that left every enemy cut by one panel at 10 % for good: later
panels did not know them, and with the share at 100 % enemies still died in one hit. Frail
enemies had the same flaw (a panel started with frail on recorded 1 as the original); see
`extras.rs` for both fixes.
