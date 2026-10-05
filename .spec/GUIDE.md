# Compass HUD and quest goal guidance — research and design

Requested 2026-10-02: a compass HUD fixed at the top centre of the screen, and a switchable guide
that points the way to quest goals. This document holds the research, the decisions and the
implementation notes. Steam build 24045435.

Related documents: the route to a goal is in [ROUTES.md](ROUTES.md), map drawing in
[MAP.md](MAP.md), the panel in [PANEL.md](PANEL.md), feature stages (F1–F11) in
[FEATURES.md](FEATURES.md), game data in [SURVEY.md](SURVEY.md).

## 1. Facts about the game (web research)

- No waypoints, quest log or map markers — by design. The director: there is no magic compass
  pointing the way and no shopping-list quest log —
  https://game8.co/articles/latest/hell-is-us-has-no-map-or-quest-markers-to-test-your-navigational-skills
- The in-game compass is an item. Using it shows the cardinal directions at the top of the screen
  briefly; it never points at a goal — https://game8.co/games/Hell-is-Us/archives/545878
- Datapad sections: Investigations (6 main ones), People, Items, Locations, Research, Exploration
  (Good Deeds, Mysteries, Timeloops) — https://hellisus.wiki.fextralife.com/Investigations
- Region order: Senedra Forest → Acasa Marshes → Vyssa Hills → Lake Cynon → Lethe → Talju →
  Marastan → Jeljin → Arcas Spire → Auriga Museum → Plains of Mist —
  https://hellisus.wiki.fextralife.com/Walkthrough
- No existing mod shows goals or markers (checked 36 Nexus mods, GitHub, FearLess). The official
  accessibility options have nothing either — https://steamdeckhq.com/game-reviews/hell-is-us/
- Compass strip projection: inside the field of view, `x = tan(Δyaw)/tan(FOV/2)·W/2`; a strip wider
  than the field of view is linear — https://vazgriz.com/467/flight-simulator-in-unity3d-part-2/

## 2. Facts about game memory (local research, read-only)

### GUObjectArray
- `FUObjectArray` is at image +0x9304460 (chunk array pointer at +0x9304470): 255,987 objects in
  4 chunks. Found by scanning writable sections for the shape (Objects, PreAllocated, Max, Num,
  MaxChunks, NumChunks) where the `InternalIndex` (+0xC) of the objects in items 1..31 matches their
  own slot. Items are 0x18 bytes; 64K items per chunk.
- Subsystems are outside reflection, so this is the only way to find them.

### Quests are a knowledge graph of facts
- `QuestData` (`Quest01_DA`, …): `QuestElements` (related identities: people, places, things) and
  `QuestLinks` (relations).
- The `FactData` family (`QuestStatusData`, `TextFact`, `LinkFact`, `TypeFact`, `ImageFact`, …):
  `bIsQuest` (+0x4c), `AssociatedIdentity` (+0x50), `AssociatedQuestData` (+0x58).
- **There is no coordinate field anywhere.** Even location facts are text
  (`Herbalist_SenedraForest_Location_TextFact`).
- Subsystems: `QuestEventSubsystem` (world), `FlowSubsystem` (Flow graphs),
  `ResearchGameSubsystem`, `GoldenPathSubsystem` (CDO only — never instantiated in the shipping
  build). `Bragi` is music, `Bifrost` is travel between regions.

### The hero's knowledge is the save state
`CharlieSaveSubsystem.SaveSystem` → `CharlieSaveSystem.Saves` → `CharlieSaveGame` (one per slot; the
one with the newest `SaveDate` is the current playthrough). `CharlieSaveGame.Player`
(`CharlieSavePlayerState`, 1440 bytes):

