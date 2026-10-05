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

## 8. The order, and what it is not (2026-10-05)

`reach` runs in rounds (what a round makes doable counts from the next), so each node has a depth
from the start; `doctor graph` prints, per world, the round its world-map entry is first given, and
per main quest the rounds of its facts. Measured: depths reach 7 at most (Marastan's entry after
Talju's departing truck, Auriga's after Jeljin's key scroll), and every main quest's first facts
are at round 0. That is the graph's local "this before that", not the story's order: the story
moves on by the quests' scripts and the conversations' topics, which are blueprint logic, not data.

Tried and dropped: gating a world on its world-map entry (`WMA_<world>_Travel_Identity_DA`).
The entry is map knowledge given on arriving or talking there (Acasa's by Jova's herbalist, who is
in Acasa), not the way in: as a gate every world waited on itself.

So the guide keeps the game's own journal for the story's order (which quest, which step: read
live) and the graph for what a step needs first.

## 9. Barriers on the way (2026-10-05)

The user: an item has to be fetched to open a puzzle or key door, but the guide points past the door
to what lies behind it, not to the item. Behind-door edges are not in the data (§1), so the route
tells: a route of the auto guide that goes through something (`Path::through`) is matched against the
barriers of the world the graph knows are still shut (`Graph::door_steps`: doors, key locks, Lymbic
locks, panels, gates, placements, keypads, dials), the nearest within 5 m of its first blocked leg
(`barrier_on`). The guide then takes up that barrier's chain's first step (the goal there, or one
made for it), holds it, and traces "the way to X runs through Y: first ...". It lets go when the
barrier opens or the step is no longer first. A step whose own way is blocked is not taken up (else
the two take turns), and while a barrier is seen the closed-spot rule (GUIDE.md) stays off.

A one-sided door (SURVEY.md §10) keeps its `opens_from`: its step goal, and a survey goal at it the
graph steps to, stand there instead of at the door, with "opens from the other side only", so the
route goes round to the side it opens from rather than to the face that cannot open it. Once used, it
is an opened door like any other (ROUTES.md §8). Test: `a_one_sided_door_is_guided_to_from_the_side_it_opens`.

## 10. The auto guide in the graph's order; where a step is worked (2026-10-05)

The user: the auto guide should follow the graph, not go to whatever is nearest. Before, held-back
goals dropped out of the auto guide's choice and their chains' first steps were goals like any other,
so it went to the nearest place any chain led through. Now each held-back goal keeps the goal of its
chain's first step (`Goal::first`, set by `gate` and `flood`), and the auto guide takes the followed
quest's objectives nearest first, each by what comes first for it: a held-back one by its first step,
an open one itself (`target::next_goal`). A step whose way is blocked gives way to the next objective;
with none, it falls back to the nearest wanted goal as before. Test:
`auto_goes_by_the_graph_not_to_the_nearest_place`.

A step no longer takes over a goal beside it that is held back itself: at the forge, the step "Child"
(a receiver) took the quest's payload 1 m away, which waited on that same receiver, so the guide
pointed at the payload. And a step stands where the hero works it (`Graph::stand`): a one-sided
door's open side; a receiver worked through devices among them, on their floor (devices more than
25 m from their middle: the nearest to it). The user saw no ring at the Forge Foyer's puzzle: its
receiver hangs 4 m over the room's six levers, off the top of the screen. Test:
`a_receiver_is_worked_among_its_devices_on_their_floor`.

## 11. Audit of the whole graph (2026-10-05)

The user: draw the graph and audit it all for what is cut off or in the wrong order. `hiumod doctor
graph` now also writes `Mods\doctor\graph.json` (each node: world, name, class, place, whether it has
a save GUID, its needs as a tree, what it gives, its round); local data, never committed. Over the
2598 nodes and 1061 edges (a need's leaf to what meets it; items and facts to their three earliest
givers):

