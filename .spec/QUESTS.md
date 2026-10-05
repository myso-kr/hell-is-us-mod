# Automatic main-story guidance: research (2026-10-03)

Request: order the linear quests in progress and automate guidance all the way to the endgame —
research first. Two lines of research: the game data (doctor and the pak path list, build 24045435)
and the web (walkthroughs).

## 1. Game data

### Main quests = `Quest01`–`Quest06`

- Pak path: `Content/GameData/Quests/Quest0N/` (assets: Quest01 33 · 02 20 · 03 18 · 04 16 · 05 19 ·
  06 8). Each folder holds:
  - `Quest0N_DA` (QuestData);
  - `Quest0N_Started_StatusFact_DA` / `Quest0N_Completed_StatusFact_DA` (Quest01 uses
    `Start_Status` / `Complete_Status`);
  - `Links/*_LinkFact_DA`;
  - `Quest0N_Name_DA` (StringFact — the display name);
  - image and description facts.
- Quest items live in `Content/Items/Quests/Quest0N/` (keys and so on).
- **`QuestData`** (`FactOwnerData`): `IdentityName` ("Quest01"), `MainQuestElement` and
  `QuestElements` (`QuestMindMapElement` {Identity, position} — the people, places and objects on
  the mind map), `QuestLinks`, `Links` (StoryUnit pairs). It is a **relationship graph**, not a list
  of steps.
- **`IdentityData`**: `IdentityName` ("Victor Gaz"), `OwningStoryUnit`, `BaseFacts` (the facts
  attached to that identity).
- **`FactData`**: `DebugName`, **`Priority`** (int), **`Track`** (FName — the topic), `bIsQuest`,
  `AssociatedIdentity`, **`AssociatedQuestData`**. Subclasses: `QuestStatusData`
  (AdditionalQuestStatus), `LinkFactData`, `StringFactData`, `ImageFactData`, …
- **Tracks and priority** (measured, Quest01): a track runs `…_Potential_LinkFact` (Priority 0) →
  `…_Confirmed_LinkFact` (Priority 1). For example, `Quest01_VictorGaz` 0 known → 1 known;
  `Quest01_DetainedPriest` 0 and 1 both unknown; `Quest01_Tania` 0 unknown.
  - A track whose potential fact is known but whose confirmation is not = **the next thing to do**.
  - A track with neither known = a branch not opened yet.
- In the save examined: 20 of Quest01's 30 facts known; Quest02 not started. Only some facts of
  Quest02, 04 and 06 were loaded (asset streaming).

### Golden path — absent from the release build

- `GoldenPathData.QuestSteps` (`QuestStep` {World, TeleportMarkerTag, StepDescription}) and
  `GoldenPathSubsystem` exist only as classes (CDO). No data asset among the pak's 102,259 paths —
  it was a development teleport sequence. Not usable.

### Already in place (`GUIDE.md §2–5`)

- The save's `KnownFacts` / `FactTags` / `QuestStates` are read (`src/read/knowledge.rs`), and the
  places that give a payload fact **not yet known** become goals (`src/guide/goals.rs`, tiers
  Quest / Secret / Clue). Quest tier = the fact has an `AssociatedQuestData`.

## 2. Web — the main line (Fextralife, Game8, guided.news and others; see sources for detail)

| # | Data | Walkthrough title | Act | Main regions (in order) | What opens the next |
|---|---|---|---|---|---|
| 1 | Quest01 | Family Reunion | 1 | Senedra Forest (Caddell Farm) → Acasa Marshes (Jova) | APC key → Acasa; Vitalis house key |
| 2 | Quest02 | Family Legacy | 1 | Vyssa Hills → Lake Cynon (Lymbic Forge) | Keystone of Grief; Blue Flower conversation → Lake Cynon |
| 3 | Quest03 | Keystone of Terror | 2 | Talju → Marastan → Arcas Spire | Blood Queen's Sword; time loop |
| 4 | Quest04 | Keystone of Ecstasy | 2 | Pathem Abbey → Lethe Library → Vyssa mines → Plains of Mist | Land Grant |
| 5 | Quest05 | Keystone of Rage | 2 | Jeljin → Lethe Ministry of Culture → Auriga Museum | Two Iron Seals |
| 6 | Quest06 | Into the Unknown | 3 | Lake Cynon → Mount Obek (Eye of God) | Four keystones, four orbs — one ending |

