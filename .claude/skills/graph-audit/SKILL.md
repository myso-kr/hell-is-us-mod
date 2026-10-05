---
name: graph-audit
description: Regenerates and audits hiumod's requirement graph (hiumod doctor graph -> Mods\doctor\graph.txt and graph.json) for dangling references, untracked links, cycles over "used", order inversions, needs given by nothing, not-yet-known windows, isolated and unreachable nodes, and traces why a node, fact or item is reached late or never. Use after changing src/guide/graph.rs or the survey, or when the user asks why the guide wants something first, why a place is "stuck", or to audit the graph.
---

# Requirement graph audit

Background: `.spec/GRAPH.md` (§11 is the audit, §13 the story order). The graph is built from the
survey (`Mods\survey`) by `src/guide/graph.rs`; `doctor graph` reads only the survey, not the game,
so it runs without the game.

## Steps

1. Regenerate (from the repository root; use `target\next\release\hiumod.exe` if that build is newer):
   ```
   target\release\hiumod.exe doctor graph
   ```
   It prints the summary (nodes, doable, stuck, per-world counts, things no place gives, code-only
   givers, the world/quest order, Datapad fact coverage) and writes `<install>\Mods\doctor\graph.txt`
   and `graph.json`. Read `graph.txt` first: "stuck" and "needed, given by no place" should be 0.
2. Audit `graph.json`:
   ```
   python .claude/skills/graph-audit/scripts/graph_audit.py --show 5
   python .claude/skills/graph-audit/scripts/graph_audit.py --world LakeCynon --json "$env:TEMP\audit.json"
   ```
   Baseline (GRAPH.md §11, §13): dangling 1 (`AS_DirFloor_01_Cut_PLI`, reads as met), untracked 11,
   cycles 0, order 0, no_giver 0, unreachable 0. A rise in any of these after a change is a
   regression to explain; windows (must-not-know facts something gives) and isolated nodes are
   expected in the hundreds, so look at what changed, not the total.
3. Trace one thing:
   ```
   python .claude/skills/graph-audit/scripts/why_not.py Quest.Facts.LymbicForgeWaterLevel2
   python .claude/skills/graph-audit/scripts/why_not.py ForgeMachineLever --world LakeCynon --depth 3
   ```
   `!` marks the need that holds last (or never): that chain sets the round. "given by NOTHING"
   points at a script, scene or code giver (GRAPH.md §6, §13 `CodeGives`).
4. To find where an unknown giver lives in the game's data, use the survey-asset skill (`--grep`).

## Rules

- `graph.json`, `graph.txt` and any findings file are game-derived: keep them under `Mods\doctor`
  or `%TEMP%`, never in the repository.
- Record a new audit result in `.spec/GRAPH.md` and the CHANGELOG's Unreleased section when the
  numbers change because of code.
- Node and fact names in replies are fine; quote game text (labels in Korean etc.) only to the user.
