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

- Default keys at the time: minimap F9, marker F6, **compass F10, next goal F11**. All four can be
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
