# Cheat expansion research (2026-10-03)

The question (from the user): what further cheats could be built, based on web
research, and whether a UE-side extension of `doctor` would help. Two lines of
inquiry: existing trainers and Cheat Engine tables (web), and read-only probes of
the game's memory on build 24045435.

The current state of every cheat is in [CHEATS.md](CHEATS.md). This document keeps
the research behind it.

## 1. Web: what existing trainers offer

FearlessRevolution blocks direct access, so its threads and seven `.CT` files were
read through Wayback Machine copies (Tuuuup! v2 for Steam 1.5.40 / exe 5.5.4.0,
ndck1, VampTY, Sianz, matthew80, DarkMango). Offsets change with every patch, but
the names stay the same.

| Feature | Offered by | Implementation hint |
|---|---|---|
| Infinite health / god mode | WeMod, CH, PLITCH, DarkMango, Tuuuup! | **HP = `EnduranceCap`**, stamina = `Endurance` (same as ours). Tuuuup! hooks the attribute-update function and zeroes decreases for the player's set only |
| One-hit kill / easy kills | WeMod, PLITCH, DarkMango, Tuuuup! | sets non-player Endurance to 10 |
| Infinite items / free use | WeMod, CH, PLITCH, DarkMango | item +0x48 = quantity. Code side: the quantity subtraction, AOB `41 2B C1 45 3B C8`. Definition +0xD8 `QuantityMax`, +0xDC `bCanMultiStack` |
| Set quantity of selected item | WeMod, PLITCH, Tuuuup!, ndck1 | grabs the item last inspected in the inventory |
| Infinite shards / free crafting | CH, Tuuuup! | shards are ordinary stack items (same quantity field) |
| Pickup multiplier | Tuuuup! | multiplies positive deltas in the add/remove function (code hook) |
| Weapon XP multiplier | CH, PLITCH, Tuuuup! | AOB `41 03 8E 3C 01 00 00`; weapon instance +0x13C XP, +0x148 "max XP" (measured since: next-level threshold) |
| Weapon grade, module slots | Tuuuup!, ndck1 | definition +0x1A0 `Grade`, +0x1C8 `MaxModuleSlots` |
| Damage / defence multipliers | CH (damage multiplier), many requests | `MeleeDamageBoostCoefficient`, `WeaponAttackPower`, `GlobalDefenseCoefficient`, `DamageTakenBoostCoefficient`, … |
| Enemy slow / freeze | CH, PLITCH, DarkMango, Tuuuup! | non-player pawns' `CustomTimeDilation` |
| Game speed | all | global time dilation (WorldSettings `TimeDilation`) |
| Fly / noclip | CH, PLITCH | `CharacterMovement.MovementMode`, collision off |
| Save / restore position (5 slots) | CH | actor location |
| Parry / invincibility window, animation rate | requests / VampTY's attribute list | `InvincibilityWindowModifierCoefficient`, `*AnimPlayRateMultiplier` |
| Drone cooldown | matthew80, Tuuuup! | `DroneDockedAbilityCooldownModifierCoefficient` (Tuuuup! writes the float at `[rbx+0x110]+0x40`; structure unknown) |

