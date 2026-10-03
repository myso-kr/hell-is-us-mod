# .spec — documents for picking up the work

This folder is for whoever continues the work, person or AI, starting cold. Read the first five in
order and you can take the next step; the rest are reference, one topic each.

| Order | Document | Answers |
|---|---|---|
| 1 | [STATUS.md](STATUS.md) | Where things stand: what works, what was confirmed in play, what was not |
| 2 | [ROADMAP.md](ROADMAP.md) | What comes next, in what order, and when each item counts as done |
| 3 | [RUNBOOK.md](RUNBOOK.md) | How to build, run, check, release, and respond to a game update |
| 4 | [ARCHITECTURE.md](ARCHITECTURE.md) | How the code is laid out and how it follows the game's memory |
| 5 | [DECISIONS.md](DECISIONS.md) | Why things are the way they are, and what was tried and dropped |

Reference:

| Document | Topic |
|---|---|
| [PLAN.md](PLAN.md) | What the project is: the game, the phases, the shape of the work |
| [ANCHORS.md](ANCHORS.md) | How the tool finds its way in, and what a game update can break |
| [CHEATS.md](CHEATS.md) | Every cheat: what it writes, and whether it was seen working |
| [CHEATS-RESEARCH.md](CHEATS-RESEARCH.md) | Research for more cheats: existing trainers, probes, why coefficients fail, the stages |
| [DOCTOR.md](DOCTOR.md) | The `doctor` probes: inspect, find, dump, watch, scan |
| [GUIDE.md](GUIDE.md) | Compass and goal guidance: what the game knows, how goals are worked out |
| [ROUTES.md](ROUTES.md) | Routes to the goal: obstacles, water and slopes, the game's navmesh |
| [MAP.md](MAP.md) | The map: why it is drawn from the world, and how it is drawn |
| [QUESTS.md](QUESTS.md) | The main story and the quest journal and tracker |
| [JOURNEY.md](JOURNEY.md) | Where players struggle, stage by stage, and the next features proposed for it |
| [FEATURES.md](FEATURES.md) | The guide's features by stage (F1–F11): backups, pins, missables, puzzles, vaults, enemy groups, achievements |
| [SURVEY.md](SURVEY.md) | The game-data survey: the tool, its stages, automatic reading and the .NET 8 runtime |
| [PANEL.md](PANEL.md) | The panel: layout, theme, spacing, console |
| [I18N.md](I18N.md) | Languages: following the game's, its own names, the mod's text tables |
| [SITE.md](SITE.md) | The GitHub Pages site and the introduction video |
| [RESEARCH.md](RESEARCH.md) | Research before starting: the engine, existing mods, tools, minimap examples |

When documents disagree, the code wins, and ANCHORS.md is the reference for what the tool relies on in
the game. `docs/` is the generated website, not documentation.

Sibling projects: `~/dungeons2-mod` (Minecraft Dungeons II — the same structure, the original),
`~/combolands-mod` (MelonLoader), `~/big-dragon-mod` (CDP).

Last updated: 2026-10-04 · Steam build 24045435 · hiumod 0.2.0
