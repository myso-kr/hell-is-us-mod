# Doctor extensions — tools for looking up game data (2026-10-03)

Request: make the memory-scanning `doctor` query in-game data more thoroughly, using what Unreal
itself describes. These are lookup tools, not a way to apply effects.

## Decision: no UE4SS, from outside

UE4SS's lookup features (object dump, Live View, SDK headers) come from the engine's **reflection
data** — the same data this crate already walks from outside (FNamePool, GUObjectArray,
UStruct/FField). Outside reflection (native fields, e.g. an item's quantity at +0x48) a UE4SS dump
does not know the names either. So D1 (game files untouched) stays, and fields outside reflection
are found by **showing the gaps, watching for changes and scanning for values**.

## Commands (all read-only; results are also saved under `Mods\doctor\`)

| Command | What it does |
|---|---|
| `doctor inspect <target> [depth] [gaps]` | Every reflected field of a live object, as a typed value (structs expanded, the first 8 array elements, pointers followed to `depth`). `gaps`: the native bytes between fields, as u32 (also read as float) |
| `doctor find <text>` | Searches property names across all classes and structs (≈14,000): `Owner.Prop type @+offset (size)` |
| `doctor dump [prefix…]` | An SDK-style listing (default prefixes `Charlie`, `Story` → 471 entries, `sdk.txt`), including struct and array element types |
| `doctor watch <target> [seconds]` | Compares the object's memory every 0.1 s and prints changed fields by name — native ones by offset — with timestamps |
| `doctor profile [seconds]` | Runs the panel's worker steps (ten a second, no toggles, so nothing is written) for 30 s or `seconds`, and prints each phase's calls, mean and worst ms (`prof.rs`), the first 8 s apart as the warm-up |
| `doctor scan <target> <value>` / `scan next <value>` | Where a value (u32/f32/f64) sits in an object; after changing the value in game, `scan next` keeps only the places that still match |

Targets: `hero`, `controller`, `asc`, `sets[:N]`, `inventory`, `items[:N]`, `save` (the latest
`CharlieSaveGame`), `world` (WorldSettings), `enemy[:N]`, `0x<address>`, or a class name `[:N]`.

## Measured (build 24045435)

- `inspect items:3 0 gaps` → `CharlieInventoryUseableItem` shows `· native +0x48..: 0x9 …` — the
  quantity 9 is the first word outside reflection.
- `scan items:3 9` → 1 place (+0x48); `scan next 9` → still 1.
- `find Quantity` → 18 hits (`InventoryItem.Quantity`, `ItemData.QuantityMax @+0xd8`,
  `CraftIngredient.Quantity`, …).
- `dump` → 471 classes and structs, 2,078 lines.

## Case study: weapon experience (2026-10-03)

1. `find WeaponCurrentXP` → the save's `CharlieInventoryItemState.WeaponCurrentXP` (one-handed sword
   90, twin axes 1560).
2. `scan CharlieInventoryWeaponItem:0 90` → 4 places; twin axes 1560 → 2 places (+0x13C, +0x144).
3. `inspect … gaps` comparing the two weapons' tails: for the twin axes the next-level threshold at
   +0x148 is 0 (grade cap — kills no longer raise it).
4. `watch` while killing with the one-handed sword:
   - +0x13C 90 → 265 (cumulative XP);
   - +0x140 90 → 5 (XP within the level);
   - +0x148 260 → 520 (next threshold);
   - +0x130 0 → 1 (level);
   - +0x138 3 (cap);
   - +0x14C / +0x150 700 → 725 (stats).

   This became the `weapon_xp` cheat (`src/cheat/cheats.rs`, `src/cheat/extras.rs`).

## How to find a new cheat

1. `find` for candidate names.
2. `inspect <target> 1 gaps` for the values and the gaps.
3. Move the value in game while running `watch`, or use `scan` / `scan next`.
4. Express the offset relative to a reflected field (e.g. `ItemData` + 8) in code.
