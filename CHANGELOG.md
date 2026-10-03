# Changelog

All notable changes to hiumod. Versions follow [semantic versioning](https://semver.org/).

Every release records the Steam build it was verified on. Steam's `buildid` is in
`steamapps\appmanifest_1620730.acf`, and `hiumod doctor` prints it. What the tool relies on in the
game, and what an update can break, is in [`.spec/ANCHORS.md`](.spec/ANCHORS.md).

## Unreleased — 0.2.0

### Panel

- A **Now** page, where the panel opens after a break: what was going on last time, the good deeds
  about to be missed, what is left in this region by kind, and which other regions are worth a trip.
- A **Puzzles** page: puzzles nearby, Lymbic locks, the vaults and the full puzzle list, moved out of
  the Guide and Collect pages. No page has more than four cards now.
- A **Help** page beside Now at the top of the sidebar: first steps, every page in a line (press to
  open it), the keys as set, and what to do when something is off.
- Dashboard look: the Now page opens on a hero with a live map of the region, its name and six KPI
  tiles, and other regions as a bar chart; Collect and Puzzles open on progress rings; collections
  and enemy groups have progress bars; good deeds, mysteries and timeloops are rings.
- The same look on every page: the Guide target as a large panel with its distance; quests with a
  progress bar each; Clues and Help with KPI tiles and numbered steps; cheats, saves and the debug
  checks as aligned rows with status chips; achievements with progress bars; Map settings as form
  rows. Long explanations moved into hover text.
- States read at a glance: chips for "opens now", covered or short, soon or later, a vault's state;
  keys as keycaps; card headers without the accent rail.
- One look per role: press a line to be guided there (no separate Guide buttons), one small
  reveal button at most per line, normal-size action buttons, and switches instead of checkboxes,
  sized to the text beside them.
- The sidebar lists the tools first and the cheat groups below; both fold under their heading, and
  the cheats start folded, their heading counting the cheats on.

### Guide

- **Region ledger:** counts of quest places, hand-overs, puzzles, Lymbic locks, vault doors,
  collectibles, enemy groups and NPCs with more to tell, per region — counts only, nothing named.
- **Trip planner:** other regions sorted by what you can do there right now.
- **Lymbic locks:** the rods each lock takes and which you hold; each missing rod shows how far its
  pickup is (up or down too); press a lock or a missing rod to be guided there.
- **Previously:** where the last session ended and the quest you were following.
- **Height:** every distance in the panel's lists shows ↑/↓ when the place is 3 m or more above or
  below you.
- **Clues:** a new page with the followed quest's Datapad entries — who and what it involves, and
  what you know of each — and a search over everything you know and hold, in the game's language.
  The game's text is read once more for it (`facts.tsv`).
- **Shard budget:** the achievements card shows what *Good Vibrations*, *Accessorizing* and *To the
  Teeth* still cost in shards by the game's own recipes, from the weapons and gear you hold, whether
  your shards cover it, and how many timeloops are still open. The game's crafting table is read once
  more for it (`recipes.json`, `doctor tables`).
- **Completion board:** opening a sort in the collection card shows how many are left in each other
  region, most first.

## 0.1.0 — 2026-10-03

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
