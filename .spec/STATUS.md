# Status (2026-10-03)

## In one paragraph

hiumod attaches to Steam build **24045435** of *Hell Is Us*, where `doctor` passes every check. It is
a panel (cheats and tools), overlays (minimap, big map, compass, quest tracker) and a CLI, all from
outside the game. Twelve cheats are verified in play. The guide reads the game's own data: the quest
journal, goals from what the save knows, routes on the game's navmesh, every puzzle and vault answer,
enemy groups left, collectibles and achievements. Everything follows the game's language (12). The
repository is ready to be published; nothing has been pushed yet.

## Repository

- Local only. It will be published as `myso-kr/hell-is-us-mod` (public, the user's decision,
  2026-10-03). Commit authors are unchanged; local paths were rewritten out of the history (D21).
- CI (`.github/workflows/ci.yml`: fmt, clippy `-D warnings`, tests, a no-game-files check, the site
  check) and Release (`release.yml`: tag → Windows zip → release notes) are written but have never run.
- GitHub Pages: `tools/site/config.json` points at `https://myso-kr.github.io/hell-is-us-mod/`; the
  source will be `main` / `/docs`.

## Verified in play (build 24045435)

| Area | What was seen |
|---|---|
| Attaching | `doctor` all ok: name pool +0x9220CC0, GEngine +0x947CF10, FField offset +0x44, the chain, the gate, position, 8 attribute sets with 69 attributes (ANCHORS.md) |
| Cheats | `god`, `stamina`, `speed`, `hero_time`, `game_speed`, `enemy_time`, `frail`, `stock`, `shards`, `weapon_xp`, position slots, `ghost` (as a way past enemies) — CHEATS.md |
| Minimap | Position and rotation, markers, the trail, SVG icons, used items and corpses hidden, F9 cycling |
| Guide | Quest journal updating live when a quest completes; enemy-group counts falling as groups are cleared; vault dial numbering matching the symbol numbers; vault symbols drawn as SVG |
| Panel | Every page captured after the spacing and theme work (PANEL.md §4); sidebar status, including "Game data · Ready" with the build stamp written |
| Game data | Automatic reading on this machine (data present → stamped, ready). The runtime-install path was tested piecewise: the winget package id resolves, and the dotnet-install fallback installs a working runtime in 11 s (71 MB) |

## Not yet confirmed

- `lymbic`, `lymbic_cost`, `skill_cooldown` — not tried (Lymbic powers were not unlocked in the test
  save; the last two are coefficients and are expected to fail, D12).
- The panel's [Install .NET 8] button end to end on a machine without the runtime.
- Whether the game's inventory (Datapad) and gamepad menus raise the "game menu open" signal that hides
  the overlays (MAP.md §7).
- Toggles resuming after the gate closes and reopens around a load.
- The trail per region after travelling to another region; overlay positions under DPI scaling.
- CI and Release on GitHub (no remote yet).

## Known limits

- **Steam achievements are not blocked.** Achievements earned while cheating reach Steam.
- Exclusive fullscreen hides the panel; play windowed or borderless.
- Damage, defence, healing, cooldown and XP coefficients cannot be changed from outside (D12) — they
  need GameplayEffects applied inside the game (ROADMAP).
- Routes are 2D on the navmesh and the obstacle grid; two-storey interiors, ramps and stairs can still
  confuse the grid fallback (D15).

## What the tool leaves on the player's machine

- Nothing inside the game's own folders. On first run it creates
  `...\steamapps\common\Hell Is Us\Mods\` (or `%LOCALAPPDATA%\hiumod\` if that is not writable):
  settings, `hiumod.log`, `originals.txt`, `minimap.txt`, `talked.txt`, `survey\` and `locale\` (the
  game data read on this machine), `backups\` (save copies), `doctor\`, and `dotnet\` only if the
  fallback runtime install was used.
- No other mods (UE4SS etc.) were installed.
