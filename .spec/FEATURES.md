# Feature phases: F1–F11, and the "now" page

A record of the guidance features, added in phases. The F numbers come from the 2026-10-03 feature
plan: phase 1 is F2, F3, F5, F9; phase 2 is F1, F4, F10, F11; phase 3 is F6, F7, F8.

Everything is read directly from game data; walkthrough text is not used. The one exception is the
deadlines for missable good deeds (F1), which are not in the game data and are taken from guides, with
the sources credited.

## 1. Phase 1 (2026-10-03): save backup F9, mysteries and time loops F5, hand-over hints F2, map pins F3

- **F9 Save backup** (`src/infra/backup.rs`): checks the modification times of
  `%LOCALAPPDATA%\HellIsUs\Saved\SaveGames\*.sav` every 2 s. After a change, once the files have been
  quiet for 3 s, everything is copied to `Mods\backups\<timestamp>\`; the newest 20 are kept. The
  Saves tab shows the latest 3, with "Back up now" and "Open folder".
- **F5 Mysteries and time loops:** `SecretsSubsystem.Mysteries` (MysteryData, 0x70, 43 entries) and
  `Timeloops` (TimeloopSecretData, 0x78, 14 entries) share the good-deed layout: Guid `+0x18`, start
  tag `+0x28`, completion `+0x30`, Title `+0x38`. The save's `SecretsState.Mysteries/Timeloops` uses
  the same elements. Code: `quests::Kind::{Mystery, Timeloop}`, cache lines `mystery`/`timeloop`.
  Measured: 5 mysteries and 2 time loops in progress.
- **F2 Hand-over hints:** every trade of every NPC in the survey database (`Entry.trades`: the wanted
  item, the facts and tags given back). The first version remembered only the first trade per NPC,
  which missed the Gold Watch for the hub merchant (who takes the empty baby bottles and the Gold
  Watch). An item the player has, whose reward is still new, is listed under "Things to Hand Over".
  Measured: Gold Watch → the hub merchant in Senedra; two Sheet Music items → the Fearful Fiddler in
  Acasa.
- **F3 Map pins:** `Marker {at, kind: PinKind, note}` (now in `src/map/pins.rs`); the original kinds
  were locked door, puzzle, later, and mark. Saved as `marker W x y z kind note…`; old lines load as
  mark. Pin ids set bit 62 (survey database ids use bit 63). The overlay inserts this region's pins as
  goals, so they get guidance and routes, and the map draws them as pins instead of diamonds. Panel:
  kind for new pins, and a list sorted by distance (change kind, note, guide, delete).
- **Tab split** (user request: do not crowd too many elements into one panel): Map, Guide, Quests,
  Saves, Debug.

## 2. Phase 2 (2026-10-03): missable good deeds F1, keystones F11, collectibles F4, NPC stories F10, 24 pin kinds

- **F1 Missable good deeds** (`src/guide/missables.rs` + `assets/missables.tsv`): deadlines are not in
  the game data, so the missables tables from guides (Game8, PowerPyx) became a data file. Each deadline
  (end of Act 1, Quest03, the Talju truck, the Ministry of Cultural Primacy, the third keystone) is
  judged from main quest state as soon / later / passed. A chain with several good deeds (A Light in the
  Dark 1–4) lists all of them. Shown as a Quests tab card and a warning line in the tracker. Measured
  during Act 1: Land of Milk and Honey is "soon".
- **F11 Keystone order:** in Act 2 (Quest02 done, three keystones not done), the remaining keystones in
  the recommended order (Terror → Rage → Ecstasy), with the good deeds that close before the next
  keystone.
- **F4 Collectible progress:** items are classified by the folder in their survey path (Relics,
  LoreItems, Research, Cosmetic, Drone, WeaponModules, Weapons, DefensiveGears, Lymbic, CraftingTomes)
  and marked collected by save GUID. Fixed a bug where placed actors that give only a collectible were
  dropped when reading the survey database. Only items placed in the world count (not NPC rewards or
  shops), so the number of places can exceed the game's own count (one item in several places).
- **F10 NPCs with more to tell:** NPCs have no save ID (all 227), so "has the player talked to them"
  cannot be known. Instead: NPCs whose dialogue graph gives facts or tags the player does not know yet.
  Collect tab.
- **Guide helper** `guide_to` (`src/ui/panel/mod.rs`) + `MapState.adhoc`: the overlay can also use a
  survey place that is not loaded (collectible, NPC) as a goal and guide to it.
- **24 pin kinds:** `assets/pins/<word>.svg` (pin shape plus a white symbol), 24 `PinKind`s; the old
  words (`locked`, `later`) carry over. The panel uses a dropdown.
- **Grid balance** (pre-masonry, see `PANEL.md` §2): Map tab = [minimap, terrain, big map, keys] /
  [show on map, map pins]; Saves tab = [backup] / [save files].

## 3. Phase 3 (2026-10-03): puzzles F6, vault notebook F7, enemy groups left F8

**F6 Puzzle helper** (`src/read/puzzles.rs`, Guide tab, "Puzzles Nearby")

- The quest pass, which already walks every object, also collects puzzle components
  (`src/guide/quests.rs`, `Role::Puzzle`). Once a second, those within 40 m of the hero are read.
- Dials: `DialPuzzleActionComponent.Dials` → `DialComponent.{DialState, DialSolution, NbDialState}`,
  shown as "dial n: turn k steps (now a → b)". Solved when `bIsDialsLocked` is set or every dial
  matches. Positions are 0-based in the game and shown +1, the same convention as vault code symbol
  numbers (confirmed 2026-10-03 by opening a vault in game).
- Keypads and computers: `KeypadRuneComponent.Rune.ExpectedCode` (a string). Opened state:
  `KeypadAction` / `ComputerAccessAction.bHasBeenActivated` on the actor.
- Item placement (key doors, Lymbic rods, photos, etc.): `ItemPlacementActionComponent.Solution` → a
  single item, or a list in `ItemPlacementCondition.Solution`. Names use the game's translations.
- Answers are hidden by default and shown with "Show answer" (remembered for the current run only).

**F7 Vault notebook** (`src/guide/tables.rs` vaults, Collect tab)

- Table: `Gameplay/Research/CacheData/VOFK0n_*_CacheData_DA` (ResearchCacheData): GUID; name, region and
  clue (string table → `{g:ns/key}`); `NumberOfLoreEntriesToUnlock`; `Code` (four ECacheSymbols →
  symbol numbers 1–8). tools/survey writes it to `Mods\survey\vaults.json`.
- Save `Player.ResearchState`: `KnownLoreEntries` (research count); `KnownCacheEntries` (every vault is
  present from the start, so only `+0x11 bIsShownToPlayer` means "known"); `OpenedCaches` (GUIDs).
  States: open / info available (shown, or research count ≥ required) / locked (research n/m).
- Vault door location: the survey records the `VOFK_<region>_DialPuzzle_Interact_BP` actor with
  `"vault": true`, which enables the Guide button.
- Codes are hidden until "Show code". The symbol images are not textures in the game (they are
  materials), so originally only numbers were shown; see §5 for the drawn symbols.

**F8 Enemy groups left** (`src/guide/tables.rs` hollows, Collect tab)

- Table: `GameData/Spawner/<World>_Root_WP_Spawner_DT` (SpawnerLymbicEntityData: SpawnerSerializeGuid,
  EntitiesToSpawn, SpawnerLocation, TimeloopActorID): 522 spawners in 11 regions, written to
  `Mods\survey\spawners.json`.
- Rule: if the save's `World.RegionStates` has a state for the spawner's GUID, the group is cleared.
  Measured: spawners without a record have live enemies nearby (≤ 5 m); those with a record mostly do
  not (2 exceptions, attributed to a neighbouring spawner's enemies). Confirmed in game on 2026-10-03:
  clearing a group lowers the count. The record's Data is a single byte (`F8`); the
  SerializeSpawnerState (DefeatedLymbicEntities) is not stored as such.
- Shown: groups and enemies per region, per time loop, and guidance to the nearest. A live
  achievement-side value exists, `CharlieAchievementsUnlockerSubsystem.AdditionalClearedMapInformation`
  (per region and time loop `bAll…EnemyKilled`), but is not displayed.

Refreshing the data: `doctor survey` (everything, ~2 min) or tools/survey `--tables` (tables only, a
few seconds).

**Puzzle list** (F6 extension, Guide tab, "Puzzle List")

- Extends the 40 m nearby view to all regions: the surveyor records each puzzle actor's components as
  `"puzzle"`: dials (`DialComponent` sorted by name, NbDialState, DialSolution), keypads
  (`KeypadRuneComponent.Rune.ExpectedCode`), item placement (`Solution` → item, or the Solution list on
  the condition object; follows exports into blueprint packages).
- Actual distribution:
  - dials 24: 3 dials × 10 positions (8), 3 × 6 (1), 4 dials × 8/4 positions (3), vaults 4 × 8 (12);
  - codes 33: 3 digits (4), 4 digits (8), 5 digits (7), 6 digits (14);
  - item placement 212.
  Duplicates with the same position and kind are merged.
- Solved: ✓ if the save has a state for the actor's GUID. Remaining puzzles in the current region come
  first, then by distance; answers hidden; Guide button. Key and item placement puzzles appear only
  when their checkbox is ticked.

## 4. Achievement progress (2026-10-03)

- Reads Steam's cache rather than the Steam API (`src/game/achievements.rs`):
  - `<Steam>\appcache\stats\UserGameStatsSchema_1620730.bin`: 40 achievements, with API name,
    per-language names and descriptions, hidden flag, progress stats;
  - `UserGameStats_<account>_1620730.bin`: unlocked bits, unlock times, stat values.
  Both are binary KeyValues.
- Game language → Steam language (`koreana`, `japanese`, `schinese`, `brazilian`, …). 12 achievements
  show progress (good deeds /26, mysteries /43, relics /29, item placements /25, …).
- Collect tab "Achievements" card: unfinished first; hidden achievements stay masked until "Show".
  Re-read every 10 s, so changes appear once Steam writes the file after the game reports.
- The game's own table (`AchievementsDefinitions_DT`) has developer placeholder names, so it is not used.

## 5. Vault symbols (2026-10-03)

- Vault code symbols 1–8 are Plutchik's eight emotions in alphabetical order: 1 Admiration,
  2 Amazement, 3 Ecstasy, 4 Grief, 5 Loathing, 6 Rage, 7 Terror, 8 Vigilance. Each language's names
  are in the i18n tables, using the game's official term where one exists (in Korean, for example,
  the game's own words are used for Ecstasy, Rage, Terror and Grief).
- Confirmed by matching the six vault codes (`vaults.json`) against symbol names in guides (Fextralife,
  DualShockers, etc.), e.g. Vault of the Copse [7,1,6,2] = Terror, Admiration, Rage, Amazement, and then
  checked against vault doors in game.
- Drawings: SVGs drawn for the mod, using web images only as reference (none are in the repository):
  `assets/symbols/<name>.svg` (100×100, white stroke 4, round caps). Other colors are made by
  replacing `#ffffff` (`src/map/symbols.rs`).