| Check | Found | Meaning |
|---|---|---|
| Dangling reference (a need names an actor not in the graph) | 1 | a level-instance floor (`AS_DirFloor_01_Cut_PLI`): the need reads as met |
| Untracked link (the actor needed has no save GUID) | 11 edges, 7 actors | always met: the edge holds nothing back (6 in Senedra) |
| Cycles over "used" | 5 (14 nodes) | every one an elevator: its call levers need its state, it needs them |
| Order inverted (needs what comes only in a later round) | 0 | |
| Needed, given by nothing | 0 | |
| "Not known yet" windows | 10 | a fact that must not be known, given somewhere: the window can close |
| No edge in or out, giving nothing | 937 | doors with no condition (257), pickups whose payload is not read (122), activators of receivers in other cells, quick chats |

**The story order is not in the graph.** Of the places that give each main quest's facts, 94–97 %
are doable in round 0 (Quest05: 294 of 307), because nothing the graph knows gates them: the quests'
steps (StoryUnits) that do are not nodes yet. The rounds order places by their puzzles and keys, not
by the story. This is the first thing to add (§12).

## 13. The story order (2026-10-05)

The user: remove the graph's gaps entirely. The audit (§11) showed the story order missing. What the
game's data holds instead of a list of steps:

- **Conversations are gated.** A conversation (`ConvoRoot`, its `ConvoIntro` beside it, `TopicFlowAsset`s
  under it) reaches each payload through condition nodes: `HasFact` / `DoesNotHaveFact` / `HasTagFact` on
  their `Passed`/`Failed` pins, and `ConditionSubGraph`s (`ConditionFlowAsset`, read back from its finish
  through `LogicalAND`/`LogicalOR`). The survey tool now writes, for each payload and sub-graph, what must
  hold on the way to it (`gated`, `gated_subgraphs` in flows.json; up to four ways each). A topic's
  `QuestionSelector` names its subject, not a gate: topics open on `Conversation.TopicsUnlock.<who>.<what>`
  tags, and talking one through sets `<tag>.Spoken`. The graph makes a node per thing said (`Say`), needing
  what holds on its way; before, a person gave all of every branch from the first round.
- **Regions are gated.** The APC goes to a region once `WMA_<world>_Travel_BifrostTransitionFact` is known
  (a base fact of the region's travel identity, given by conversations and notes). Every node of a region
  needs it, where anything gives it; the region the hero stands in always counts as reached (live).
- **Fights give.** A spawner's `GuaranteedDropSpawnerPayload` is given when its enemies are beaten, and it
  wakes on `ActivationRequiredTags` and sleeps on `ActivationBlockedTags` (Marastan's market cleared,
  Auriga's protocol waves). The survey now records spawners with either.
- **Bosses wait for their quest.** A world's boss-fight payload that gives a main quest's facts needs that
  quest begun (Senedra's boss gives Quest03's keystone).
- **Listeners mostly wait.** A quest listener names every tag its blueprint uses; one that something else
  gives, it waits for (Jova's grieving father waits for `Act01Complete`), only the rest are its outcome.
- **Loops that are positions.** A device that needs the state of the receiver it sets off (an elevator's
  call lever) does not need it first: the five elevator loops are gone.
- **What only the code gives** is a node of its own (`CodeGives`), listed by `doctor graph`: two lore
  topics and two of Auriga's secret waves.

Result: 2996 nodes, all reachable, 0 cycles, 6398 edges (1061 before), Datapad facts with a giver
854/887 (755). The order reads Senedra 0 → Acasa 2 → Vyssa 4 → Plains of Mist 5 → Lake Cynon 14 →
Jeljin, Lethe Propaganda, Talju 17 → Lethe Library 18 → Marastan 19 → Auriga 20; Act 1's quests
(Quest01 0–18, Quest02 5–16), Act 2's from 16 (Quest03 16–22, Quest04 16–21, Quest05 16–26). Test:
`the_story_order_comes_from_what_is_said_and_where_one_can_travel`.

Open: a choice slot's items (the Eye of God takes any keystone) are not yet needed (needing any of
them left 1627 nodes stuck; being looked into); Plains of Mist opens early through a letter in Vyssa.

## 14. Next

1. What gives the 15: the quests' steps (StoryUnits), encounters (a boss killed), loot tables, scenes.
2. The guide on the graph: the auto target's chain resolved against the save (facts, tags, items
   held, save GUIDs used); the first node of it guided to, the chain in the cards and the trace.
3. Quest order: each quest's steps as nodes, so the main story and the side quests read as one
   graph from the start to the end, checked against a walkthrough.
