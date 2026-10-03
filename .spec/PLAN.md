# Plan

What this project is, and the shape of the work. The current state is in STATUS.md, the next steps in
ROADMAP.md.

## What the game is

- *Hell Is Us* by Rogue Factor, published by Nacon. Steam app 1620730. Single-player only.
- Unreal Engine 5.5.4. The internal codename is *Charlie* (`CharliePlayerController`,
  `CharlieCharacterHero`).
- Stats are GAS attributes in native sets: `EnduranceAttributeSet`, `LymbicAttributeSet`,
  `PlayerAttributeSet` and others. Health is the ceiling on stamina (`EnduranceCap`).
- No Denuvo, no anti-cheat. The executable is readable on disk.
- By design there is no map, no compass and no quest markers. The game does ship compass assets, a
  country map used inside the APC, and a developer cheat manager (`CharlieCheatManager`).

## What hiumod is

A companion that runs beside the game as its own process and reads (and, for cheats, writes) the
game's memory. It modifies no game files.

- **Panel** — cheats grouped by tab, and tool pages: map, guide, quests, collect, saves, debug.
- **Overlays** — minimap and big map, compass, quest tracker, each hidden while a game menu is open.
- **Guide** — goals worked out from what the save knows, routes on the game's navmesh, the quest
  journal, missable good deeds, puzzle and vault answers (hidden until asked), enemy groups left,
  collectibles, achievements.
- **CLI** — `doctor` and its probes, `list`/`get`/`set`/`hold`/`restore`/`pose`.
- **Game data** — the survey tool reads the game's maps and text on the player's machine
  (SURVEY.md); nothing from the game is shipped.
- **Languages** — follows the game's language, 12 of them (I18N.md).

## Phases

| Phase | Attaches by | Gives | Status |
|---|---|---|---|
| **0** | external process, `ReadProcessMemory` / `WriteProcessMemory` | the panel, cheats, the CLI | working on build 24045435; 12 cheats verified in play |
| **0b** | the same process, click-through layered windows | minimap, big map, compass, tracker | working in game |
| **0c** | the same process, plus the survey tool over the game's paks | the guide: goals, routes, quests, puzzles, vaults, collectibles | working in game |
| 1 | RE-UE4SS (Nexus #43, UE 5.5 preset) | effects only reachable in-game: damage and cooldown cheats, achievement blocking | not started (ROADMAP §5) |

## The map, as it turned out

1. **No map image** — the hero's disc with the trail, markers and compass points (D11).
2. **Things nearby** — actors classified by class and drawn as SVG icons (D14).
3. **The game's map** — investigated and set aside: the only map image is the country map in the APC
   (MAP.md §3–4). Instead the map is drawn from the world: static-mesh outlines, terrain shading and
   contours from the heightfields (D16, MAP.md).

## Known gaps

- **Achievements are not blocked.**
- **Exclusive fullscreen hides the panel.** Windowed or borderless works.
- Damage, defence, healing, cooldowns and XP gain cannot be changed from outside (D12, Phase 1).
