# The requirement graph

Requested 2026-10-05, after the guide sent the hero to a key in a flooded hall whose drain was not
yet set off: what each place needs before it can be used, for the whole game, from the start to the
end — quests, NPCs, items. The guide then goes to the first thing of a chain that can be done now
("the large gear → its slot → the lever → the drain → the key"), not to the nearest goal.

## 1. Where the edges are in the game's data

Read from the levels by the survey tool (`tools/survey`, `--peek` with a path match for
sub-objects), confirmed on the Lymbic Forge (Lake Cynon):

| Edge | In the level | Example |
|---|---|---|
| A receiver is set off by any of its activators | the receiver's action components: `Activators[].InteractableActor` | `LymbicForgeWaterDrainStep01_PayloadInactive` ← `ForgeMachineLever` |
| An action can be used only when a condition holds | an action's `ActionCondition.Condition`, a sub-object | the lever: `InteractableStateCondition` on `MediumGearPlacement`, `ExpectedState` 1 (used) |
| A placement takes items | `puzzle.items` (`ItemPlacementAction`) | `LargeGearPlacement` ← `Quest02_GearLarge_Item_DA` |
| A place gives items, facts, tags | `PayloadRuneComponent` | the drain: `Quest.Facts.LymbicForgeWaterLevel2` |
| An NPC gives what its conversation gives | the flow's payloads, through its sub-graphs (`flows.json`) | |

Conditions seen across the game (count of actors): `IsHeroInTrigger` 87, `DoesHeroHasFact` 49,
`IsSingleUseInteractableActivated` 27, `InteractableState` 26, `DoesHeroHasNotFact` 22, `And` 14,
`IsAnotherSceneComponentAtLocation` 14, `IsHeroNotInCombat` 4, and a few others. The survey writes
each as `{type, actor, state, tags, facts, items, all}`, every level actor it names recorded
whatever the property; where the hero stands (in a trigger, not in combat) is met by going there and
is not a step of the graph.

## 2. The graph (`src/guide/graph.rs`)

A node per surveyed interactable or NPC (copies per streaming cell are one). Needs: its activators
(`Any`), its conditions (`All`, `Fact(tag, has)`, `Used(actor)`), the items it takes; gives: its
payload's items, facts and tags, and an NPC's conversation's. `chain(node, state)` walks back from a
node over what is not met — an item to a giver not used, a fact to a giver, an actor to its node —
to the first node whose needs are all met; `reach()` propagates, from nothing known, which nodes the
game can be played to.

`hiumod doctor graph` prints it (and writes `Mods\doctor\graph.txt`). First run, 2026-10-05:

- 2468 nodes in the 11 worlds; 2397 can be reached from nothing known; 71 cannot.
- All 71 wait on 15 things no place gives: boss kills (`GraveyardRushBossKilled`,
  `HammerOfModusEncounterKilled`), items the quests' scripts hand out (keystones, keycards, the hero's
  necklace, Colonel Vaas's office key), and a few facts set by scenes.

## 3. The guide on the graph (2026-10-05)

`Graph::gate` (attached.rs, each step): a goal whose node has a chain of two or more against the
save (used = save GUIDs with a state; known = facts and tags; held = items) is held back
(`Gate::Conditional`, "first: …") and the chain's first node made a goal of its quest (or a goal
already there takes the quest on), so the story's guide goes there. A node that keeps no state (an
area trigger) holds nothing back. A chain that ends in what no place gives leaves the goal alone.

Deadly water names no drain in the game's data (the Lymbic Forge's `DeadlyWaterStaticForge01/02`
receivers have no activators and nothing refers to them; a blueprint or a sequence empties them).
So the live pass keeps the deadly water boxes as found (`Scene::pools`, indoors too, not cached) and
`Graph::flood` holds back a goal inside one below its surface ("under water: drain it first"), the
nearest unused drain of the world (`…Drain…`, or giving a `WaterLevel` tag) guided to through its
chain. Measured by the Forge: the two keys under water held back; the axle gear held back behind
"the wall panel ← the first-generation Lymbic activator", the activator made the story's goal.
Whether a drained box goes from the live pass is to be seen once drained.

## 4. Order and position puzzles, objects moved (2026-10-05)

Asked: "devices to strike in an order" and "interactions with objects" in the graph too. In the
receivers' actions:

