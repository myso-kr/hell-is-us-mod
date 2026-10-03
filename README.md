# Hell Is Us — mod

Single-player cheats and a minimap overlay for *Hell Is Us* (Steam, AppID 1620730,
`HellIsUs-Win64-Shipping.exe`, Unreal Engine 5.5). They are applied from outside the
game through its memory. **No game files are modified.**

> **Status: attaches to Steam build `24045435`.** `doctor` passes every check there:
> anchors, reflection chain, hero gate, position, and all 8 attribute sets.
> Four cheats are verified in play: `god`, `stamina`, `speed` and `hero_time`.

| Document | What it settles |
|---|---|
| [CHANGELOG.md](CHANGELOG.md) | what each release changed, and the game build it was verified on |
| [docs/PLAN.md](docs/PLAN.md) | what this is, the phases, and what each one attaches to |
| [docs/ANCHORS.md](docs/ANCHORS.md) | how the tool finds its way in, and what a game update can break |
| [docs/CHEATS.md](docs/CHEATS.md) | the cheats, what each writes, and what "verified" means |
| [.spec/](.spec/README.md) | handoff documents for whoever continues the work (Korean): status, next steps, procedures, structure, decisions, research |

## Quick start

1. Unzip the release next to nothing in particular — it writes only to
   `<game folder>\Mods\`.
2. Double-click `hiumod.exe`. It starts the game through Steam if it is not running,
   attaches once you are in, and closes when the game exits. Play windowed or
   borderless.
3. Press **`** (the key left of 1) to show or hide the panel. Its header button opens
   a console for the commands below.
4. Once, for the guide's full data: run `doctor survey` in that console (needs the
   .NET 8 runtime; about two minutes). It reads the game's maps and text into
   `Mods\survey` and `Mods\locale` — never into the repository or the game.

The mod speaks the game's language: it follows the game's text setting (12
languages), and item, NPC and region names come from the game's own translations.

## What it does

- **Cheats** (panel): health, stamina, Lymbic energy, speed, time scale, frail
  enemies, consumables, weapon XP, positions — written only while you control the
  hero, and put back when switched off.
- **Minimap and big map** (F9 cycles mini → big → off): walls and floors from the
  level's geometry, shaded terrain and contours, floors above and below faded,
  your trail, 26 kinds of icons (SVGs in `assets/icons/`) and 24 kinds of pins you
  place (F6). Each kind can be hidden.
- **Compass and guide** (F10, F11 for the next goal): the game has no quest markers,
  so the guide works the goals out from what the save says you know and what each
  place in the world still holds. It follows your chosen quest, routes along the
  game's own navmesh and shows the distance and height difference.
- **Quest tracker**: main quests, good deeds, mysteries and timeloops with real names,
  what the followed quest still needs here and elsewhere, and missable deadlines.
- **Collect**: collectibles per region, enemy groups left for the every-Hollow
  achievement, the six Vaults of Forbidden Knowledge with their codes, NPCs with
  more to tell, and your Steam achievements with progress.
- **Puzzles**: every dial, keypad and item placement in the game with its answer —
  hidden until you press *Show answer*.
- **Saves**: each game save is backed up to `Mods\backups` (last 20).

Every overlay hides while a game menu is open. The keys can be changed in the panel.

```
hiumod doctor          # checks everything, writes nothing — load a save first
hiumod doctor survey   # the guide's data from the game files (once per game update)
hiumod hold god stamina speed=1200  # until Ctrl+C, then puts things back
hiumod restore         # if a hold was killed rather than stopped
hiumod help            # every command
```

See `.spec/GUIDE.md` (Korean) for how each part reads the game.

## How it finds its way in

The tool stores no offsets. Each time it attaches, it does the following:

1. It scans the game image's writable data for the engine's name pool and for
   `GEngine`. Each must turn up exactly once.
2. It reads the engine's own reflection to learn the path to the player:
   `GameInstance → LocalPlayers[0] → PlayerController → Pawn`, then the pawn's
   `AbilitySystemComponent → SpawnedAttributes`.
3. It finds each attribute by the game's own name, such as `EnduranceCap` or
   `LymbicEnergy`.

So a Steam patch only breaks something when it renames something. When that happens,
`doctor` says which name went missing.

## Safety rules the code keeps

- **The hero gate.** Nothing is written unless the controlled pawn is the hero, a
  `CharlieCharacterHero`. Loading screens, menus and cinematics close the gate.
  While it is closed, held cheats pause; they are not dropped.
- **Every write lands on an attribute or nowhere.** An attribute must be declared
  16 bytes (an `FGameplayAttributeData`). It must also start with the vtable that
  all attributes share, inside the game image.
- **Originals go to disk first.** `originals.txt` is written before anything is
  overwritten. Switching off, closing the panel or running `restore` puts the
  originals back.

## Building

```
cargo test                       # unit and integration tests, no game needed
cargo build --release
powershell tools/package.ps1     # dist\hiumod-<version>.zip: the exe, the survey tool, the documents
```

The survey tool (`tools/survey`, C# with CUE4Parse) needs the .NET 8 SDK to build.

---

> **Unofficial fan-made tool.** Not affiliated with or endorsed by Rogue Factor or
> Nacon. Requires a legitimately purchased copy of the game. Steam achievements are
> **not** blocked. Cheats can corrupt your save, so back it up first. Provided as is,
> with no warranty. See [NOTICE](NOTICE).