- The mapping from data number to walkthrough title was **confirmed by quest item names**:
  Quest02_KeystoneGrief, ForgeHammer; Quest03_TaljuParkingLotKey, ArcasSpireElevatorDoorKey;
  Quest04_LetheLibraryKey, VyssaHillsMinesKey; Quest05_KeystoneRage, AurigaMaintenanceKey,
  JeljinCraneKeys.
- **Act 2's three keystones can be done in any order** (walkthroughs recommend Terror → Rage →
  Ecstasy). Obtaining the first keystone advances time, and some good deeds fail.
- Sources: https://hellisus.wiki.fextralife.com/Walkthrough ·
  https://game8.co/games/Hell-is-Us/archives/543745 ·
  https://guided.news/en/guides/hell-is-us-walkthrough-the-complete-solution-to-all-3-acts/ ·
  https://gamerant.com/best-order-to-get-keystones-hell-is-us/

## 3. Design proposal

1. **Current stage.** Main quests = `QuestNN_DA`, in NN order. Current = the lowest number whose
   completed-status fact is unknown ("in progress" if its started fact is known, otherwise "next").
   Act 2 (03–05) is order-free: of those started, the one with the most progress (share of facts
   known); if none, 03.
2. **Goal ranking.** Among existing goals (places that give an unknown fact), places that give
   **facts of the current main quest** come first. Within those: (a) the confirmation fact
   (Priority 1) of a track whose potential fact is known → (b) the first fact (Priority 0) of a new
   track → (c) the rest. Ties go to distance.
3. **Out of range.** When the current quest has no goal in the loaded range (another region), show
   the "main regions" from the table above as the next destination (region names only — walkthrough
   text is not copied). Region coordinates could be collected from APC destinations and region entry
   (follow-up).
4. **Panel.** A guide card such as "Main story 3/6 · Keystone of Terror — clues found 12/18" with the
   next 2–3 tracks, and a **"Main story, automatic"** guide mode (replacing and extending the
   earlier automatic mode, nearest quest goal).
5. **Display names.** Read the in-game string of `Quest0N_Name_DA` (StringFact) for a localised
   name where possible; fall back to the number.

Limits: when a fact comes from an NPC conversation there is no place object to mark as a goal; the
most that can be shown is "talk to …" using the track (person) name. Act 2 order is the player's
choice.

## 4. Implementation — quest journal and tracker (2026-10-03)

Request: overlay the quest in progress, with its real name and description, at the right middle of
the screen, and choose the current quest in the panel (MMORPG style) — side quests as well as main
quests.

### Reading FText (measured, build 24045435)

- `FText` = TextData pointer + flags. TextData: +0 vtable, +8 reference count, **+0x10 history**.
- **history +0x30 → +0x08 = FString — the localised display string** (for Quest01, the localised
  "Family Reunion" and its description). If absent, history +0x20 → +0x10 = FString — the source text
  (English "Family Reunion").
- `quests::ftext(m, at)` (`src/guide/quests.rs`) is used for every TextProperty
  (`StringFactData` / `LinkFactData.Description` +0x60, `SecretRow.Title`, …).

### Side quests = good deeds

