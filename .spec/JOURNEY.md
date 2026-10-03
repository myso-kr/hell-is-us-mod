# Player journey and the next features

Research into where players of *Hell Is Us* struggle, stage by stage, and the features that would help
without taking the game's design away from them. Researched 2026-10-03. Built so far: §3.1, §3.3 (rods
→ locks), §3.4, §3.5 and §3.8 — FEATURES.md §6; the rest is still a proposal.

The rule every proposal keeps: **answers stay hidden until asked.** The game's identity is "no map, no
markers"; the mod is for the night a player would rather not be lost, not a replacement for exploring.

## 1. What players run into

From reviews and guides (sources at the end), sorted by how often the same pain comes up:

| Pain | What players say | Covered today |
|---|---|---|
| Getting lost, not knowing what was missed | walking in circles in the first areas, "never sure if I was missing something"; hours before finding an item | minimap, guide goals, needs per quest |
| Backtracking with no fast travel | the only fast travel is back to the APC; a key found later means a long trek to the rift or puzzle it opens | — |
| The Datapad buries the clue you need | "a sprawling menu … millions of shreds of paper with cryptic clues … overwhelming to find the one you need" | journal names, needs list (partly) |
| Item hand-overs | "Who does this go to? Was it that person asking for ID cards?" | hand-overs (F2) |
| Side activities untracked | good deeds and mysteries are "great fun" but hard to remember where their NPCs and objectives are | journal, missables (F1) |
| Missable achievements | 7 missable: NPCs die or move after story events (*Man of the People*, *Antiquarian*, *Lend an Ear*, *Ever After*), and closing timeloops ends respawns so shards become finite (*Good Vibrations*, *Accessorizing*, *To the Teeth*) | deadlines for good deeds (F1); nothing for the shard budget |
| The Lymbic chain | chests and doors need Lymbic rods by emotion and letter; chests give Amine prisms; prisms (Theta, …) close specific timeloops — a chain across regions | puzzle list shows each lock's solution; no chain |
| Combat repetition | five Hollow Walker types in three tiers; Hazes keep their linked Walkers invulnerable until killed | enemy groups left (F8) |
| Coming back after a break | the design leans on memory; after a pause the notes no longer make sense | — |

Completion in numbers (PowerPyx): 25–50 h, one playthrough, 45 research items, 29 relics, 26 good
deeds, 46 mysteries, 14 timeloops, 8 drone module types, 25 glyphs, 10 caps, 6 vaults. No point of no
return in Act 3 for the non-missable collectibles.

## 2. The journey, and where a feature fits

| Stage | The player's question | Proposal |
|---|---|---|
| First hours (Senedra, Acasa) | "Did I miss something here?" | §3.1 Area ledger |
| The investigation loop | "Which clue was that? Who wanted this?" | §3.2 Clue board |
| Exploring with new items | "What does this rod/key open, and where?" | §3.3 Key-item chain |
| Planning a trip | "If I take the APC to Vyssa, what can I finish there?" | §3.4 Trip planner |
| Before a story beat | "Is there anything I lose if I go on?" | §3.5 Point-of-no-return check |
| Closing timeloops | "Will I still have enough shards?" | §3.6 Shard budget |
| Fights | "Why won't this one die?" | §3.7 Haze links |
| Returning after a break | "Where was I, and what was I doing?" | §3.8 Previously |
| The last 10 % | "What is left for 100 %?" | §3.9 Completion board |

## 3. Proposals

Each: what it shows, the data it needs and whether we have it, and the effort.

### 3.1 Area ledger
- Shows, for the region the hero is in: things still left by kind (quest goals, items, puzzles, lymbic
  locks, timeloop, enemy groups, NPCs with more to tell), as counts only — "Acasa: 3 puzzles, 1 lock,
  2 NPCs". Opening a line reveals the places. A tracker line can show the total.
- Data: all present (survey per world, save element states, needs, F4/F8/F10).
- Effort: small — a new card and tracker line over existing lists.

### 3.2 Clue board
- The Datapad's clues regrouped by what they are for: per quest and good deed, the facts known, the
  items held for it, and the hand-over or place each one leads to. A text search over the known facts
  and items in the game's language ("ID card", "Vaas").
- Data: known facts and tags (knowledge), fact texts in `Mods\locale`, quest links (QUESTS.md). The
  link from a fact to the quest it serves exists for main quests; good deeds go by tag prefix.
- Effort: medium — a panel page; the search needs the locale text of known facts only.

### 3.3 Key-item chain (Lymbic rods, keys, prisms)
- When an item enters the inventory, list what it now opens: "Lymbic Rod — Rage X: opens the lock
  in Jeljin (needs Victor too — not held; found at …)". And the reverse from a lock: rods held / missing
  and where each missing one lies.
- Data: the survey already has **24 rod-locked panels** with their rods (`LymbicLockPanel_*`,
  solution = rods by emotion and NATO letter), **40 rod pickups** and **27 Amine prism sources**;
  held items come from the inventory. Missing: what each chest panel *gives* (the chest's payload sits
  on a separate actor) and which prism signature each timeloop needs — both need a survey pass (§4).
- Effort: medium; small for rods → locks alone.