- Drawing them in the panel: `PANEL.md` §2.

## 6. The "now" page: region ledger, before you go on, previously, Lymbic locks (JOURNEY.md §3)

Built from JOURNEY.md's first proposals, with the panel's tabs reorganised so no page carries more
than four cards (PANEL.md §1).

- **Region ledger** (`src/guide/ledger.rs`, §3.1): for every region, what is left by kind — quest
  places (a place two quests need counts once), hand-overs possible now, dials and keypads unsolved
  (vault doors and Lymbic locks apart), Lymbic locks (and how many the rods held open), vault doors,
  collectibles (`Collect::left_by_world`), enemy groups, NPCs with more to tell. Shown as the Now
  page's **hero**: a live north-up map round the hero (the overlay draws it once a second while the
  panel shows, `Shared::hero`), the region in large type, a "can do now" chip, and six KPI tiles on a
  3 × 2 grid; a tile opens the page that lists the places.
- **Trip planner** (§3.4): the same rows for the other regions as a bar chart (`tw::bars`), sorted by
  what can be done there now (hand-overs + locks the rods held open, in the accent), then by what is
  left (the bar behind it), all to one scale.
- **Before you go on** (§3.5): the good deeds whose story point can come any time now (F1's "soon"),
  the act 2 keystone order, and how many deadlines are further off; a button to the Quests page.
- **Previously** (§3.8, `src/infra/session.rs`): the panel writes `Mods\session.txt` (time, region,
  followed quest, position) every 30 s while the hero is in play, and reads the previous one once at
  start. After a gap of 30 minutes or more the panel opens on the "now" page with a card: when and
  where the last session ended, the quest followed and its open leads now. Dismissed with "Got it".
- **Lymbic locks** (§3.3, `Survey::locks`): every lock panel whose answer is Lymbic rods (24 in the
  survey, a duplicated panel counted once), with each rod held (✓) or missing (✗). A missing rod shows
  where its nearest pickup not taken is — the distance in this region, the region name elsewhere. A
  lock's line and each missing rod's line are pressed to be guided there (the line guided to stays
  marked); pressing is the asking, so there is no extra reveal step and no separate buttons. Some rods
  lie in other Lymbic chests. Locks the rods held already open come first; locks elsewhere that cannot
  open yet are only counted.
- **Height in every distance:** a list's distance adds ↑/↓ when the place is 3 m or more above or below
  (`raster::span`) — one Acasa lock sits 9 m under the monument it is "3 m" from. The compass already
  marks height with its own arrow; this brings the panel's lists (guide target, places, quest needs,
  hand-overs, collectibles, puzzles, pins, locks) in line.
