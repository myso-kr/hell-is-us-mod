# Changelog

All notable changes to hiumod. Versions follow [semantic versioning](https://semver.org/).

Every release records the Steam build it was verified on. Steam's `buildid` is in
`steamapps\appmanifest_1620730.acf`, and `hiumod doctor` prints it. What the tool relies on in the
game, and what an update can break, is in [`.spec/ANCHORS.md`](.spec/ANCHORS.md).

## Unreleased

### Changed (performance)

- The minimap and the big map keep up with the hero: the overlay reads where the hero stands every
  frame, instead of waiting for the rest of the game's reading (up to a second behind before).
- The panel's reading of the game takes a fifth of the time it did (10.7 ms a step on average, from
  55): the walks over every object rest between passes and read a page at a time, and names and
  class layouts already read are kept.
- The big map's window is only as wide as the map's circle, and a frame that would show nothing new
  is not drawn again: about 40 % less work a frame, and next to none while the hero stands.
- The maps' ground shows from the start: each world's ground and obstacles are kept in
  `Mods\cache\`, as a game keeps its shader cache, instead of waiting about 6 s for the first
  reading of the game. The ground walked is gathered, so the maps show it even where the game has
  not loaded it.
- `doctor profile [seconds]` shows where the panel's reading of the game spends its time.

### Added

- Choice puzzles on the Puzzles page: the Watcher's Nest's ceramic flowers and the Eye of God's
  orbs, each with its sets of slots and how they stand (placed right, not yet, something in a wrong
  slot), the game's riddle, and per set the game's own clue and then the right slot, one step at a
  time. The wrong slots no longer show a stand-in item as their answer.
- The guide's target is ringed in the game's own view when it is near (within 60 m) and in sight, with
  its distance: a choice puzzle's answer shows exactly which groove or statue.

- The Guide page's area map: north up round the hero as wide as the big map's radius, with what
  is followed in its colours and their routes. Pointing at a place says what it is and how far;
  pressing it follows it, or lets it go. Beside it, the journey: how far was walked this run and in
  each region, and what was followed and reached.
- The Map page's presets (Explore, Fight, Minimal, Photos) set the display, radius, layers and
  icons in one press, under both maps' previews side by side.

- Follow up to five places at once besides the auto guide: every Guide button across the panel adds
  its place (press again to focus it, again to let it go), each with its own colour, route, compass
  mark and ring in the game's view. The Guide page lists them all, with Focus and Let go; the
  next-goal key moves the focus. Places followed are kept across runs and let go by themselves once
  done.
- Follow several quests at once: the Quests page's Follow button adds a quest, which then goes to its
  own next goal with its own colour, and on to the next. The Guide page shows each with the goal it
  is at, the Now page lists them under the story followed, and the quest tracker in the game shows
  them in their colours.
- One way to be guided anywhere: every row that guided now ends in the same Follow toggle, and the
  row itself only shows. No quest is picked for the guide any more: the auto guide keeps to the main
  story; a quest picked before is followed instead.
- The Guide page has two cards, the auto guide and what is followed by hand; its quest, secret and
  clue toggles are gone, every kind of goal is always shown.

### Fixed

- A person is pinned where they are: Victor Gaz, already at the forge in Jova, was still pinned where
  he is first met, and some people were listed twice.

## 0.3.0 — 2026-10-04

Verified on Steam build **24045435**, where `hiumod doctor` passes every check.

### Changed

- The console opens and closes with <kbd>Ctrl</kbd>+<kbd>&#96;</kbd> (not Shift, the game's sprint),
  instead of a header button that appeared only over a game menu. The header shows the panel's and
  the console's keys as keycaps, written as the keys pressed (` and Ctrl+`: "~" is another key on
  some layouts).
- The key pickers no longer offer the game's <kbd>F1</kbd> (HUD) and <kbd>F7</kbd> (photo mode) or
  Steam's <kbd>F12</kbd> (screenshot); a saved key on one goes back to the defaults.
- The console opens on its own: with the panel hidden it stays hidden, and hiding the panel no longer
  closes the console.
- The panel is smaller: at most 55 % of the screen's width and 72 % of its height (two columns of
  cards on 1080p, three on 1440p and wider).
- New default keys, next to the game's F1: <kbd>F2</kbd> map display, <kbd>F3</kbd> compass,
  <kbd>F4</kbd> next goal, <kbd>F5</kbd> pin (were F9, F10, F11, F6). If you never changed them, they
  move to the new keys by themselves; keys you chose stay.

### Added

- A **Settings** page that asks, in four short questions built on what players run into, what the
  mod may do: the map and navigation, where hidden things are, answers and spoilers, cheats. Until
  you answer, the mod adds nothing to the game; pages you have not allowed stay in the sidebar,
  greyed. Your answers can be changed any time.

### Changed (panel)

- The Now page is built around what players find hardest: the story you follow with its next goal
  and last clues (and where you left off), what you can do right here (hand-overs, locks your rods
  open, the nearest places), the side stories under way, the regions worth a trip and why, and what
  is about to be missed; the story card spans two columns.
- Long achievement lists scroll inside their card.
- Quests, Clues, Puzzles and Collect lay their long lists out wide: the journal, a quest's clues and
  this region's puzzles two columns wide beside a short card, the NPCs with more to tell, the vaults
  and the achievements across the page.
- The panel's background moves softly: contour lines drifting and a route finding its way, as in the
  introduction video (can be turned off on the Settings page). Cards are thin, slightly see-through
  glass.

### Fixed

- Haze links now show in fights: a Haze is spawned when the fight starts, and its link to every
  Hollow Walker it keeps alive is drawn from then on (confirmed in play: one Haze holding four).

## 0.2.1 — 2026-10-04

Verified on Steam build **24045435**, where `hiumod doctor` passes every check.

### Fixed

- Updating from the footer stopped after the download with "update failed": the zip's unpacker
  (`tar.exe`) could not start from the panel, which runs without a console. It now gets its own
  input and output, as the download does. From 0.2.0 itself the button still fails: download this
  release by hand once (the footer's "What's new" link opens it); updates work from 0.2.1 on.

## 0.2.0 — 2026-10-04

Verified on Steam build **24045435**, where `hiumod doctor` passes every check.

### Panel

- A **Now** page, where the panel opens after a break: what was going on last time, the good deeds
  about to be missed, what is left in this region by kind, and which other regions are worth a trip.
- A **Puzzles** page: this region's puzzles (those nearby first, with the answer the game holds),
  Lymbic locks and the vaults, moved out of the Guide and Collect pages.
- A **Help** page beside Now at the top of the sidebar: first steps, every page in a line (press to
  open it), the keys as set, and what to do when something is off.
- Dashboard look: the Now page opens on a hero with a live map of the region, its name and six KPI
  tiles, and other regions as a bar chart; Collect and Puzzles open on progress rings; collections
  and enemy groups have progress bars; good deeds, mysteries and timeloops are rings.
- The same look on every page: the Guide target as a large panel with its distance; quests with a
  progress bar each; Clues and Help with KPI tiles and numbered steps; cheats, saves and the debug
  checks as aligned rows with status chips; achievements with progress bars; Map settings as form
  rows. Long explanations moved into hover text.
- The panel runs once: starting it again brings the running one to the front. It has a tray icon
  (click to show or hide, right-click to quit, which restores the originals as × does).
- The panel stays inside the screen when moved, restored or grown; the console's command line is no
  longer cut off; the console, and its button in the panel's header, show only over a game menu.
- The Map page has a legend card of the maps' lines and colours, as drawn with the settings now.
- The big map covers the whole screen, centred on the hero and fading out toward the edges; its
  radius, at most 400 m (the minimap's too), is measured to the screen's short side with a margin above and below, and
  its lines, icons and dots are drawn at full resolution. The minimap and big map cards
  each preview their own map.
- The big map moves smoothly: the hero's position glides between readings, the map is drawn about
  30 times a second, and a frame costs a tenth of what it did (its ground slides instead of being
  redrawn, and is redrawn on a thread of its own; the trail draws only its latest stretch).
- The panel and its tray icon carry the mod's mark instead of Windows' generic icon.
- The console and its header button appear as soon as a game menu opens, without clicking back and
  forth; minimising the panel hides it and gives the keyboard back to the game.
- A footer across the panel: restore all, "Keep settings", the version, an update from GitHub
  releases (checked at start; downloaded and applied when you ask), GitHub and the copyright.
- A splash while the panel starts: it shows until the game, its data and your kept cheats are ready
  and the panel has laid itself out, so the panel appears finished.
- The pages are regrouped so each answers one question: NPCs with more to tell moved to Clues, pins
  to Guide, the shard budget is its own card on Collect, the nearby puzzles and the puzzle list are
  one card, the secrets ring shows only on Quests, and the sidebar groups the pages as Play, Finding
  the way and System.
- Hazes are linked on the maps: a thin violet line from each Haze to the Hollow Walkers it keeps
  alive, so you can see which to kill first (in the legend while enemies are shown).
- Places the game names only by an internal trigger ("AcasaHermitTombFullyOpened") are named in the
  game's language, from its own text: the Datapad entry their facts are about; else the good deed,
  mystery or timeloop their tags belong to (begun or not, with the steps between found by the tags
  handed out together, or a telling word shared); else the place their name holds
  (`Universal_Location_…`) or this region; what they mark ("opened", "done") follows after a dot.
  A Lymbic rod's pickup has the rod's name.
- Both sides of the panel have the same margin; the achievements list's order and "show unlocked"
  switches no longer squeeze each other.
- Long lists can be grouped: places by kind, achievements by kind (story, deeds and mysteries,
  combat, gear, research and collections).
- The map's lines, outlines and terrain edges are antialiased.
- The map's background, lines and icons each have their own opacity, with presets (Solid,
  Balanced, Subtle, Icons only).
- The quest tracker sits at the top right under the minimap, clear of the game's pop-ups.
- Cards keep one gap everywhere, also between cards stacked in one column.
- The map's water now reaches its real shore instead of stopping at the game's water boxes, and
  the big map's ground is drawn as dots with gaps between them so the game shows through (on by
  default); icons, the route and pins stay solid.
- Scrolling is visible: solid scroll bars and a fade at an edge that hides more. Other regions are
  chips; puzzles and timeloops no longer show internal names.
- States read at a glance: chips for "opens now", covered or short, soon or later, a vault's state;
  keys as keycaps; card headers without the accent rail.
- One look per role: press a line to be guided there (no separate Guide buttons), one small
  reveal button at most per line, normal-size action buttons, and switches instead of checkboxes,
  sized to the text beside them.
- The cheat groups sit below the pages in the sidebar and start folded, their heading counting the
  cheats on.

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
- **Shard budget:** a card on Collect shows what *Good Vibrations*, *Accessorizing* and *To the
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
