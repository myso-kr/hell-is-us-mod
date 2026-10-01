# Plan

## What the game is

- *Hell Is Us* by Rogue Factor, published by Nacon. Steam AppID 1620730. It is
  single-player only.
- Unreal Engine 5.5.4. The internal codename is *Charlie* (`CharliePlayerController`,
  `CharlieCharacterHero`).
- Stats are GAS attributes in native sets: `EnduranceAttributeSet`,
  `LymbicAttributeSet`, `PlayerAttributeSet` and others. Health is the cap on
  stamina (`EnduranceCap`).
- There is no Denuvo and no anti-cheat. The executable is readable on disk.
- The game deliberately has no map or quest markers. It does ship compass and
  world-map assets (`CompassSumg`, `*WorldMapSoftTexture`) and a developer cheat
  manager (`CharlieCheatManager`).

## Phases

| Phase | Attaches by | Gives | Status |
|---|---|---|---|
| **0** | external process, `ReadProcessMemory`/`WriteProcessMemory` | cheat panel, `pose` | code done, **not yet run against the game** |
| 0b | same process, a second click-through window | minimap: position, heading, trail, markers | planned |
| 1 | RE-UE4SS (Nexus #43, UE 5.5 preset) | in-game UI, developer cheat manager, the game's own map textures, achievement blocking | later |

## The minimap, step by step

1. **No map image.** Draw a north-up or heading-up disc around the hero, with the
   path walked, player-placed markers and compass points. This needs only `pose`.
2. **Things nearby.** Walk `GUObjectArray` for actors by class name (enemies,
   chests, teleport markers) and draw their positions.
3. **The game's map.** Extract the world-map textures and their world-to-map
   transform with FModel. The AES key is extracted locally. Draw beneath the trail.
   Extracted assets are never committed or redistributed.

## Known gaps in phase 0

- **Achievements are not blocked.**
- **Exclusive fullscreen hides the panel.** Windowed or borderless works.
- **Inventory counts and currency** are not GAS attributes and are not covered yet.
