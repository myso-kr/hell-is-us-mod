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
- The overlays show only while the game window has focus — not while the panel
  does.
- The panel is laid out with CSS Flexbox and Grid (taffy): cards sit side by side
  while there is room and stack when there is not, every widget is measured rather
  than guessed, and the window is sized to its page's cards. Settings that are
  best left at their defaults are no longer shown (the F9 cycle, north correction),
  and the panel's notes and debug details are trimmed.
- Setting shards now stops at 990 (900 by default). At the game's own maximum of
  999 a stack takes no more, so pickups stop and no acquired notice shows.
- Ghost (survival tab): the hero joins the enemies' side, so they ignore it — and
  its own blows do not land while it is on, so it is for getting past, not for
  fighting. A cheat to ignore hits was tried two ways and dropped: the game's
  hits do not go through anything a data write reaches.
- A weapon experience multiplier (1–10×), not yet tried in play: whatever a kill
  gives a weapon, it gets that again times the multiplier less one. Weapons at
  their grade's level cap are left alone.
- `hiumod doctor` can look inside the running game, writing nothing: `inspect` a
  live object's fields (and the native bytes between them), `find` a property by
  name, `dump` an SDK-like listing, `watch` what changes, and `scan` for a value
  then narrow it as it changes. Results are also saved under Mods\doctor.
- The overlays no longer flash over the panel. Both were topmost windows and took
  turns putting themselves first; while the panel shows, the overlays now stay
  just under it.