Sources: [WeMod](https://community.wemod.com/t/hell-is-us-cheats-and-trainer-for-steam/370869) ·
[MrAntiFun](https://mrantifun.net/threads/hell-is-us-trainer.26160/) ·
[Cheat Happens](https://www.cheathappens.com/81830-PC-Hell-is-Us-trainer) ·
[PLITCH](https://www.plitch.com/en/games/hell-is-us-385402285381128192) ·
[FLiNG](https://www.fling-trainer.com/hell-is-us-trainer/) (16 options, names not confirmed) ·
FearlessRevolution [Tuuuup!](https://fearlessrevolution.com/viewtopic.php?t=36512),
[ndck1](https://fearlessrevolution.com/viewtopic.php?t=36338),
[demo thread](https://fearlessrevolution.com/viewtopic.php?t=35282),
[DarkMango](https://fearlessrevolution.com/viewtopic.php?f=5&t=36430) (read via web.archive.org).

Game mechanics (Fextralife, Game8):

- Shards are both currency and crafting material: Neutral, Ecstasy, Grief, Rage
  and Terror, each in small, medium and large.
- Weapon grades 1–5 (three glyph slots from grade 2); armour up to grade 4.
- Consumables: medicine (instant or over time, with a carry limit), Lymbic
  chargers/batteries, suppressors, upgrade books.
- Four drone skill slots.

## 2. Memory probes (read-only, confirmed on this build)

- **Eight attribute sets**, in the order of the ASC's `SpawnedAttributes`:
  Endurance, Lymbic, Damage, Weapon, Locomotion, Poise, PlayerDefense, Player.
  Sample values: `EnduranceCap 5000 / EnduranceMax 5000`, `LymbicEnergy 554/554`,
  `WeaponAttackPower 299`, `CurrentMaxWalkingSpeed 450`,
  `AbilityCooldownModifierCoefficient 0.1` (with the cooldown cheat on),
  `DefensivePower 0/50`.
- **Inventory**: `CharlieInventoryLoadoutSubsystem` → `CharlieInventory`
  (Owner = the hero) → `Items` TArray (53 entries). Item classes:
  `CharlieInventory{Item, UseableItem, UseableCooldownReductionItem,
  UseableShowImageItem, WeaponItem, GearItem, ShardItem, DroneItem,
  DroneModuleItem, CosmeticItem, DroneCosmeticItem, LevelSequenceItem}`. The only
  reflected field is `ItemData` (+0x40, a data asset).
- **Stack quantity = item +0x48** (u32, native, i.e. `ItemData` + 8). It matches
  the save's `Quantity`: `Cons_MedicineCivilianT01` 9, `Craft_Shards_Neutral_G01`
  122, `…Rage_G01` 75, `…Terror_G01` 33, `…Ecstasy_G01` 18, `Cons_DroneBoosterT01`
  6, `Cons_LymbicChargerT01` 6.
- Save file: an element of `CharlieSaveGame.Player.Inventory.Items` is 176 bytes:
  Guid, ItemData (soft reference), **Quantity at +0x38**, ItemPlacementSlotIdx,
  BoundModules, **WeaponCurrentXP at +0x90**, bIsNew, DroneCurrentCosmeticItemID,
  ItemPreferenceState. This is a copy taken at save time; the live object is
  authoritative.
- Owned by the hero: `CharlieHeroAbilitySystemComponent`,
  `DamageOverTimeComponent`, `ParryComponent`, `HeroMovementComponent`, abilities
  (`StoryHero_*_GA_C`, `Drone_*_GA_C`, gear `DefensiveGear_*_GA_C`), and more.

## 3. The key constraint: why coefficient cheats failed (D12)

Play-testing on 2026-10-02 (`verify.txt`) showed that values the game **reads
directly** (Endurance, the movement component, time dilation) respond to memory
writes. Values **GAS recomputes** (`*Coefficient`, weapon attack power) held in
memory but had no effect: the result is rebuilt from the aggregator's base value
plus modifiers, or an ability captures the value when it starts. This is why the
published tables use **code hooks (AOB)** rather than data writes. The full list of
attributes tried is in [CHEATS.md](CHEATS.md).

## 4. Proposed phases

### A. Possible with the current approach (external process, data writes), low risk

| Cheat | Method | Note |
|---|---|---|
| Infinite consumables | keep `Useable*` item quantity (+0x48) at or above its value when switched on | capped by the definition's `QuantityMax` |
| Shards max / set | write `ShardItem` quantity (panel: a value per kind) | currency; a set-once kind |
| One-hit kill (enemy health 1) | lower the health of enemy actors (the scanner already knows them) | needed to confirm where enemy health lives (it turned out to be `HealthAttributeSet`, §4.1) |
| Enemy slow / freeze | enemy pawns' `CustomTimeDilation` (0.1–1) | same mechanism as the hero's time dilation, already proven |
| Game speed | WorldSettings `TimeDilation` | global |
| Save / restore position; teleport on map click | write the root component's location | physics and streaming need checking |
| Weapon XP | weapon item +0x13C XP (+0x148 threshold) | offsets were for 1.5.40; re-confirm by reflection and measurement |
| Fly / noclip | `CharacterMovement.MovementMode` = Flying, capsule collision off | needs up/down input handling |

### B. Code patches (AOB), as the tables do

NOP the quantity subtraction (`41 2B C1 45 3B C8`), multiply weapon XP
(`41 03 8E 3C 01 00 00`), hook the attribute update. Rewriting code bytes with
`WriteProcessMemory` is possible from an external process too, but patterns can
break with each patch, and it needs restore and verification machinery. Only for
what A cannot do.

### C. An in-game UE module (the "doctor extension")

- **UE4SS works with this game** (a UE 5.5 preconfigured build,
  [Nexus 43](https://www.nexusmods.com/hellisus/mods/43)). There is no public SDK
  dump, and reflection is not obfuscated.
- Uses: (1) **extending `doctor`**: UE4SS's object dumper and UHT header generation
  would confirm native fields (quantity +0x48, XP +0x13C, …) by name, and Live View
  would verify them. (2) **The proper route for coefficient cheats**: calling a
  UFunction such as the ASC's `BP_ApplyGameplayEffectToSelf` from Lua to apply
  damage, defence, XP and cooldown multipliers as GameplayEffects. GAS then does the
  calculation itself, which gets past the D12 wall.
- Cost: changes D1 (no modification of game files), requires installing the
  `dwmapi.dll` proxy, and a crash can take the game down with it. It would talk to
  the current panel through files and a named pipe.

## 4.1 Phase A as implemented (2026-10-03)

All of the following were built and then verified in play the same day (status in
[CHEATS.md](CHEATS.md)); fly/noclip and map-click teleport were not built.

- `game_speed`: WorldSettings `TimeDilation`, the existing plain-field mechanism;
  the original value is recorded on disk.
- `enemy_time` and `frail` (`src/cheat/extras.rs`): the scanner's live enemies,
  with each enemy's original value kept in memory.
- `stock` and `shards`: the inventory. `Attached::inventory()` finds it once in
  GUObjectArray and checks Owner = hero every time. Quantity is `ItemData` + 8; a
  value above `QuantityMax` is taken as a moved layout and skipped.
- Five position slots.
- `weapon_xp`: built afterwards from `doctor watch` measurements; see CHEATS.md for
  the native offsets.

Findings while building:

- Enemy health is `HealthAttributeSet.Health` / `HealthMax` (Tier 1: 680–1130), a
  different set from the hero's Endurance.
- Consumables: a stack that reaches 0 is removed from the inventory and cannot be
  put back, so `stock` holds a floor of 2.
- Shards: only kinds the player already has a stack of can be written (a missing
  kind cannot be created). The game's own cap is 999.

## 5. Recommendations

1. **Phase A first**: infinite consumables, set shards, enemy slow/freeze, game
   speed, one-hit kill, save/restore position. All of these sit on reflection and
   the scanner that already exist, and a failure only needs the value put back.
   (Done, §4.1.)
2. Damage, defence and XP multipliers: the proper route is **Phase C (UE4SS +
   GameplayEffect)**. If wanted, start small with the `doctor` extension (dump and
   verify).
3. Phase B (code patches) only for what A and C cannot do, with pattern
   verification and automatic restore.

## 6. Ghost and ignoring hits (2026-10-03)

The question (from the user): can a ghost that enemies do not notice, and a god
mode that ignores hits, be added? Investigated with `doctor find` / `inspect`.

### Ghost: teams

- `CharlieCharacter.TeamID` (GenericTeamId, +0x67C) and `Faction` (+0x67D): the
  hero is 1/1, enemies (the HollowWalker family) are 2/2. `HazeGhost` and
  `CharlieActorGAS` carry the same fields.
- UE AI perception filters targets by team affiliation (hostile / neutral /
  friendly), via `AISenseConfig_Sight/Hearing.DetectionByAffiliation`, so putting
  the hero on the enemies' team might make them treat it as a friend.
- **Result:** setting both `TeamID` and `Faction` to the enemies' values (from a
  nearby enemy, or 2/2 if none) makes enemies ignore the hero, but the hero's blows
  no longer land either. `Faction` decides both perception and damage, and the
  relationship is symmetric: being unseen while still able to hit is not possible
  with data writes. `ghost` ships as "Ghost (for exploring — you can't attack while
  it's on)", verified. Turning it off restores the original values; if the panel is
  killed, the change stays until the game is reloaded.
- History: a second version wrote `TeamID` alone, on the theory that perception
  uses the team and damage uses `Faction`. Enemies still attacked, so it was
  reverted to writing both.
- Other leads, not pursued: `HearingStimuliEmitterRuneComponent` and
  `FootstepEmitterRuneComponent` (noise), `CharlieCombatStateHandler.AggroedEntities`
  (enemies already aggroed), `LymbicEntitySensesParameters.AggroRangeSightRatio`.

### Ignoring hits

- The game's invincibility is decided by **gameplay tags**
  (`DamageDealerThrowableComponent.bIgnoreInvincibilityTags`,
  `GameplayEffect.GrantedApplicationImmunityTags`). Adding a tag means inserting an
  element into the ASC's tag-count map and container, which needs an allocation
  inside the game, so it is not done with data writes.
- `DamageDefinition.bShouldTriggerHitReaction` belongs to the data assets (enemy
  attack definitions); changing it would change every hit reaction at once.
- **Result:** both attempts had no effect, and the toggle was removed (the
  `Untouchable` effect remains in the code with no row):
  - v1 cleared the engine's `AActor.bCanBeDamaged` (+0x5A, bit 0x04, confirmed by
    reading the FBoolProperty's ByteMask). The game's damage path does not read it.
  - v2 cleared `bGenerateOverlapEvents` (bit at +0x25B) on every PrimitiveComponent
    the hero owns, since enemy attacks use `DamageDealerBox/Capsule/SphereComponent`
    shapes and overlaps only happen when both sides generate them (side effect: the
    hero's own overlap triggers, such as doors, pickup range and lethal water, stop
    responding). Enemy hits are not overlaps (traces and tags).
- Ignoring hits needs either tags (an in-game allocation) or a code patch
  (Phase B).
- The existing `god` (refilling the health cap) is take-damage-but-never-die:
  hit stagger and knockdowns remain.
