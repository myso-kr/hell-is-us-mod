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

## 4. Next

1. What gives the 15: the quests' steps (StoryUnits), encounters (a boss killed), loot tables, scenes.
2. The guide on the graph: the auto target's chain resolved against the save (facts, tags, items
   held, save GUIDs used); the first node of it guided to, the chain in the cards and the trace.
3. Quest order: each quest's steps as nodes, so the main story and the side quests read as one
   graph from the start to the end, checked against a walkthrough.