- New cheats, not yet tried in play: game speed (the world's own clock), enemy
  speed, frail enemies (one blow kills), consumables that do not run out, setting
  the count of shards you hold, and five saved positions to go back to.
- The panel shrinks back when a shorter page is chosen. The divider between the
  sidebar and the page used to take all the window's height, so it could only grow.
- The big map is drawn as outlines on a clear background by default, in the
  style of Diablo's overlay: walls, shores and contours as lines, nothing filled.
  The minimap can use the same style.
- One map key steps through the displays, as most games do: minimap, big map,
  off. The panel picks the display and which displays the key steps through.
  The big map's own key is gone.
- Panel cards no longer overlap: columns hold to their width and clip, form
  rows have a fixed label column, and sliders, button rows and notes fit or wrap.
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
- Quest journal and tracker. The main quests (Quest01–06) and good deeds (the
  game's side quests) are read with their in-game names and descriptions; `FText`
  is decoded. A tracker at the right middle of the game window lists the quests
  under way. The followed one is expanded with its description, clues found, and
  open leads. The map tab's 퀘스트 card picks the quest to follow. The default is
  "메인 스토리 자동", and auto guiding prefers goals that advance the followed
  quest. Good deeds come from the always-loaded `SecretsSubsystem`. Titles the game
  has shown are in Korean; the rest use the English source from the `UI_Secrets_ST`
  string table until then. Korean titles are kept in `Mods\quests.txt`.
- Following a quest turns route guidance on. The guide keeps to the nearest place
  that advances that quest and moves to the next when it is done; a target picked
  by hand is kept. NPCs are now goals: conversation payloads, including topic
  subgraphs, and item hand-overs that complete good deeds. Pickups that hold quest
  items are goals until taken.
- Auto guiding skips places that only pay out when something else happens there,
  such as a door opened with a key or a solved puzzle (`_PayloadInactive_` markers
  other than visit triggers). It also skips items of main quests not begun. The
  list marks such places "조건 필요".
- Fixed the route flipping between ahead and back. A recomputed route that heads
  the same way (within 100°) always replaces the old one, and so does any route
  once the hero strays more than 6 m. One that turns the hero round must be
  clearly shorter (under 85% of what is left). The compass passes corners cut
  short instead of pointing back at them. The walked trail is now only slightly
  cheaper than open ground.
- Routes no longer cross marsh water or unwalkable slopes. Only bridge-like decks
  near the surface make the water under them dry; trees, rocks and reeds used to
  punch dry holes through it (+26% water found in Acasa). Ground steeper than 45°
  now blocks, since the hero cannot jump; 35–45° stays dear.
- Floors on the map and the compass. Icons and goals on another floor (3 m or more
  up or down) are drawn faint, with a ▲/▼. The walls of the floor above and of the
  floor or cellar below show as ghosts. Compass pins shrink and fade with distance,
  and the target's label shows its height difference (`85m ▼12m`).
- Routes now use the game's own navmesh, read from World Partition navigation
  chunks in memory. They follow stairs, floors and cellars as the game's AI walks
  them, with the obstacle grid as fallback. A goal that can't be walked to (behind
  a locked door or a puzzle) gets a route to the nearest reachable point and a
  dashed last leg. Auto guiding then goes first to something reachable near it
  (a note, a key, a lever).
- `hiumod doctor usmap` writes a .usmap of the live reflection. The new
  `tools/survey` (C# with CUE4Parse) reads every World Partition map of the game and
  writes what hands something out (pickups, devices, markers, NPC conversations,
  hand-overs) with world positions and save GUIDs to `Mods\survey`. That is 1,174
  actors in 11 worlds, including every main-quest item.
- The survey is in the game. `hiumod doctor survey` builds the mappings and runs
  the tool, which now finds the AES key itself. Places from the survey that are not
  loaded become goals. The panel's quest card lists what the followed quest needs
  in every world: what's left, nearest first, click to guide; other worlds by count.
  The tracker sums it up ("필요한 것: 이 지역 n곳 · 다른 지역 m곳").
- What has been taken is read from the save. A placed thing's GUID under the save's
  region element states means it was picked up or used (verified by picking up a
  photo). The survey's needs and goals use that before facts, tags and inventory.
- The quest journal is ready in about 3.5 s instead of 15–20 s. Its pass over every
  object now runs on a time budget: 120 ms a step until the journal is first built,
  25 ms after that.
- After a conversation that teaches something, auto guiding moves on from that NPC.
  It returns once a new topic opens elsewhere. Talked-to NPCs are kept in
  `Mods	alked.txt`. The guide card has a 건너뛰기 (skip) button for goals finished out
  of sight.
- The panel opens with ` (~) instead of F8, and F8 can now be picked for the map.
  The panel has a console at its foot. It runs the CLI's commands (`pose`,
  `doctor survey` …) in the background and streams their output. `help` and
  `clear` are built in, ↑/↓ recall commands and Esc stops one.
- The console drops down from the top of the game window across its width,
  see-through as in Half-Life, while the panel is open. It no longer sits at the
  panel's foot.
- The panel is more compact and easier to scan. Text is a size smaller, and cards
  are raised panels with rounded corners and an accent bar. The selected sidebar
  item is outlined. Segoe UI Symbol is a fallback font, so arrows and shapes
  (▾ ▸ ↑ ↓) render instead of boxes.
- The travel trail on the map fades with age: the recent way is bright and wide,
  the old way thin and faint.
- Map icons per sort, 23 of them instead of 6. Enemies are skulls coloured by type:
  Feral red, Primeval violet, Negator orange, Protector teal. Items are coloured
  badges with a glyph for medicine, food, consumables, weapons, gear, skills, drone
  modules, research, lore, quest items and stashes. Loot and NPCs are circles,
  doors, locks and translations diamonds, and save points a drop.
- The console starts closed. Open it with the 콘솔 button in the panel's header.
- Clearer guide controls. One 자동 안내 switch replaces 자동 plus 안내 끄기. The
  target row has 다음 목표 ▶ to move on. A hand-picked target shows that it is kept,
  with a 자동으로 button. With no target, the card says why: auto off, nothing near,
  the quest's goals are in another region, or all were skipped. Skipped goals can
  be brought back.
- Save backups (F9). Each time the game writes a save, every save file is copied
  to `Modsackups\<time>\`, once the write has settled. The newest 20 are kept.
- Mysteries and timeloops in the journal (F5), beside main quests and good deeds,
  with their names from the game. They can be followed like good deeds, and their
  places come from the survey.
- Hand-over guidance (F2). The panel lists NPCs who want an item you hold and have
  not been given it yet ("Caddell GoldenWatch → Herbalist, Senedra"). Every trade an
  NPC takes is checked, not just the first. In this world a press guides there.
- Labelled map pins (F3). The marker key places a pin of the chosen kind: locked
  door or chest, puzzle, later, or mark. Pins are coloured on the map and compass,
  and can take a note. The panel lists them nearest first, to change their kind,
  edit the note, guide to them, or remove them. Old marker lines still load.
- The panel's tools have their own tabs: 지도 (maps, pins, keys), 안내 (compass,
  guide, where to go), 퀘스트 (journal, needs, hand-overs), 세이브 (backups) and
  디버그, instead of one crowded map page. The save backups card has
  지금 백업 and 폴더 열기.
- Missable deed warnings (F1) and keystone order advice (F11). Deadlines come from a
  table in `assets/missables.tsv` (from the guides). They are judged against the
  journal's main-quest progress as due now, later or past. The quest tab lists them
  soonest first, and the tracker warns when one is due now. In act 2 a card lists
  the keystones left in the suggested order (Terror first) and what is due before
  the next one.
- Collection progress (F4) and NPCs with more to tell (F10) in a new 수집 tab. It
  shows per collectible sort (relics, lore, research, caps, drone modules, skills,
  weapons, gear, Lymbic rods, crafting tomes) how many placed pickups are taken,
  here and everywhere, from the survey and the save's states. You can guide to the
  nearest left. It also counts good deeds, mysteries and timeloops done, and lists
  the NPCs whose talk still holds something new.
- Map pins come in 24 kinds, each drawn from its own SVG in `assets/pins/`: locked
  door, locked chest, Lymbic lock, puzzle, code, key needed, for later, merchant,
  person, quest lead, danger, strong enemy, timeloop, save point, shortcut, climb,
  dead end, water, lookout, treasure, note, base, unknown and mark. A dropdown picks
  the kind. Pins from before keep their kind.
- Panel pages are balanced. The map page splits into the map settings with keys,
  and what the map shows with pins. The save page gains a card listing the game's
  own save files and when each was written.
- Memory. The engine alone stays near 50 MB and the overlay's heavy work peaks at
  112 MB, both measured headless. The panel logs its working set, private bytes and
  peak once a minute and shows them in the debug tab. Journal, needs, collection
  and other survey-derived lists are now worked out once a second, not ten times,
  and shared between panel and overlay instead of copied every frame.
- Source layout. The modules now sit in folders by role: unreal, read, cheat,
  guide, map and infra. The panel is split into one file per page, and raster
  into canvas, compass and map drawing. Target choice moved to guide/target.rs,
  and file paths all start from paths.rs. Every old module path still resolves.
- Languages. The mod follows the game's text language (TextCulture in the profile
  save) and switches with it. Item, NPC and region names come from the game's own
  translations in all 12 of its languages, extracted by `doctor locale`. An NPC
  shows their real name once the hero has learned it. The mod's own text is
  written in Korean and wrapped in `tr!`/`trf!`, with an English table and a test
  that every string has a translation. A Japanese or Chinese font is loaded only
  when that is the game's language. The quest-name cache is kept per language.
- Phase 3, read from the game's data:
  - Puzzles near the hero, with their answers hidden until asked for: dial turns,
    keypad and computer codes, and the items a lock or placement takes.
  - A vault notebook: the six Vaults of Forbidden Knowledge, with state, research
    progress, clue, code and a guide to the door.
  - Enemy groups left per region and timeloop, for the every-Hollow achievement,
    with a guide to the nearest.
  - tools/survey writes spawners.json and vaults.json (`--tables` for those alone).

