# Anchors

This tool pins no build-specific numbers for the engine chain. Below is what it
finds at run time (`src/unreal/anchors.rs`), and what a game update can break.

## Found on every attach

| What | How | Breaks how |
|---|---|---|
| FNamePool | the one pool in the image's writable data whose block 0 starts `None`, `ByteProperty` | an engine upgrade changes the allocator layout |
| GEngine | the one global in writable data pointing at a non-default object whose class derives from `GameEngine` | two engine objects, or none yet (too early in start-up) |
| FField layout | of the candidates in `names::LAYOUTS` (four as of this writing), the one under which `GameEngine.GameInstance` leads to a `GameInstance` | an engine upgrade moves FField members: add a candidate |

Anything that is not unique is refused, never guessed.

## Learned once the hero is in play

`GameInstance`, `LocalPlayers`, `PlayerController`, `Pawn`, `RootComponent`,
`RelativeLocation`, `ControlRotation` and `SpawnedAttributes` are found by
property name. The ability system is the pawn's property whose object derives from
`AbilitySystemComponent`. A rename shows up in `doctor` as `no property X`.

The hero is a pawn whose class derives from `CharlieCharacterHero`
(`player::HERO` in `src/unreal/player.rs`).

## Engine layout constants (`src/unreal/names.rs`)

UObject `ClassPrivate` +0x10, `NamePrivate` +0x18, `OuterPrivate` +0x20. UStruct
`SuperStruct` +0x40, `ChildProperties` +0x50. These change only with an engine
upgrade.

Some cheats do rely on native, build-specific offsets past the reflected fields
(inventory stack count, weapon experience); those are listed in `CHEATS.md` and
checked for a plausible shape before every write.

## Builds

| Steam build | Status |
|---|---|
| 24045435 (2026-10-01) | `doctor` all ok (2026-10-02): name pool +0x9220CC0, GEngine +0x947CF10, FField layout offset +0x44, chain GameInstance 0x11F8 → LocalPlayers 0x38 → PlayerController 0x30 → Pawn 0x2E8 → ASC 0x688 → SpawnedAttributes 0x1088. Cheats verified in play on this build: see `CHEATS.md` |

## Verifying a build

1. Load a save, then run `hiumod doctor`. Every line should say `ok`.
2. Run `hiumod get EnduranceCap Endurance LymbicEnergy` and compare the values
   against the HUD.
3. Run `hiumod pose` and walk. x/y should move, and yaw should follow the camera.
4. Add the build to the table above.