### 3.4 Trip planner
- Groups everything open by region and by what the hero can do there now (holding the item, the NPC
  reachable), so one APC trip clears several: "Vyssa Hills — 2 hand-overs, 1 lock you can open, 1
  mystery lead".
- Data: §3.1 per region, plus §3.3's "can do now".
- Effort: small once §3.1 and §3.3 exist.

### 3.5 Point-of-no-return check
- When the followed main quest reaches a step that ends something (an NPC dies or moves, Pathem Abbey
  in Act 2), the tracker shows "before you go on" with the missable deeds and NPC talks still open.
  Extends F1, which only lists deadlines.
- Data: `assets/missables.tsv` already names the deadlines; the trigger is the main quest's state,
  which the journal reads live.
- Effort: small.

### 3.6 Shard budget
- Before the last timeloops are closed: shards held vs the shards still needed for the upgrade
  achievements (*Good Vibrations*, *Accessorizing*, *To the Teeth*), and how many timeloops are still
  open (enemies keep respawning while they are).
- Data: shard stacks (the `shards` cheat reads them), timeloops open (journal), upgrade levels of
  weapons and gear (weapon level is known for `weapon_xp`; gear upgrades need a probe), achievement
  progress (Steam cache, FEATURES.md §4).
- Effort: medium — the upgrade cost table needs to come from the game's data.

### 3.7 Haze links
- On the minimap, a thin line from each Haze to the Hollow Walkers it keeps alive, so the player sees
  which to kill first. No combat automation.
- Data: needs a probe — which component on a Hollow holds its link to a Haze. Enemy actors are already
  classified (actors.rs).
- Effort: medium, research first.

### 3.8 Previously
- On the first attach of a session: where the hero was, the followed quest and its next goal, and the
  facts learned in the last session. "Previously in Hadea" card, dismissable.
- Data: keep a small snapshot at session end (`Mods\session.txt`: world, position, followed quest,
  known facts count and the last N facts); diff against the save on the next start.
- Effort: small.

### 3.9 Completion board
- The achievement categories as one board with counts and the regions where each remaining item is:
  research 45, relics 29, deeds 26, mysteries 46, timeloops 14, drone modules 8, glyphs 25, caps 10,
  vaults 6. Missables marked.
- Data: collections (F4) by item folder, journal counts, achievements (Steam cache), vaults (F7). Glyph
  and cap folders need checking in the survey's item paths.
- Effort: small to medium.

## 4. Research needed first

- **Chest contents:** where a Lymbic chest's reward sits relative to its lock panel (same actor
  hierarchy, attached child, or a separate payload actor nearby) — survey pass over one region.
- **Timeloop prism signature:** which Amine prism (T01/T02/T03 = Greek signatures) each timeloop
  rift needs — look for the rift actor's placement or condition in the survey's flows and placements.
- **Haze → Hollow link** (§3.7): a `doctor inspect` on a linked Hollow during a fight.
- **Gear upgrade costs** (§3.6): the upgrade data tables in the paks.

## 5. Order

| Order | Feature | Value | Effort | Needs research |
|---|---|---|---|---|
| 1 | §3.1 Area ledger | high — the most common pain | small | no |
| 2 | §3.5 Point-of-no-return check | high — avoids a lost achievement | small | no |
| 3 | §3.8 Previously | medium — the returning player | small | no |
| 4 | §3.3 Key-item chain (rods → locks) | high — the chain players look up most | medium | partly (§4) |
| 5 | §3.4 Trip planner | high once 1 and 4 exist | small | no |
| 6 | §3.9 Completion board | medium — the completionist's last stretch | small–medium | glyph/cap paths |
| 7 | §3.2 Clue board | medium — the Datapad's worst pain | medium | no |
| 8 | §3.6 Shard budget | medium — three missable achievements | medium | upgrade costs |
| 9 | §3.7 Haze links | low–medium | medium | yes |

Every proposal is a card or a tracker line with its answers hidden by default, in the 12 languages
(new text keys in every `assets/i18n` table), and confirmed in play before it counts as done.

## Sources

- [Hell Is Us Review — WellPlayed](https://www.well-played.com.au/hell-is-us-review/) — the Datapad, fast travel only to the hub, side activities
- [Hell is Us Review — GamingBolt](https://gamingbolt.com/hell-is-us-review-i-am-my-own-monster) — backtracking to rifts, five Walker types, Haze links
- [Hell Is Us Review — Gamecritics](https://gamecritics.com/jack-dunn/hell-is-us-review/) — hours to find an item, "who does this go to?"
- [Hell is Us Trophy Guide & Roadmap — PowerPyx](https://www.powerpyx.com/hell-is-us-trophy-guide-roadmap/) — missable trophies, collectible counts, points of no return
- [Amine Prism — Theta — Fextralife](https://hellisus.wiki.fextralife.com/Amine_Prism_-_Theta) — chests that give prisms and the timeloops they close (search summary)
- [Hell is Us beginner tips — KeenGamer](https://www.keengamer.com/articles/guides/10-beginner-tips-to-get-started-in-hell-is-us/) — Healing Pulse, Hazes (search summary)
- The game's own data: `Mods\survey` (rod-locked panels, rod and prism pickups), counted 2026-10-03.