| Field | Contents (test save) |
|---|---|
| `Knowledge.KnownFacts` | 124 known facts (`CharlieFactState.KnowledgeDataPath`, a soft path) |
| `Knowledge.FactTags` | 28 tags: `Quest.Facts.SenedraMedKitGiven`, `Secrets.Mystery.CaddellsBrothersTreasureStarted/Completed`, … |
| `Datums.QuestStates` | Active investigations: `Quest01_DA` (Family Reunion) |
| `Datums.DatumStates` | 10 learned identities (parents, herbalist, OMSIF, APC, regions, …) plus new facts |
| `SecretsState` | Good Deeds 26, Mysteries 43, Timeloops 14 (item states) |
| `CurrentSavePoint`, `World.SeenCheckpoints` | Last save point; 4 visited save points |

A soft path (`FSoftObjectPtr`) is an 8-byte weak pointer + package FName + asset FName + sub-path,
so the asset FName index can be compared directly with an object's name.

**Unverified:** whether the save object updates live during play or only when the game saves.

### What interactable world objects carry
Components attached to the `InteractableDynamicPayloadActor` family (items, puzzles, triggers):
- `PayloadRuneComponent.Rune.PayloadData`: `ContainedFacts` (TSet<FactData*>), `BaseIdentity`,
  `TagFacts` (GameplayTagContainer), `ItemsToAdd`, … — **the facts and tags you receive by using it**.
- `WorldLocationRuneComponent.Rune.WorldLocation` (FVector) — its own position.
- `QuestListener` actors reference quest status facts (`FactTag_Quest02Started`) and receive events.

## 3. Conclusion — goal positions can be computed

The game stores no goal coordinates, but it does expose **the position of every object that would
give the hero a fact or tag they do not know yet**. These are classified as:
- **Quest goal** — a fact whose `AssociatedQuestData` is an active investigation, or a tag starting
  with `Quest.`
- **Secret** (mystery, good deed, timeloop) — a tag starting with `Secrets.`
- **Clue** — everything else (records, research material, …)

Objects already used (`bHasBeenActivated`) are excluded, as in stage 2.

Limits: facts obtained through dialogue (NPCs) are not objects and are missed (see §6 for how NPC
goals were later handled). Goals in another region (a different world) are not visible.

## 4. Design

- **Compass HUD:** a click-through strip fixed at the top centre of the game window (a layered
  window, the same technique as the minimap). ±90° around the camera yaw, linear, ticks every 15°,
  N/E/S/W and the heading in degrees. Pins: the guide target (highlighted, with distance), user
  markers, save points, quest goals. Targets outside the strip get an arrow at the nearer end.
- **Guide:** the panel's Guide tab — a target list (quest goals / secrets / clues / markers / save
  points, nearest first). Choosing one highlights its compass pin and draws a line from the hero to
  it on the minimap. Options: an auto-guide switch for the nearest quest goal, a key to cycle
  targets, and turning guidance off.
- **Route:** the first version gave only straight-line direction and distance. A NavMesh (Recast)
  route was deferred because there was no public example of reading `dtNavMesh` from outside the
  process. Routes were later built, first on an obstacle grid and then on the game's navmesh — see
  [ROUTES.md](ROUTES.md).

## 5. Implementation notes (2026-10-02)

Paths below are the current ones (see the code map in [ARCHITECTURE.md](ARCHITECTURE.md)); at the
time several of these lived in different files.