- **Completion board** (§3.9): not a page of its own. The Collect page's collection card already
  counts every sort (glyphs are its "Lymbic skills", caps its "Caps"); opening a sort now lists, under
  the nearest left here, how many are left in each other region, most first. Good deeds, mysteries and
  timeloops stay on the Quests page, vaults on the Puzzles page.
- **Clue board** (§3.2, `src/guide/clues.rs`, a **Clues** page): the Datapad's facts the hero knows,
  grouped by the entry they are about (a person, a place, a thing — the story unit), in the game's
  language from `facts.tsv` (I18N.md §2). Two cards:
  - the followed quest's entries: an entry belongs to a quest when one of its known `Quest` facts reads
    as the quest's name ("Family Reunion"); pressing an entry opens what is known of it — description,
    then connections, then where. Good deeds, mysteries and timeloops advance by their own steps, not
    the Datapad, so they have no entries; the card says so.
  - a search: a word or a name over every known line (an entry's name matches all its lines) and the
    items held, in any case.
  Measured: 29 entries and 255 lines known in the save examined.
- **Shard budget** (§3.6, `src/guide/budget.rs`), at the top of the Collect page's achievements card:
  - The recipes are the game's: every `CraftRecipe` under `Gameplay/Crafting/` (`recipes.json`, 174:
    84 weapon upgrades, 60 defensive gear imbues, 12 shard infusions, 18 consumables). A recipe takes an
    item and shards and makes the next grade; a few grades have a second recipe (`…_Recipe02_DA`, from a
    `Level10` item). Shards are a feeling (`Neutral`, `Rage`…) and a tier (G01–G03); three neutral shards
    infuse into one of a feeling within a tier.
  - Held items and their counts come from the inventory (a stack's count is the u32 after its
    `ItemData` pointer, as the stock cheat reads it).
  - For each upgrade achievement not yet earned: the cheapest way, by the recipes, from the items held
    (Dijkstra over items, a feeling's shard weighing three neutral ones). *Good Vibrations*: one weapon
    of each of the four types to grade 5; *Accessorizing*: one defensive gear to grade 4; *To the Teeth*:
    two weapons at 5 and two gears at 4.
  - Kept short (the achievements card was too wordy): the shards held as a size × feeling table; one
    line per achievement with a chip on the right ("Covered", or "Large 700 short", counted in neutral
    shards); under it the upgrades in one line ("Ecstasy Twin Axes 4→5 · …"); a weapon type not held
    in amber. The full cost is the name's hover text; the shard sizes are the heading's.
  - The heading's right side says how many timeloops are still open. Its hover explains why it matters:
    once all are closed the enemies stop coming back and the shards held are all there will be.
  - The achievement list: the name with progress on the right, the condition under it in small text
    (the condition stays visible: it is what the player needs).
  - A hidden achievement (*To the Teeth*) shows its plan only once it is shown, like its text.
  - Checked against guides (2026-10-03): a shard's size is the dropping enemy's level (small from
    level 1, medium 2, large 3) and sizes do not convert, so small shards held do not cover a large
    shortfall; the heading's hover says so. The grade 5 / grade 4 costs the guides give match the recipes read;
    PowerPyx confirms that cleared timeloops stop enemies respawning, making shards finite.
  - Measured on the save examined: Good Vibrations costs 50 of each feeling and 100 neutral at G03,
    700 neutral G03 short; the four weapons are at grade 4.
- Not built yet from JOURNEY.md: the chest rewards and the timeloop prisms (§4 research), Haze links.

## 7. Choice puzzles: the right slot, a hint at a time (2026-10-05)

Asked: research the ceramic flowers quest and design a puzzle guide for it in the mod, then look
for puzzles like it and build it.

**The puzzles.** Two in the game ask the player to choose a slot for each item, with decoys among
them. Both use one blueprint, `Base_1SlotPlacementPuzzleCheck_PassiveInteract_BP`, and nothing else
does (the survey's 70 other placements take one item at one place: keys, gears, keystones):

- **The Watcher's Nest, Vyssa Hills** (Quest02, "Family Legacy"): four Ceramic Flowers, found on
  Pentos Algea's coffin, go into 19 flower-shaped grooves. The riddle is "Note - Wall Grooves":
  "Through his name, his title, the symbol of his Order, and the date of inception of his Order,
  shall the path be revealed." Vitalis's computer has a research file per set of grooves: 8 under
  wooden shields in the entrance hall (his title, Warden of Grief), 3 under metal shields in the
  corridor (the Vigil's coat of arms), 2 flanking the old banner (1513), 4 in the alcoves behind the
  whiteboard (his name, Algea), and 2 by the armour rack, which the file calls meaningless. All four
  in, the ground rumbles; the coffin's lever then opens it (Mark of the Betrayed, the Blue Flower
  Scroll, Gildas Brom). Players get stuck on the scattered clues, the alcove hidden behind the
  whiteboard, picking the wrong groove of a set, and pulling the lever early or missing the rumble.
  A flower put in the wrong groove can be taken out again (the user, in play).
- **The Eye of God, Lake Cynon** (Quest06): four orbs (Gold, Cobalt, Crimson, Emerald) go to eight
  statues of the Order's founders. "Scroll - Failsafe" names eight keepers; the orbs belong to the
  keepers of Ecstasy, Grief, Rage and Terror, which their descriptions tell (great bliss, sorrow,
  anger, fear). The statues of Vigilance, Admiration, Loathing and Amazement are decoys.

Sources: Fextralife (Ceramic Flower, Scroll - Failsafe, the orbs), TheGamer, DualShockers,
NerdStash, BaseDotaku, Game8, Vandal; hellisus.org contradicts all of them and was not used.

**What a slot counts as right.** The slot's placement `Solution` is only what it accepts: every orb
at each statue, a stand-in (Bronze Cup) at each wrong groove, which the panel used to show as the
answer. The answer is the class default's `Item`: the base sets `ItemPlacementValidation_DummyItem`,
the right slots' classes set the flower or their orb, the decoys keep the dummy. The survey writes it
as the puzzle's `expects` (SURVEY.md). The family home's picture holders (Quest01) take any of the
three photos and check nothing per slot: the door's code from the photos' car numbers (32-17-13) is
the puzzle there, and they are left as they were.

**Grouping** (`src/guide/slots.rs`): choice slots within 250 m of each other in a world are one
puzzle; within it, slots within 10 m and 1.5 m of height are one set (one wall's grooves; a floor up
is another). The Watcher's Nest comes out as sets of 8, 4, 3, 2 and 2 (one of the pairs with no right
slot); the Eye of God as eight single statues, four with no right slot.

**State.** What a slot holds is read live while it is loaded (`ItemPlacementActionComponent.Slots`
→ `ItemPlacementSlotComponent.Item`, `puzzles.rs`), matched to the survey's slot within 1 m, and
remembered through the run. A set is done (the right item in the right slot, nothing in its decoys),
wrong (something in a decoy, or the wrong orb), open, not seen yet, or one where nothing goes. The
save holds each slot's state only as rune bytes, not read.

**The card** (Puzzles page, across the page, the Settings page's "answers" granted as for the whole
page): each puzzle with its sets placed right out of those with an answer, and that a wrong
placement can be taken out again; on asking, the game's riddle (the note or scroll, its inline
images dropped); then per set, its name (the research file's title, or "Spot n"), slots, distance
and state, with two steps on asking: the game's clue for that set (the research file; the orb
statues have none, the scroll covers them) and the answer (the right slot and its item, or that
nothing goes there). A set with no right slot reads like the others until its answer is asked for,
so the list does not give the decoys away. The line guides to the set, or once its answer is told,
to the right slot. With all four flowers in, it says to go back and pull the tomb's lever.

