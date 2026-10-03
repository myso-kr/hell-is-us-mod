# Changelog

All notable changes to hiumod. Versions follow [semantic versioning](https://semver.org/).

Every release records the Steam build it was verified on. Steam's `buildid` is in
`steamapps\appmanifest_1620730.acf`, and `hiumod doctor` prints it. What the tool relies on in the
game, and what an update can break, is in [`.spec/ANCHORS.md`](.spec/ANCHORS.md).

## Unreleased — 0.1.0

The first release. Verified on Steam build **24045435**, where `hiumod doctor` passes every check.

### Attaching

- Runs beside the game as its own process and reads its memory; no game file is modified.
- No per-build offsets for the engine: the name pool and `GEngine` are found on every attach, the
  FField layout is chosen from known candidates, and the path to the hero is learned from the
  engine's reflection by property name.
- Writes go only to the hero, behind a gate that closes for loading screens, menus and cinematics;
  held cheats pause while it is closed. Originals are written to disk before anything changes and are
  put back on switch-off, on closing the panel, or with `hiumod restore`.
- A warning when the game's build differs from the one the release was verified on.

### Cheats

Twelve, each seen working in play: refilled health (`god`) and stamina, walking speed, the hero's own
speed, game speed, enemy speed, frail enemies, consumables that do not run out, shard stacks,
a weapon-XP multiplier, five saved positions, and a ghost mode for getting past enemies. Three more
(Lymbic energy, Lymbic cost, skill cooldown) are in the table but not yet tried. Damage, defence,
healing, cooldown and XP coefficients cannot be changed from outside the game and are not offered —
see [`.spec/CHEATS.md`](.spec/CHEATS.md).

### Map

- A click-through minimap and a big map (F9 cycles minimap → big map → off), hidden while a game menu
  is open.
- Drawn from the world itself: walls and floors from the level's static meshes, terrain shading and
  contour lines from the landscape heightfields, floors above and below faded.
- Your trail, 26 kinds of icons (enemies by family, items by sort, loot, NPCs, doors, puzzles, vaults,
  save points, enemy groups) and 24 kinds of pins you place yourself (F6); each kind can be hidden.
- An outline style with a clear background for the big map, and adjustable radius, opacity and icon
  size.

### Guide

- A compass strip at the top of the screen (F10) with distance and height to the target.
- Goals worked out from what your save knows and what each place in the world still gives; F11 moves
  to the next one, and goals finished out of sight can be skipped.
- Walking routes on the game's own navmesh, falling back to a grid round collision shapes, deadly
  water and slopes over 45°. Goals behind closed doors and puzzles lead to what opens them first.

### Quests and collections

- A quest tracker and journal: main quests, good deeds, mysteries and timeloops by their real names,
  following the save live. Follow a quest to be guided to what it still needs, here and in other
  regions.
- Good deeds you can still miss, with the story point they fail at, and the advised keystone order.
- Hand-overs: NPCs who want an item you carry.
- Every dial, keypad and item-placement puzzle with its answer, the six Vaults of Forbidden Knowledge
  with their codes drawn as the game's symbols, and the enemy groups left per region for the
  every-Hollow achievement — every answer hidden until you ask.
- Collectibles per region, NPCs with more to tell, and your Steam achievements with progress.
- Each game save is copied to `Mods\backups` (the last 20).

### Game data

- The guide's data is read from your own game files by the bundled survey tool — automatically, once
  you control the hero, and again when the Steam build changes. Nothing from the game is shipped.
- When the .NET 8 runtime the survey tool needs is missing, the panel offers to install it: through
  winget, or without administrator rights into the game's `Mods` folder.

### Languages

- Follows the game's text language, in all twelve it ships: English, Korean, Japanese, Simplified
  Chinese, German, French, Spanish, Italian, Polish, Brazilian Portuguese, Russian and Turkish.
- Names of places, people and items come from the game's own translations on your machine; the mod's
  own text was localized per language against the game's terms.

### Panel and command line

- A panel (backquote key) with cheats by tab and pages for the map, guide, quests, collections, saves
  and debugging, and a drop-down console that runs the CLI.
- `hiumod doctor` (with `inspect`, `find`, `dump`, `watch`, `scan`, `saves`, `survey`), `list`, `get`,
  `set`, `hold`, `restore`, `pose`.

### Project

- A website in twelve languages with an introduction video, and a release workflow that builds the
  zip on Windows, checks that nothing from the game is in it, and publishes a SHA-256.
