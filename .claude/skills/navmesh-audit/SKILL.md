---
name: navmesh-audit
description: Dumps the game's loaded walking navmesh with hiumod doctor nav (Mods\doctor\navtiles.bin), rebuilds it offline the way src/read/navmesh.rs does, and explains why two points are or are not connected, naming the doors at each gap from graph.json. Use when the guide says a goal is blocked or "through something", a route goes the long way round, or the user asks whether a place can be walked to.
---

# Navmesh audit

Background: `.spec/ROUTES.md` §6–9. "Blocked" in the guide is exactly navmesh connectivity. The
navmesh is the game's baked one, with **every door closed**; live, the guide joins it through doors
it judges passable (`Graph::passable`, `NavMesh::bridged`): used, or opened by a mere press, and
one-sided doors only from the side they open (`opens_from`).

## Steps

1. Dump the tiles loaded around the hero (game running, a save loaded, hero near the place; reads
   memory only):
   ```
   target\release\hiumod.exe doctor nav
   ```
   Writes `<install>\Mods\doctor\navtiles.bin` (u32 LE length + raw Detour tile, each). Only the
   streamed-in chunks are there: dump again after moving to another area.
2. Rebuild and ask (positions are Unreal cm at the feet; take them from the guide trace or
   `hiumod pose`):
   ```
   python .claude/skills/navmesh-audit/scripts/navmesh_audit.py stats
   python .claude/skills/navmesh-audit/scripts/navmesh_audit.py where X Y Z
   python .claude/skills/navmesh-audit/scripts/navmesh_audit.py why AX AY AZ BX BY BZ
   python .claude/skills/navmesh-audit/scripts/navmesh_audit.py why --trace
   ```
   `why --trace` takes the hero and the auto target from the last `guide.jsonl` state. Output: each
   point's poly and piece, the gaps (<= 2 m, `--gap`) out of A's piece with the nearest door-like
   node of `graph.json`, and the fewest-gaps chain from A to B.
3. Read it:
   - Same piece: the mesh is not the cause; look at the guide's rules (guide-trace skill).
   - A chain of 1.3–1.5 m gaps at doors: the way runs through those doors. Each is passable live
     only if used or press-only; otherwise it is the barrier the graph's step should lead to
     (GRAPH.md §9). Check the door's needs with the graph-audit skill's `why_not.py`.
   - No chain: a lift, a drop, water, or tiles not loaded (dump nearer B, or raise `--margin`).
   - A point "off the navmesh": the guide reaches a goal from a floor within 3 m across, 8 m below
     or 1.5 m above (`floor_under`); farther, it reads as through something.
   - Check the dump's time against the trace's: a dump from another area or session misleads.

`navtiles.bin` is game data: never copy it into the repository.
