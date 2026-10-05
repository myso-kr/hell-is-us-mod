---
name: survey-asset
description: Looks things up in Hell Is Us's cooked game data with hiumod's survey tool (tools/survey, CUE4Parse) - list package paths by regex, dump a package's exports as JSON, grep packages for names, peek at a placed actor and its sub-objects in a world, find exports that refer to a name, or run a world or full survey. Use when you need to know where an item, fact, tag, actor, condition or giver lives in the game files, or what a blueprint/data asset holds.
---

# Survey the game's data

Background: `.spec/SURVEY.md` (§2 sources, §3.1 the tool, §7 how it reads actors), `.spec/GRAPH.md`
§1, §5, §6 (how givers were found with `--grep`). The tool's modes are in `tools/survey/Program.cs`.

## Running it

The wrapper fills in `--game` (found through Steam), checks the built tool and the mappings, and
passes every other argument through:

```
python .claude/skills/survey-asset/scripts/survey.py <mode> [--save out.json]
```

Equivalent raw form: `dotnet tools/survey/bin/Release/net8.0/survey.dll --game "<steam library>\steamapps\common\Hell Is Us" <mode>`.
Needs the .NET 8 runtime, the built tool (`dotnet build -c Release` in `tools/survey`) and
`<install>\Mods\doctor\HellIsUs.usmap` (`hiumod doctor usmap`, game running). Each run finds the AES
key in the game's exe; the key is never printed or saved: keep it that way.

| Mode | What it gives | Cost |
|---|---|---|
| `--ls <regex>` | mounted paths matching (case-insensitive), e.g. `--ls "Gameplay/QuestListeners/.*Jova"` | seconds |
| `--dump <package path>` | the package's exports as JSON (`HellIsUs/Content/...uasset` or `.umap`) | seconds |
| `--grep "a\|b" [--in <prefix>]` | `name<TAB>package` for each package whose bytes hold a name (default prefix `HellIsUs/Content/`); narrow `--in`, e.g. `HellIsUs/Content/Gameplay/` | minutes over all content |
| `--peek <actor name part> --world <W>` | every export whose path contains it (the actor, its components, its conditions) with merged properties | ~a minute per world |
| `--refs <name> --world <W>` | exports whose property values mention it (who points at an actor) | ~a minute per world |
| `--world <W> --out <dir>` | a survey of one world (`<W>.json`, `flows.json`, tables) | a minute or two |
| `--world all` (or no mode) | the full survey | several minutes: run it in the background |
| `--tables` / `--locale` | spawner/vault/recipe tables to `Mods\survey`; every culture's text to `Mods\locale` | seconds / a minute |

Worlds: AcasaMarshes, AurigaMuseum, Jeljin, LakeCynon, LetheLibrary, LethePropaganda, Marastan,
PlainsOfMist, SenedraForest, Talju, VyssaHills. Without `--world`, `--peek`/`--refs` walk all 11.

## Rules

- A survey without `--out` overwrites `<install>\Mods\survey`, the panel's live data. For experiments
  write to a scratch folder (`--out "$env:TEMP\survey"`); rerun into `Mods\survey` only when the code
  that reads it changed (or use `hiumod doctor survey [world]`, which also writes the mappings and
  the locale).
- Long output: `--save` to a file under `%TEMP%`, then search it with Grep instead of reading it whole.
- Everything the tool prints or writes is game data (names, text, positions): never commit it, never
  paste game text into code or docs. Cite package paths and class/property names as evidence.
- Cooked actor names end in `_UAID_…_<number>`; the trailing number does not exist at runtime
  (SURVEY.md §7, S5), so strip it before matching live objects.
