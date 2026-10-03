# Game-data survey: maps, spawners, vaults, puzzles and the game's text (2026-10-03)

The survey reads the game's own data offline, on the player's PC, so the guide knows about things
that are not loaded around the hero. One run of `tools/survey` (C# .NET 8 + CUE4Parse) produces:

- `Mods\survey\<World>.json` — every placed actor in a world's World Partition maps that hands
  something out (pickups, devices, markers), NPCs and their conversations, hand-overs, puzzles
  (dials, keypads, item placements) and vault doors, with position and save GUID (§3–§7).
- `Mods\survey\flows.json` — the conversations NPCs run and what their nodes hand out.
- `Mods\survey\spawners.json` and `vaults.json` — the game's own spawner and vault tables
  (see `FEATURES.md §3`, F7 and F8; read by `src/guide/tables.rs`).
- `Mods\locale\` — the game's text for every culture, plus `names.tsv` (see `I18N.md §2`).

Live puzzle answers near the hero are read from memory, not from the survey (`src/read/puzzles.rs`,
`FEATURES.md §3`, F6). Since §8 the survey runs on its own; nobody needs to type a command.

The original request was a complete survey of quest items and where each one is: without it, the
player could reach an entrance and still not know where the item was.

## 1. Why live goals cannot find everything

Goals (`src/guide/goals.rs`) come only from actors **loaded in memory right now**. World Partition
streams only the cells around the player, so:

- Items, NPCs and devices outside the load range (roughly 80–200 m) are unknown.
- Items in other regions (separate worlds) are unknown.
- Behind doors and puzzles (underground areas and so on), even a loaded cell does not say where the
  key comes from.

What is needed is a list surveyed in advance from **all** of the game data: an offline database.

## 2. Data sources (measured)

| Source | Contents | Confirmed |
|---|---|---|
| Pak (IoStore, AES) `Maps/<Region>/<Region>_Root_WP/_Generated_/*.umap` | Cooked World Partition **streaming cells**; every placed actor lives here (but see §7 S2) | 4,043 cells: LakeCynon 868 · LethePropaganda 788 · LetheLibrary 550 · SenedraForest 520 · Marastan 310 · AcasaMarshes 253 · PlainsOfMist 242 · VyssaHills 202 · AurigaMuseum 108 · Jeljin 102 · Talju 99 |
| Same pak, `Maps/<Region>/*.uasset` (Town, TownSecrets, SophieHouse, …) | Sub-levels for level instances and data layers | Paths only |
| Components of actors in a cell | `PayloadRuneComponent` (ItemsToAdd, ContainedFacts, TagFacts), `SaveIdentifierRuneComponent` (Guid), `FlowComponent` (RootFlow), `TradeGiveItemRuneComponent` (ValidTrades), `InteractionActionComponent` | Structure confirmed through runtime reflection (`GUIDE.md`, `QUESTS.md §5`) |
| Save `World.RegionStates[]` | Per region (LevelPath), `ElementStates[]` = {Identifier: Guid, RuneStates, Data} — **the state of placed objects, keyed by GUID** | 123 measured in Senedra |
| Save `KnownFacts` / `FactTags` / inventory | Facts and tags already known, items held | Already read (`src/read/knowledge.rs`, engine inventory) |
| Item asset paths | `/Items/Quests/QuestNN/…`, `/Items/Secrets/<Region>/…` | `QUESTS.md §1`, `goals.rs` |

The AES key: how to find it in the executable is described in `MAP.md §1`; the key itself is not
written anywhere. Tools: retoc (already in use) and the .NET 8 SDK (installed on the development
machine), which makes CUE4Parse — FModel's engine, with IoStore, UE 5.5 and unversioned-property
support — usable.

## 3. Overall structure

```
[game paks] --(offline survey tool, on the player's PC, once per patch)--> Mods\survey\*.json
                                                                             │
[game memory] --(hiumod at runtime)-- knowledge, save state, current world --┼--> goals (near = live, far = DB)
                                                                             └--> tracker, panel, compass, route
```

### 3.1 Offline survey tool `tools/survey` (C# .NET 8 + CUE4Parse)

- Inputs: the Paks folder, the AES key (found by the tool at run time, never saved to a file; see
  §7 S5), and **mappings (`.usmap`)**.
- Mappings: cooked assets use unversioned properties, so a schema is required. `hiumod doctor usmap`
  writes a `.usmap` from runtime reflection, which hiumod already reads in full
  (`src/unreal/usmap.rs`). CUE4Parse reads blueprint classes from each asset's BPGC.
- Processing: every cell umap of each `_Root_WP`, and for each actor export:
  - class (import path), root component `RelativeLocation` (cell actors are in world space), cell
    name;
  - `SaveIdentifierRune.Identifier` (Guid) — **the key that matches the save**;
  - `PayloadRune`: ItemsToAdd (item paths), ContainedFacts (fact paths), TagFacts (tags);
  - NPCs: `FlowComponent.RootFlow` path → that flow asset (plus its topic sub-graphs) and the
    contents of each `FlowNode_Payload`;
  - `TradeGiveItemRune.ValidTrades`: the wanted item and the reward payload;
  - classes named `_PayloadInactive_` (conditions and visits) told apart from `GatherSingleUse`
    (pickups).
- Fact assets (`FactData`) are read separately to attach `AssociatedQuestData`, Track and
  Description (the FText source key).
- Output: `Mods\survey\<World>.json` and `Mods\survey\facts.json` (as built, there is no
  `facts.json`; the tool writes `flows.json` instead — §7 S3). These are **never committed or
  distributed** (they are derived from game data). The build ID is recorded alongside so the player
  is told to re-run after a patch (§8).

### 3.2 Runtime `src/guide/survey.rs`

- On start, reads `Mods\survey` and indexes it by world. If it is missing, only live goals are used,
  as before (no loss of function).
- **State** (what is still left):
  1. If the save's `RegionStates[current world].ElementStates` holds that GUID and its Data means
     "used", it is done. (Interpreting Data needed measurement — compare saves before and after a
     pickup; settled in §7 S4.)
  2. If the facts and tags of the payload are already known, it is done (the same rule as
     `goals.rs`).
  3. If the item is already held, it is done (inventory).
  4. Within the loaded range, the live value (`bHasBeenActivated`) wins.
- **Merging goals**: live goals plus DB goals, one per GUID. A DB goal carries `source: Survey` and
  its cell name on `Goal`.
- **Quest links**: item `QuestNN_` → that main quest; a fact's `AssociatedQuestData`; good deeds by
  reward-tag prefix or hand-over item. Together these give, for each quest, every item it needs,
  where each one is, and whether it is already obtained.
- **Other regions**: when a goal is in another world, the tracker shows the region and says to take
  the APC; the panel lists such goals by region.

### 3.3 Guidance

- A DB goal in this world but outside the load range: route on the navmesh to the nearest reachable
  point, then a dotted line for the rest (reusing the blocked-route handling). Nearer, the cell loads
  and the goal becomes a live one, with an exact position.
- Goals behind doors or puzzles: guide first to the key or note items of the same quest (from the
  DB), extending the blocked-goal "helper" (now in `src/guide/target.rs`) to the DB.
- Panel: a **Needed** list on each quest card — item, NPC or device, region, distance, state (left
  or obtained); click to be guided there.
- Map: DB goals are diamonds too (outline only until loaded); other floors are dimmed (reusing the
  floor display of `MAP.md §11`).

## 4. Stages and checks (user confirmation after each)

| Stage | Work | How it is checked |
|---|---|---|
| S1 | `hiumod doctor usmap` — reflection → `.usmap` | CUE4Parse/FModel read the Charlie classes |
| S2 | Survey skeleton: open the paks, list cell umaps, print actor class and position (one region) | Known objects in Jova (Acasa) match their runtime coordinates (e.g. `Quest01PictureC`) |
| S3 | Extract payloads, GUIDs, NPCs and trades; JSON for every region | Against live goals: every goal within load range is in the DB |
| S4 | Save state: settle the meaning of `ElementStates.Data` by comparing saves before and after a pickup | Picking up one item turns it to "obtained" |
| S5 | Runtime merge, quest links, the panel's Needed list, other-region guidance | Family Reunion: every remaining item is listed with its location |

## 5. Risks

- **Data layers.** Some actors are active only in the data layer of a given story stage; an actor
  can be in a cell and still not exist right now. Plan: extract cell/actor data layers and filter at
  runtime by whether the layer is on (loaded). Result in S3: cooked actors have no `DataLayer*`
  property (0 found), so membership sits with the cell or the runtime hash. The tool still records
  `layers` if any such property appears; revisit if it matters.
- **Meaning of Data not settled.** Without the `ElementStates.Data` format, rule 1 is skipped and
  rules 2–4 alone are used (facts, tags and inventory decide most cases). Settled in S4.
- **CUE4Parse compatibility with UE 5.5 and the game's custom versions.** Would show up in S2
  immediately. Fallback: `retoc to-legacy` plus a small Rust parser for the few components needed —
  reflection already provides the schema. Not needed: CUE4Parse worked (§7 S2).
- **Distribution.** Survey results, the key and anything extracted stay out of the repository. The
  tool's code is in the repository; it runs on the player's PC.

## 6. Decisions to make

1. Tool implementation: **C# + CUE4Parse** (recommended — a proven, fast parser) vs. a Rust parser
   of our own (no external dependency, slow to build). **Decided: C# + CUE4Parse** — built in S2
   (`tools/survey`, CUE4Parse 1.2.2).
2. Scope: **quest and good-deed items, keys and notes, and devices and NPCs with a payload**
   (recommended) vs. every pickup (consumables and materials included). **As built** (from
   `tools/survey/Program.cs`): the tool does not filter by item type — it keeps every placed actor
   that has a payload, a conversation, a hand-over, a puzzle or a vault door. The runtime then uses
   quest and secret items for goals and counts collectibles by item folder (`COLLECT` in
   `src/guide/survey.rs`).

## 7. Progress log (2026-10-03)

### S1 — Mappings (done)

- `hiumod doctor usmap` → `Mods\doctor\HellIsUs.usmap` (usmap v0, uncompressed): 14,224 loaded
  classes and structs plus 2,249 enums, 1.8 MB.
- UEnum name table at +0x40: (FName, i64) pairs; the `Enum::` prefix is stripped. FProperty
  `ArrayDim` at +0x30; inner-type pointers from +0x70.

### S2 — Survey skeleton (done)

- `tools/survey` uses C# .NET 8 + **CUE4Parse 1.2.2** — the last release for net8 (the newer
  1.2.2.2026xx releases are .NET 10 only), with UE5_5 support. Oodle comes from the
  `oo2core_9_win64.dll` retoc had already downloaded. Nothing extra to install.
- **Placed actors are not only in the cells** (`_Generated_/*.umap`) but also in
  `<Region>_Root_WP.umap` itself (always loaded). Many quest items are there: 122 of Acasa's 178.
- **Component values usually live in the blueprint template.** The placed copy's `Template` → that
  template's `Template` → … chain is merged, the copy's own values on top. Example: PayloadRune's
  ItemsToAdd is on `Quest01PictureC_…_BP_C:PayloadRune_GEN_VARIABLE`.
- **Positions compose the attachment chain.** When the root component is attached to another actor
  (`AttachParent`, an export index in the same package) its location is relative, so the parents'
  location, rotation (FRotator → quaternion) and scale are composed.
- Check: matches the live goal positions in Acasa **exactly**, e.g. VitalisOfficeOpened
  (−7699, −13903, 506) and the Victor conversation (−7094, −16770, 1310).

### S3 — All regions (done)

- 11 worlds in 2 min 14 s: 1,174 actors and 361 conversation graphs (NPC → RootFlow → topics and
  sub-graphs, recursively).
- Quest items (pickups): Quest01 8 (Acasa 6, Senedra 2) · Quest02 28 (LakeCynon 16, Vyssa 12) ·
  Quest03 15 (Marastan 10, Senedra 4, Talju 1) · Quest04 17 (Vyssa 10, LetheLibrary 6, Plains 1) ·
  Quest05 24 (Auriga 12, Jeljin 6, LethePropaganda 6) · Quest06 10 (LakeCynon). Good-deed and secret
  items: 138. Items given in conversation: 19, of which 15 are quest items (house key, keycard,
  library key, …).
- Output: `Mods\survey\<World>.json` = {name, class, cell, at, guid, payload{items, facts, tags},
  flow, trades}; `flows.json` = {path: {payloads, subgraphs}}. Not committed or distributed.
- Data layers: cooked actors have no `DataLayer*` property (0); membership is on the cell or runtime
  hash side. To be investigated if it becomes necessary.
- Manual run (at the time):
  `survey --paks <Paks> --usmap <usmap> --oodle <dll> --aes <key> --world all --out Mods\survey`.
  Moving key discovery into the tool so one command runs everything was part of S5.

### S5 — In-game integration (done; before S4, "obtained" was judged from knowledge and inventory)

- **`hiumod doctor survey [world]`**: writes the mappings, then runs `tools/survey`. AES key
  discovery moved into the C# tool (`AesKey.cs`, .NET's built-in AES — the same method as the
  Python script). Oodle comes from `Mods\tools\oo2core_9_win64.dll` (CUE4Parse downloads it if
  missing). `--game <install dir>` determines every other path. Running a single region still merges
  into `flows.json`.
- **`src/guide/survey.rs`**: reads `Mods\survey` into per-world entries (name, class, position,
  items, facts, tags, wanted items; an NPC's entry merges the payloads of its whole conversation
  graph). The trailing number of cooked names (`_UAID_…_1325171520`) does not exist at runtime, so it
  is stripped before matching.
- **What is left** (`Entry::left`): unknown facts, unknown tags (except `Conversation.`), items not
  held — except that once all facts and tags are known, the items count as obtained (keys and notes
  leave the inventory once used).
- **Merging goals**: survey entries in the current world that are loaded (`goals.rs` collects their
  names) are left to the live goals; of the rest, those with something left become goals (the top
  bit of the goal id marks a survey entry). Measured in Acasa (Jova): 57 live + 22 from the survey.
- **Quest links**: item path `/Items/Quests/QuestNN/` → QuestNN; fact name → its main quest (facts
  of the loaded QuestData); good deeds by tag prefix.
- **Needed** (`Snapshot.needs`): for each quest in progress, the entries in every world and whether
  each is left. Measured, Family Reunion: 8 of 12 places left — Acasa 6 (riddle note, Picture C, UN
  travel map, Lymbic lock note, two conversations for Victor's house key) and Senedra 2.
- **Panel**: the quest card shows "Needed: n/m left", the 8 nearest in this region (click to be
  guided; a loaded one becomes a live goal) and a count for other regions (take the APC).
  **Tracker**: "Needed: n here · m in other regions".
- Limitation: an NPC merges every branch of its conversation graph, so it is hard to learn
  everything and it can stay "left" for a long time. S4 (the save's placed-object state) was planned
  to help.

### S4 — Placed-object state in the save (done, 2026-10-03, after the user picked up Picture C)

- `CharlieSaveGame.World.RegionStates[]` (element 0x80: LevelPath, Elements map, **ElementStates**
  at +0x70) → `CharlieSaveWorldElementState` (0x70: Identifier Guid, RuneStates map at +0x10, Data at
  +0x60 = `TArray<InstancedStruct>`).
- Measured (Acasa, 64 entries): Pictures A and C, both picked up, each have a
  `PersistentActorSaveGameState` under their own GUID (ActorData 14 B, ComponentsData 1 B,
  identical for both: a "consumed" marker). The riddle note, UN map and lock note, not picked up,
  have **no record**. Picture C's record appeared when it was picked up. The rest are
  `SerializeSpawnerState` (enemy spawners).
- Rule: if a survey entry's GUID is in the save, it was taken or used, so nothing is left. NPCs are
  the exception (the record may only mean they were spoken to once).
- Result (Family Reunion): places left 8 → 6 (Picture C and the first-aid kit in Senedra corrected
  to "obtained").

## 8. Automatic reading and the .NET 8 runtime (2026-10-03)

The player no longer types `doctor survey` (`src/infra/gamedata.rs`).

- When the panel sees the hero in control and a .NET 8 runtime present, it calls
  `gamedata::next(build)`:
  - survey (followed by locale) when `Mods\survey` has no JSON, or `Mods\survey\BUILD` differs from
    the current Steam build;
  - otherwise locale when `Mods\locale\names.tsv` is missing.
- It runs this same executable as `doctor survey` or `doctor locale`, with no window. When the run
  ends, `generation` increments; the attached engine's `Guide::fresh` re-reads the survey and the
  tables, and `i18n::follow_game` re-reads names when they are empty.
- An older survey without a stamp is taken as the current build's and stamped.
- On failure the sidebar shows the reason; there is no automatic retry (run it again from the
  console).

Runtime detection (`src/infra/runtime.rs`): a `dotnet` host whose `--list-runtimes` lists
`Microsoft.NETCore.App 8.`, tried in this order — PATH, `%ProgramFiles%\dotnet` (this process's
PATH predates an install it just ran), `Mods\dotnet`. The panel only calls `available()`, which
re-checks in the background every 15 s.

Installing happens only from a button the player presses, because it installs software:

1. winget `Microsoft.DotNet.Runtime.8` — machine-wide, UAC prompt, kept current by Windows Update.
2. If winget is missing or fails: Microsoft's `dotnet-install.ps1`
   `-Runtime dotnet -Channel 8.0 -InstallDir Mods\dotnet -NoPath` — this user only, no
   administrator. Measured: 11 s, 71 MB.

Rejected by the user: shipping the survey tool self-contained with its runtime (+31 MB to the zip).

Research behind this: Microsoft's deployment documentation (framework-dependent vs.
self-contained), winget's unattended-install options, the dotnet-install script (non-admin), and
the "You must install .NET" dialog that a framework-dependent apphost shows.
