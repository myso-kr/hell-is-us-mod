# Changelog

Versions follow [semantic versioning](https://semver.org/).

Every release records the Steam build it was verified against. Steam's `buildid`
is in `steamapps\appmanifest_1620730.acf`, and `hiumod doctor` prints it. See
[`docs/ANCHORS.md`](docs/ANCHORS.md).

## Unreleased — 0.1.0

Attaches to Steam build `24045435`, where `doctor` passes every check. No cheat has
been verified in play yet.

- Ported from dungeons2-mod: the F8 panel, the CLI, attributes by name, and holds
  whose originals are kept on disk.
- No per-build offsets. The name pool and `GEngine` are found on each attach. The
  FField layout is chosen from known candidates. The path to the player is learned
  from the engine's reflection by property name.
- A hero gate replaces the solo gate. Writes go only to the hero's attributes.
  Holds pause while the gate is closed.
- 15 cheats in four tabs (생존, 전투, 이동, 성장), all unverified. The ranges come
  from the defaults read live. Coefficients that default to 0 are added bonuses, and
  those that default to 1 are multipliers.
- `hiumod pose` prints the hero's position and camera yaw. This is the minimap's input,
  and the readings were seen moving in play.
- First play test: `god` and `stamina` work. Five coefficient attributes held in
  memory but changed nothing, so they were dropped. Speed is now the movement
  component's `MaxWalkSpeed`. Attack and dodge speed became `hero_time`, the hero's
  own time dilation. Plain fields like these go through the same checked, restorable
  path as attributes.
- Second play test: `speed` and `hero_time` work. Six more coefficient cheats did
  nothing and were dropped. That leaves 7 cheats in three tabs, 4 of them verified.
  Damage, defense, cooldowns and XP need GameplayEffects applied in game.
- Minimap overlay, seen working in game (shows, turns with the camera, F6 markers). It is a click-through window over the game
  showing the trail walked, markers (F6) and the hero's heading, with F9 to toggle. Both keys can be changed in the panel: F7 was the first choice, but it is the game's photo mode.
  It is kept per world in `Mods\minimap.txt` and has its own tab in the panel.
- The minimap dots enemies, items, loot, NPCs, and doors and puzzles. The tool finds
  them by class among the actors of every loaded level, with a full scan once a second
  (18 ms) and positions every tenth of a second. Each kind can be toggled.
- Corpses (health 0) and things already picked up or used (`bHasBeenActivated`) are
  no longer shown. Things are drawn as SVG icons (`assets/icons/`, rasterised by
  resvg) instead of dots.
- Map filters go below the five kinds, to 22 finer sorts read from class names
  (enemy families; medicine, food, weapons, gear, skills, research, lore, quest
  items...). The icon size is set in the panel (10–32 px).
- The minimap draws the level itself under everything else. Static meshes on the
  hero's floor (walls, floors, pillars) are drawn as their outlines from above, built
  from each mesh's bounds and transform. The game ships no map of the area it could
  use instead (`.spec/MAP.md`). It can be switched off in the map tab.
- Maps draw about ten times faster (the big map with everything on: 261 → 27 ms
  a frame), and the overlay keeps its pace however long a frame takes to draw.
- The big map's opacity can be set (20–100 %).
- The panel is wider, in a console layout: a sidebar with status and pages, and
  two columns of cards. Map, guide and north correction are one page.
- The map can draw the land itself: hill shading tinted by height against the
  hero's, contour lines (every 2 m, strong every 10 m), or both, with water in
  blue. Choose "지형 표시" on the map tab.
- Routes know water, slopes and bridges. Deadly water comes from the game's
  own kill boxes, cut to where the ground lies below them. The ground comes from
  the landscape's physics heightfields: steep ground costs more, and rocks are
  judged by the ground they stand on. A bridge's deck is walked over its piers.
  Routes are now worked out off the overlay thread.
- Routes no longer slip through thin fences or squeeze diagonally past corners.
  When no way round is found nearby, a wider area is searched. Only then is a
  route allowed through an obstacle, and that part is drawn as a dashed yellow
  line, with a warning on the guide tab.
- Routes go round what is really there. The game keeps no ground navmesh in
  memory, so obstacles come from every mesh's collision shapes (boxes, spheres,
  capsules, convex hulls). That covers plain meshes, instanced meshes and foliage
  alike, placed by instance matrices and each component's world transform. 69k
  obstacles are collected without stalls. Anything at the hero's level blocks. A
  route is only allowed through when there is no way round.
- North now matches the game's own compass. The game's north is world −Y (yaw
  270), not +X. This applies to the compass strip, the north-up maps and the N
  marker, and can be adjusted in the panel.
- Walking routes: A* over a grid built from the walls on the hero's level, with
  cliffs dearer and the walked trail cheaper, pulled straight where no wall is in the
  way. The route is drawn on the maps, and the compass points along it with the
  route's length.
- Big map in the middle of the game window (F3, north-up, radius 50–1000 m). All
  overlays hide while a game menu is open, detected by the game cursor showing or
  the game pausing.
- Compass strip at the top centre of the game window (F10). Quest guidance comes
  from the game's own state. The hero's known facts and tags are read from the save
  state, and every interactable's payload is compared against them. Places still
  holding something new become quest goals, secrets or clues. The compass and the
  minimap point to the chosen goal with its distance. F11 cycles goals. The panel's
  new 안내 tab lists them, and auto mode follows the nearest quest goal. Built on
  GUObjectArray and struct reflection (`.spec/GUIDE.md`).
- Save points, the objects you interact with to save, get their own group and icon.
  Autosave triggers are not shown.
- Heights are told apart. Ground is coloured by its height against the hero's feet:
  deep, lower, level, raised, cliffs and rocks, and walls. Each band has an edge in
  its own colour where the ground steps, so the edges read as contour lines. Big
  cliffs are no longer left out, and the map tab shows a legend.
- The panel has more room: it is wider and uses larger spacing and text. The map tab
  is split into four boxes. Kinds are on/off switches with their map colour, finer
  sorts are coloured chips under 'details', and counts are in parentheses.