- `MultiActivatorsActivationAction` (and `LymbicLock1stGenAction`): `HasOrder` (the order is the
  `Activators` list's), `Wait for All Activators`, `HasTimer`/`TimerDuration`.
- `MultiActivatorsStateAction` (`…MultiPositionsLogicAction`): `ActivatorSolution`, the position each
  activator must be turned to.

The survey records them as `logic` {order, all, timer, solution}. Found: 2 order puzzles (the
first-generation Lymbic locks of Acasa and Lake Cynon: 4 and 5 activators, 300 s), 14 position
puzzles (the Hermit's secret, the Sconces of Knowledge's 10, Jeljin's mausoleums, the Eye of God,
the Forge foyer, the museum's statue heads…), 2 all-of. Such a receiver needs every activator (not
any). The guide's chains say what kind of puzzle a step is ("5 activators in order, within 300 s");
the answer — each device numbered in its order, its way and distance from the puzzle, the position
it must be at — is on the Puzzles page's "Order and position puzzles" card, behind Show answer.

An object brought to a place (`IsAnotherSceneComponentAtLocationCondition`, 14) is that object used.

## 5. What the worlds give (2026-10-05)

`--grep` (the survey tool; several names at once, `a|b`) finds what refers to an item or a fact in
any package's bytes. The keystones and the Arcas Spire elevator key are given by no place: each
world's `CharlieWorldSettings` has `FirstEnterWorldPayloadData` (on first entering: Senedra gives
`Quest.Facts.Act01Started`) and `BossFightRoomCompletedPayload` (the boss fight won: Senedra's gives
the Keystone of Terror, the elevator key, `Quest.Facts.TerrorKeystoneGathered`). The survey writes
them as the world's `gives` {enter, boss (with where the fight's room is)}; the graph has a node for
each. `doctor graph` after: 2481 nodes, 2417 reachable, 64 stuck, 12 things no place gives.

## 6. Every giver found (2026-10-05)

What `--grep` found for the rest, and how the graph takes it:

- **NPC blueprints' trades and conversations**: the placed NPC does not repeat its blueprint's
  component templates (the Jova drunkard's `TradeGiveItemRune`: the liquor for Colonel Vaas's office
  key). The survey reads them from the class's package; each trade is a node of its own (at the NPC,
  needing the item, giving the reward).
- **Quest listeners** (`Gameplay/QuestListeners/*`): their logic is blueprint code, but the tags they
  set are their defaults' `Tag_…` (a boss killed, a photo taken). Recorded as `script_tags`; a node
  marked scripted ("the story going on"): a chain ending there holds its goal back with nothing to
  go to.
- **Fights**: an outcome no place gives (`…EncounterKilled`, `…Encounter.Completed`, a boss) is a
  "win the fight" node where it is needed.
- **Items no place gives** (the hero's necklace): a "the story going on" node.
- Readings fixed: an unset tag reads `None` (no need); an `InteractableStateCondition` with state 0,
  or none written (the default is not serialised), is "not used yet" (Jeljin's dial locks, usable
  while the gazebo panel is shut — read as used, they made a cycle); an actor the survey has not (a
  level instance's part) holds nothing back.

`doctor graph`: **2598 nodes, all 2598 reachable from nothing known; none stuck, nothing needed that
nothing gives.**

## 7. Datapad entries and the quests' facts (2026-10-05)

A payload gives Datapad entries whole (`BaseIdentity`: an identity — a person, a place, a thing —
with its base facts). The survey records a payload's `identities` and writes every identity's base
facts and related items (`identities.json`, 92); the graph gives a node its identities' facts.

`doctor graph` reports the facts: 755 of the Datapad's 887 are given by some node. Of the main
quests' own facts (`QuestNN` in the name) 37 of 42: Quest03 and Quest06 all, the rest missing only
the journal's descriptions and an NPC's tie to the quest — set by the quests' scripts. The 132 not
given are entries' details (Info/Desc, 74), names, locations and quest ties: cutscenes (level
sequences), the drone's talks and the quests' scripts give them, and a few are decoys never given
(`…_Dummy01_Name`).

## 8. Next

1. What gives the 15: the quests' steps (StoryUnits), encounters (a boss killed), loot tables, scenes.
2. The guide on the graph: the auto target's chain resolved against the save (facts, tags, items
   held, save GUIDs used); the first node of it guided to, the chain in the cards and the trace.
3. Quest order: each quest's steps as nodes, so the main story and the side quests read as one
   graph from the start to the end, checked against a walkthrough.
