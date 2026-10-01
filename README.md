# Hell Is Us — mod

Single-player cheats, and a minimap to come, for *Hell Is Us* (Steam, AppID 1620730,
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

Double-click `hiumod.exe`, or run it with no arguments. It starts the game through
Steam if the game is not running, attaches once you are in, and closes when the game
exits. **F8** shows and hides the panel. Play in windowed or borderless mode.

```
hiumod doctor          # checks everything, writes nothing — load a save first
hiumod list            # every attribute the hero has, read live
hiumod hold god stamina speed=1200  # until Ctrl+C, then puts things back
hiumod pose            # where the hero stands and faces (the minimap's input)
hiumod restore         # if a hold was killed rather than stopped
```

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
cargo test        # 42 tests, no game needed
cargo build --release
```

---

> **Unofficial fan-made tool.** Not affiliated with or endorsed by Rogue Factor or
> Nacon. Requires a legitimately purchased copy of the game. Steam achievements are
> **not** blocked. Cheats can corrupt your save, so back it up first. Provided as is,
> with no warranty. See [NOTICE](NOTICE).