- Apart from the main quests (Quest01–06), the game's only quest lists are **good deeds, mysteries
  and time loops** (the datapad's exploration tab). Side quests are therefore good deeds.
- Save: `CharlieSaveGame.Player.SecretsState.GoodDeeds` = `CharlieSecretEntryState` {Identity: Guid,
  bIsNew, State} (26 in this save, 3 with State 1 = in progress). State: 0 unknown · 1 started ·
  2 completed · 3 rewarded · 4 failed (inferred; only 1 measured).
- Definitions: **`SecretsSubsystem.GoodDeeds`** (a WorldSubsystem, always loaded). `GoodDeedData` is
  0x70 bytes: Guid +0x18, StartedTag +0x28 (`Secrets.Facts.<Name>Started`), CompletedTag +0x30,
  `Title` FText +0x38. (`GoodDeeds_DT` was not in memory even with the exploration tab open — the
  subsystem reads the data table and releases it.)
- Titles: only those the game has shown once carry a localised string (history +0x30). The rest are
  **string-table references**: history +0x10 = TableId FName
  (`/Game/GameData/Secrets/UI_Secrets_ST`), +0x18 = text key's pool index (u32).
  `UStringTable` +0x28 → FStringTable, +0x20 KeysToEntries (32-byte elements: key index u32, entry
  pointer +8), entry +0x10 = source FString (English).
  - The localised title is used when present, otherwise the English source. A localised title is
    remembered in `quests.txt` and never overwritten by English.
  - Measured: all 26 named; the 3 in progress were The Golden Watch, Asunder and Victor's Vigil.

### Main quests

- The `QuestData` object plus the facts tied to it by `AssociatedQuestData` (tracks and text). Name =
  the Description of the fact on the `Name` track; description = the `Desc…` track.
- Status: the Complete / Start fact on the `Quest Status` track known → completed / in progress; any
  fact known → in progress.
- **Lead** = a track that has started but is not fully known (plus the description of the last fact
  learned). Measured: Quest01 24/30, lead "Sabinian Officer".
- Names and descriptions are also remembered in `quests.txt` so they survive the assets streaming
  out.

### Cost

- The whole GUObjectArray (~350,000) is walked **4,000 objects per engine step** (with a per-class
  role cache) — about 90 steps per pass. The first steps take ~100 ms for class-name lookups, later
  ones are short. The journal compares against knowledge (KnownFacts) and good-deed state every step.

### Following and guidance

- Good deeds and goals: if a goal's new tag starts with a good deed's tag prefix
  (`Secrets.Facts.GoldenWatch`), that goal advances the good deed.
- `MapState.quest`: the quest key chosen in the panel (`Quest01` or a good-deed GUID). When unset,
  **Main story, automatic** = the lowest-numbered quest in progress (else the first not started).
  When the chosen quest ends, it reverts to automatic.
- Automatic guidance: the nearest goal that advances the followed quest (the goal's new fact has
  that quest as `AssociatedQuestData`, or its new tag starts with the good deed's prefix); if none,
  the nearest quest goal.
- Tracker (overlay, `src/ui/tracker.rs`): top right of the game window, under the minimap when it shows
  (moved 2026-10-04: the game opens its pop-ups at the right middle), 340 × up to 560. The
  followed quest is expanded (name, kind, lead count, description in 4 lines, 3 leads); up to 5
  other quests in progress get one line each.
- Text is drawn with GDI (Malgun Gothic, greyscale anti-aliasing) into a DIB and blended using
  brightness as coverage (`src/ui/pen.rs`) — the raster's stroke font only knows digits and compass
  letters. Redrawn only when the content changes.

## 5. Following a quest = routing plus NPC, item and hand-over goals (2026-10-03)

Request: route finding should switch on and keep updating while a quest is tracked, and guide to the
required NPCs, items and clues.

### Following

- Choosing a quest in the panel sets `route = true`, turns automatic guidance on and resets the
  goal.
- `MapState.chosen` (not saved) = a goal picked by hand from the list or with F11; kept until used
  up.
- Otherwise, every frame: if the current goal does not fit the followed quest (the quest changed, or
  one of its goals came into range), drop it and take the nearest fitting goal. When a goal is used
  up, move to the next — the route updates with it.
- Fitting goals (`wanted`): if any goal advancing the followed quest is loaded, only those. If none:
  for a main quest, any quest goal; for a good deed, nothing (the tracker says the followed quest
  has no goal in this region).

### NPCs (conversation)

- `NpcActor`'s `FlowComponent.RootFlow` (+0x198) = FlowAsset. `FlowAsset.Nodes` (+0x50) is a
  `TMap<FGuid, UFlowNode*>` with 32-byte elements, node at +0x10. `FlowNode_Payload` +0x1D0 =
  PayloadData (ContainedFacts set +0x18, TagFacts +0x68, ItemsToAdd +0x88).
- Topics (`FlowNode_TopicSubGraph` TopicAsset +0x218, `FlowNode_SubGraph` Asset +0x1D0) are soft
  references: the FSoftObjectPath's AssetName FName is at soft pointer +0x10. The loaded FlowAsset is
  looked up by name (collected by the `quests.rs` pass), two levels deep.
- Measured: payloads of `VictorGaz_ConvoTopic_*` and `DetainedPriest_ConvoTopic_*` give Quest01
  facts (e.g. `SabinianOfficer_FamilyReunion_Quest_TextFact_DA`) and items
  (`Quest01_FamilyHomeKey_item_DA`). `Conversation.TopicsUnlock.*` tags alone do not make a goal.
- Topics stream in and out, so NPCs are re-read every scan.

### Hand-overs (giving an item for a good deed)

- `TradeGiveItemRuneComponent.Rune.ValidTrades` = `DonationRequest` {Item +0, TradeReplies,
  Payload +0x18, …}. The reward payload's tag (e.g. `Secrets.Facts.<Deed>Completed`) ties it to the
  good deed → a "hand over: <item>" goal.

### Items

- `ItemsToAdd` entries in an interactive object's payload that are `/Items/Quests/` or
  `/Items/Secrets/` items are goals until taken. A name starting `QuestNN_` belongs to that main
  quest (`Goal.keys`). Measured: `Quest03 PholGuardGeneralScroll`, `SenedraCaddellsTreasureNote02`.

### Limits (measured)

- The Golden Watch (`Caddell_GoldenWatch`) was already in the inventory, and the NPC who takes it is
  not loaded in this region (Senedra). Even a reverse-reference scan found no world object pointing
  at the good deed's tag or item. The hand-over goal appears once the player reaches that NPC's
  region. (The survey now covers other regions: `SURVEY.md`.)
- `GoodDeedData.LocationNameFact` is empty in the release data (null for all 26) — good-deed
  locations cannot be shown.

### Conditional markers (2026-10-03, corrected after measurement)

- Reported by the user: following Family Reunion led to the Arcas Spire door, without the item it
  needs. Cause: the goal was `SenedraForestArcasSpireDoorOpening_PayloadInactive_Interact_BP_C` — not
  something to interact with, but a marker (GrantPayloadComponent) that pays out when something else
  happens (the door opened with a key). It also belonged to Quest03 (not started); it was picked by
  the "any quest goal" fallback because Quest01 had no goal in the region.
- `Goal.gate`: among `_PayloadInactive_` classes, names ending in `Visited`, `Proximity` or `Met` are
  **Visit** (going there is enough); the rest are **Conditional** (a door opening, a puzzle solved —
  going there is not enough). Measured, 13 in this region; the conditional ones were 3 doors, a
  storeroom, a time-loop completion and the Arcas Spire door.
- Automatic guidance never picks a conditional goal, nor **an item of a main quest not yet started**
  (`Goal.keys` naming a NotStarted quest). Such goals stay in the list marked "needs a key or puzzle
  first" (`NEEDS_SOMETHING_A_KEY_A_PUZZLE`).
- Which key a door needs is not on the door actor (its PayloadRune has a single fact), so guiding to
  the key from the door is not possible.

## Tracking the save object (2026-10-03)

- The game creates a new `CharlieSaveGame` object on every save (the last few stay in memory). Known
  facts and good-deed state come from the one with the latest SaveDate.
- History: the list of save objects used to be refreshed by a full GUObjectArray search (~0.4 s)
  every 60 s, so a completed quest could take up to a minute to show after the game saved.
- Now the quest pass (which walks all objects a little at a time, every few seconds) also collects
  save objects; when a new one appears, knowledge is re-read immediately.
- Measured (`doctor saves`, 2026-10-03): 3 save objects in memory (slots rotate). The object with the
  latest SaveDate updates live even without saving — facts 541 → 542, tags 132 → 134, date
  unchanged. The mod reads it every 2 s, so progress shows within seconds. Quest completion was
  confirmed to show immediately in game.

## The game's HUD, and where the overlays go (2026-10-05)

Read from the game's own widgets (`UI/HUD/*`, CanvasPanelSlot anchors and offsets on a 1080-tall
layout, scaled by the window's height), not guessed from screenshots:

| Game HUD | Where |
|---|---|
| `HUD_PlayerStatus_SUMG` (weapon, health, stamina, Lymbic) | top left, x 94, y 46, about 750 wide |
| `HUD_NotificationLog_SUMG` | left edge, middle, 500 tall |
| `HUD_SecretStarted…`, `HUD_CombatItemPickUp…` notices | right edge, middle, 100 in |
| `HUD_InteractInput_SUMG` | centre |
| `CombatHud_SUMG` wheels (Lymbic, drone, items) | bottom left (138, 480) and bottom right (138) |
| `MainSubtitles_UMG` (and quick chats) | bottom centre, 90 up |
| The compass, when raised | top centre, briefly |

So the tracker (with its context lines, context.rs) stays in the top-right column under the
minimap and ends `NOTICE_HALF` (200, scaled) above the screen's middle; the banner (banner.rs) is
under the compass strip at the top centre, for 15 s.