| File | What it does |
|---|---|
| `src/unreal/names.rs` | Struct reflection: `struct_of` (StructProperty → ScriptStruct), `inner_of` (an array's inner property), `field_type` (FFieldClass name), path from an object to a field inside a struct |
| `src/unreal/gobjects.rs` | Finds GUObjectArray (must be unique), enumerates all objects, finds objects by class name |
| `src/read/knowledge.rs` | Newest `CharlieSaveGame` → known facts, tags and active investigations (sets of FName indices) |
| `src/guide/goals.rs` | Reads each interactable's payload (facts, tags) once and caches it; compares with knowledge to classify quest / secret / clue; drops used objects |
| `src/engine/attached.rs` | `Attached::goals` — GUObjectArray once; save slots every 60 s, knowledge every 2 s, payloads every 2 s (was `engine.rs`) |
| `src/map/compass.rs`, `src/map/raster.rs` | Stroke font (N E S W, digits, m k . -); `draw_compass` (±90° linear, 15° ticks, 8 directions, pins, target distance); goal diamonds and a dashed line to the target on the minimap (was all in `raster.rs`) |
| `src/ui/layered.rs` | Shared layered-window code (minimap and compass) |
| `src/ui/overlay/`, `src/guide/target.rs` | Overlay thread: minimap + compass; choosing the guide target (`settle_target`, `cycle`) now lives in `guide/target.rs` (was `ui/minimap.rs`) |
| `src/ui/panel/guide.rs` | Guide tab: compass toggle and key, auto-guide, kind chips, current target, active investigations, Places list (nearest first, click to guide) (was `ui/panel.rs`) |

- Default keys at the time (F2, F5, F3, F4 since 0.2.2): minimap F9, marker F6, **compass F10, next goal F11**. All four can be
  rebound in the panel; if any two collide, all reset to defaults. (The map keys were later
  reorganised — see MAP.md §10.)
- Auto-guide: when nothing is chosen, or the chosen goal disappears (because it was used), switch to
  the nearest quest goal.
- North: the first version used N = world +X, but **the user found that the game's north showed up
  as W on our compass** (2026-10-02). So game north = world yaw 270° (−Y). `MapState.north_yaw`
  (default 270) drives the compass ticks and pins, north-up rotation of the minimap and big map, the
  N label and arrows. In case it differs per region, the Guide tab has a north correction
  (−Y / +X / +Y / −X). UE yaw runs clockwise seen from above, so game east = +X.
- Measured (test save, Senedra Forest): 124 facts, 28 tags, investigation `Quest01_DA` → 11 places:
  - 3 quest goals: open the APC door (`Quest.Facts.APCAcquired`, 90 m), the Arcas Spire door, the
    Arcas Spire book;
  - 6 secrets: two Lymbic doors, the Blood Queen's storehouse, a timeloop start and completion, the
    smuggler's refugee;
  - 2 clues.

  First read takes about 1 s.
- A preview tool (`examples/preview.rs`, not committed) rendered one frame from real data: compass
  bearing and target distance agreed with the direction of the dashed line on the minimap.
- **Open at the time:** checking in the game itself; whether the save state is live (does an item
  drop off the list right after pickup?); dialogue facts (not objects) never appear as goals; goals
  in other regions (worlds) are not visible; NavMesh routes were next (since done — ROUTES.md §6–7).

## 6. Moving to the next goal after an NPC conversation (2026-10-03)

- Report: after finishing the conversation with an NPC needed to progress a quest, the guide did not
  switch to the next target.
- Cause: an NPC goal merges the payloads of **every branch** of its dialogue graph. Facts from
  branches whose condition (a topic unlock) was not yet met remained, so after the conversation the
  NPC still counted as a place with something new to get.
- Fix: when the number of known facts and tags among those an NPC gives goes up (the conversation
  gave something), that NPC is **talked out**. The count of known `Conversation.` tags at that moment
  is recorded, and the NPC stays out of the goals until that count rises (a new topic opened
  elsewhere). Records are kept by name in `Mods\talked.txt` so they survive a restart. The research
  DB's "needed" list uses the same rule.
- The guide card has a **skip** button ("This goal is done or out of reach — skip to the next one
  (this session)"): it passes over a goal that finished while the mod was not watching, for the rest
  of the run, and moves to the next goal.

## 7. Following several places (2026-10-05)

Asked: "Route following is spread around (quests, puzzles…): follow several at once, managed in
one place on the Guide page; the ring in the game's view should rest on the same following."

Before, the guide had one target (`MapState.target`, `chosen`, and an `adhoc` place for what was no
goal), one route and one ring; fifteen buttons across the panel replaced it. Now (`guide/track.rs`):

- **What is followed:** the auto guide's pick (`auto`, settled as before; `held` when stepped by
  hand) and up to five places chosen (`tracks`): a quest goal, a choice puzzle's groove, a lock, a
  vault's door, a pin. Each place has its own colour, none a goal tier's; the auto guide's pick
  keeps its tier's.
- **Focus:** one of them (`focus`; none, the auto guide's pick). The compass's distance and label,
  the tracker's "no way through" and the cycle key are about it. The cycle key moves the focus
  through everything followed; with one or none, it steps the auto guide as before.
- **Following from anywhere:** every button that guided (puzzles, locks, vaults, enemy groups,
  collectibles, stories, quests, hand-overs, the Now page, pins, the goal list) now follows: a place
  not followed is added and focused, one followed is focused, the one in focus is let go. The oldest
  goes past five. A place that is a goal is followed as that goal; one that is not (a groove, a lock)
  is followed as a place and shown as a goal of its own.
- **Ending by itself:** a goal followed is matched each frame to the goal standing there (its actor
  loads and unloads: the same place under another id), and let go once no goal has stood there for
  3 s (taken, talked to, done). A choice puzzle's groove is let go when its set is done. A place
  otherwise stays until let go.
- **Kept across runs:** by world, place, label and kind (`track` lines in `minimap.txt`, and
  `track_focus`); read back under a place id until matched to its goal by place.
- **Drawn:** a route each (the one in focus worked out as before, the others every 3 s or 15 m,
  each on its own thread), on the minimap and the big map in its colour, the one in focus bolder; on
  the compass a diamond in its colour (the one in focus as before, with its distance), a chevron at
  the strip's end when off it; in the game's view a ring each within 60 m (one small window each),
  thinner out of focus.
- **Quests followed** (asked next: "several quests must be followed at once", and "the panel's
  roles must change with it, not only the logic"): a track can be a quest. Each frame it goes to its
  own next goal by the auto guide's rule (`target::next_goal`, shared), on to the next once one is
  done, and is let go when the quest is completed or failed; with nothing of it in this region it
  has no goal for now. Kept as `track_quest` lines.
- **The panel, page by page:**
  - Guide: the focus in large type, then "Followed (n/5)": the auto guide's line (skip, back to
    auto) and each track in its colour (a quest as "quest → its goal now", or "nothing of it
    here"), with its distance, a "no route" chip, Focus and Let go; Let go of all.
  - Quests: each quest line has a Follow button (in the quest's track colour once followed);
    pressing the line still picks the quest the auto guide, the needs list and the clues are about.
  - Now: the story card lists the quests followed besides, each with its colour, next goal and
    distance; pressing one brings it into focus.
  - The quest tracker in the game: after the quest guided, the quests followed besides, bold and
    ringed in their colours, then the others.

**One way to follow** (asked next: "can every press-a-name-to-be-guided become the Follow button?
the same thing is there twice now"; then "drop the Guide page's quest/secret/clue toggles, every
tier always on; split the auto guide and the manual following into separate cards"):

- Every followable row on every page (puzzles, locks and their missing rods, vaults, enemy groups,
  collectibles, stories, the quest's needs, hand-overs, deadlines, the Now page's lines, choice
  puzzles) ends in the same Follow toggle (`tw::track_line`, `tw::follow_toggle`): "Follow", or
  "● Following" in the track's colour; pressed again, let go. The row itself does nothing; asking for
  a choice puzzle's answer still follows its groove at once (`ensure`).
- No quest is picked for the guide any more: the auto guide keeps to the main story, and a quest is
  followed with its Follow button like anything else. Pressing a quest on the Quests page only shows
  its needs. What the needs, the Clues page, the Now page's story card and the quest tracker are about
  is the quest in focus, else the main story (`focused_quest`). An older `minimap.txt`'s picked quest
  is read back as a quest followed, in focus.
- The Guide page: an "Auto guide" card (its pick in large type, "in focus" when it is, Focus, Next
  goal, Back to auto, the skipped and their undo, the switches) and a "Followed (n/5)" card (each
  track in its colour, a quest as "quest -> its goal now", "in focus", Focus, Let go; Let go of all).
  The tier filter is gone: every tier is shown and guided to (`goal_tiers` is no longer saved).

## 8. The ways out of a region: the APC and the save points (2026-10-05)

The only way between regions is the APC (there is no other fast travel). Its door is
`APC_Enter_Interact_BP_C`; and every save point (`Base_SavePoint_Interact_BP_C`) carries a
`TravelToAPCAction` that takes the hero to it, but the `Child_SavePoint…NoTravel…` ones (read from
the class tree in memory, build 24045435). Neither was on the maps nor anywhere to be guided to.

- **On the maps**: the "Save" kind is now "Save & travel", with three sorts: save points that take
  the hero to the APC, those that do not, and the APC's door. The scan finds those near; the survey
  (`travel` on its actors, §SURVEY 9) the rest of the region, so the APC shows from anywhere.
- **Guided to**: when the region holds nothing more of the story followed and other regions do
  ("22 in other regions, by APC"), the auto guide's card lists the nearest APC door and the nearest
  save point that takes the hero to it, each with its distance and the Follow toggle (a place track).
- Acasa Marshes has two APC doors, neither behind a data layer: both are shown, the nearer offered.

The NPCs' kind is split as the game names them: `Convo_…` (a conversation: the story's people,
the forge, trades), `Quickchat_Secret_…` (tells a secret), `Quickchat_Quest_…` (a quest's), and the
rest (a line or two). Loot has one class only (`Base_EnemyLootContainer_BP_C`): no finer sort.

## 9. Tracing the guide, and what must come first (2026-10-05)

**The trace** (`src/ui/overlay/trace.rs`): `Mods\doctor\guide.jsonl`, a JSON record a line with
its time and kind — `event` (the auto target changing: from, to, why; the guide's own decisions)
and `state` every 5 s (the target with its source, distance, height, blocked, route; the twelve
nearest goals with wanted/blocked/skipped/followed; the tracks). Capped at 2000 lines, the last
1000 kept. The Debug page shows the last state. Every guide fault below was found from it.

**Blocked, remembered.** A route is kept only for what is followed, and the goal's "blocked" lived
in the route: dropping a blocked goal forgot it, it was picked again — the guide flickered. Now
kept apart (`blocked_seen`) until the hero moves 60 m or 3 min pass. And when 3 different goals
read as blocked within 5 s from one spot (10 m), the spot is what reads as closed (a porch inside a
building's convex hull, measured by Lake Cynon: six goals in a second down to a key 25 m
underground): blocked is not believed for 20 s and the auto target is picked again.

**Survey names.** World Partition names are `…_UAID_<16 hex>_<number>`; the number was dropped on
both sides, so different actors of one editor session shared a name (36 such groups) and the
copies of one actor per streaming cell (503 groups) were kept apart. Now an entry is its name and
place: copies within 1 m merge, ids include the place, the live `loaded` set keeps where each
actor stands, and a shared name matches only within 5 m (30 m for an NPC).

**Empty places.** A survey place the hero stays within 40 m (10 m up or down) of for 6 s with
nothing of it loaded is left out until it loads (Father Jaffer's talk by Lake Cynon, moved on by
the story).

**What must come first, step 1** (`src/guide/requires.rs`): a placement (key door, gear slot) takes
items (`puzzle.items`) the survey says who gives (`payload.items`). Every item held: the place is a
goal with the givers' quest; an item missing: its givers' detail says what for. Step 2 (state
edges: a drain's `Quest.Facts.LymbicForgeWaterLevel2` opening the flooded hall) and the whole
game's graph are next.
