---
name: guide-trace
description: Reads the hiumod guide trace (Mods\doctor\guide.jsonl in the Hell Is Us install) and summarises the last auto-target changes with their reasons, ping-pong between two goals, the last state's auto target and the nearest goals. Use when the user says the guide points the wrong way, keeps switching targets, says "blocked", ignores a goal, or asks what the guide is doing where the hero stands.
---

# Guide trace

The overlay appends to `<install>\Mods\doctor\guide.jsonl` while the panel runs with the guide on
(`src/ui/overlay/trace.rs`; the Debug page's "Guide trace" card shows the same). One JSON record a line:

- `kind: "event"`: `t`, `what` (`auto` = target change, `guide` = a note with `text`), `from`, `to`
  (label plus `[live]` or `[survey]`), `why`, `hero` [x,y,z], `world`.
- `kind: "state"` (every 5 s): `hero`, `world`, `story` (followed quest), `auto_guide`, `auto`
  (label, id, at, tier, gate `Open`/`Conditional`, source, distance_m, height_m, blocked, held,
  detail = the chain "first: …", route {points, ends_short_m, through_something}), `nearest[]`
  (12 goals with blocked/followed/wanted/skipped), `goals`, `survey_goals`, `blocked`, `tracks`.

Positions are Unreal cm, north is -Y. Labels and details are game text in the player's language
(often Korean): quote them, never commit them.

## Steps

1. Summarise (the game dir is found through Steam; `--game` or `HIU_GAME_DIR` overrides):
   ```
   python .claude/skills/guide-trace/scripts/guide_trace.py --events 15 --nearest 10
   python .claude/skills/guide-trace/scripts/guide_trace.py --grep "<label part>" --events 40
   ```
2. Read it against the rules in `.spec/GUIDE.md`, `.spec/GRAPH.md` §3, §9, §10 and `.spec/ROUTES.md` §7–9:
   - `why: the old one is blocked` = its navmesh route goes through something (no walkable link).
     Check connectivity with the navmesh-audit skill before blaming the graph.
   - `gate: Conditional` + `detail: 먼저/first: A → B → …` = held back by the requirement graph; the
     guide should be at A. If A looks wrong, trace it with the graph-audit skill (`why_not.py`).
   - A ping-pong report (A ↔ B with "wanted more" one way and "blocked" the other) means two rules
     disagree; name both rules and the code that applies them (`src/guide/target.rs`).
   - A trace older than the build being tested (compare `t` and the panel's start) says nothing about
     the fix: ask for a fresh run at the spot.
3. Report the hero's world and position, the auto target and why, and the next thing to check.

Reply to the user in Korean; keep code and docs in English.