**Marked in the game's view** (2026-10-05, after play: "the distance alone does not say where"): the
grooves of a set are 2 to 6 m apart, so the answer line's distance did not tell which. The overlay now
reads the game's camera (PlayerCameraManager → CameraCachePrivate.POV: location, rotation, horizontal
field of view; `player::Camera`) with the pose each frame, projects the guide's target onto the game
window's drawn area, and rings it there with the distance under it (`overlay/marker.rs`, a 96 × 112
window moved to the spot), when the target is within 60 m and in view. It works for any target; the
answer line makes the right groove the target. The answer line also counts the right groove from the
left as the hero faces the wall ("facing it, 3 of 8 from the left"). The ring does not know what is
in between: farther than 60 m it is not drawn.

**What play showed, and fixed** (2026-10-05, at the corridor's three metal shields):

- A flower in the right groove read as "in a wrong slot": a slot's `Item` is the hero's own
  `CharlieInventoryItem`, not the data asset; its `ItemData` is followed now.
- The ring sat by a crate on the floor: the slot actor's place is on the floor, its groove 1.55 m up
  the wall. The groove is the slot's `PlacedItemMeshComponent`, read live; until seen, 155 cm above
  the actor (the base blueprint's, measured).
- On a 3440 × 1440 screen the ring for the right groove was 165 px off: the camera's FOV is the
  horizontal one of a 16:9 view and the game keeps the vertical one, so the focal length is the 16:9
  width's (a test keeps the measured screenshot's numbers).
- The ring's distance was the camera's, a few metres behind the hero: it is the hero's now.
- The answer became the guide's target only on a second press on its line; asking for the answer
  now guides there at once. Confirmed in play: the ring on the third groove, the flower in and out
  read as it changed.

Not done: map markers for the sets, a line on the Now page when the player holds the items near the
puzzle, and the Eye of God's per-statue keeper names (the decoys' keepers are not in the data).

