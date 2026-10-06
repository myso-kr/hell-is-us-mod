# Roadmap

What comes next, in priority order. An item is done when its **done when** holds. After any game
update, do RUNBOOK.md "After a game update" before anything else.

Finished work is recorded where it belongs — STATUS.md for the current state, CHANGELOG.md for
releases, the topic documents for how it works — not here.

## 1. Publish

- Create `myso-kr/hell-is-us-mod` as a public repository and push `main` (only `main`).
- Turn on GitHub Pages: source `main`, folder `/docs`.
- Watch the first CI run: fmt, clippy, tests, the no-game-files check and the site check.
- Optionally upload the introduction video once through GitHub's web editor so the README can play it
  inline (SITE.md, "Video").
- **Done when:** CI is green, the site answers at `https://myso-kr.github.io/hell-is-us-mod/` in all 12
  languages, and the README renders with its badges.

## 2. Releases

- 0.1.0 (2026-10-03), 0.2.0, 0.2.1 and 0.3.0 (2026-10-04), 0.4.0 (2026-10-05) and 0.5.0 (2026-10-06) are released: each by renaming the CHANGELOG heading
  `Unreleased — X.Y.Z` to `X.Y.Z`, setting Cargo.toml, then `git tag vX.Y.Z` and pushing the tag.
  `release.yml` checks the tag, Cargo.toml and the changelog agree, builds the zip, checks it holds
  nothing from the game, and publishes it with the verified build and a SHA-256. Versions follow
  semver: new features in a minor version while below 1.0, fixes alone in a patch.
- **Done when:** the release has the zip and its checksum, and the zip, unpacked on a clean machine,
  starts, attaches and reads the game data (including the runtime prompt if .NET 8 is missing).

## 3. Close the open checks

From STATUS.md "Not yet confirmed": the three untried cheats (`lymbic`, `lymbic_cost`,
`skill_cooldown` — once Lymbic powers unlock), the [Install .NET 8] button on a machine without the
runtime, the menu-open signal for the inventory and gamepad menus, toggles resuming after a load, the
trail across regions, and DPI scaling.

- **Done when:** each is confirmed in play, or fixed, or recorded as a limit with its reason.

## 4. The next features

JOURNEY.md proposes nine features from research into where players struggle, in order: area ledger,
point-of-no-return check, "previously", the Lymbic key-item chain, trip planner, completion board,
clue board, shard budget, Haze links. Its §4 lists the research some of them need first.

- **Done when:** each built one is confirmed in play and documented in FEATURES.md.

### 4.1 Filming mode (proposed by the user, 2026-10-06)

**Done (2026-10-06)**, confirmed in play: `cheat/film.rs`, CHEATS.md "Filming mode". The camera's
own flight and the combat camera's settings followed (CHEATS.md "Flight").

The hero and the camera driven along a route for recording: the route from the navmesh (the
guide's) or from points placed on the 3D map, smoothed; the hero moved along it at 60 Hz with the
teleport write (root location and ComponentToWorld, velocity), the camera turned with the
controller's `ControlRotation` (along the way, or at a point). The ghost and god cheats keep a fight
from breaking the shot; streamer mode hides the overlays from the recording.

- **Probed in play (2026-10-06):**
  - Position written at 60 Hz (root `RelativeLocation` and `ComponentToWorld`) with `Velocity`:
    449 of 450 cm in 3 s, the hero within 0.7 cm (mean) of the last write before the next; the
    game does not push back, the height follows the floor, and the walk animation plays. The body
    keeps its own facing unless turned too (`RelativeRotation` yaw and the transform's quaternion):
    without that it looked like walking backwards.
  - `Velocity` alone: 68 of 225 cm — the movement component brakes it with no input.
  - **`Pawn.ControlInputVector` (the stick's input) written at 250 Hz: the game walks the hero
    itself** — 911 cm in 2 s exactly the way asked (with the speed cheat at 720), the body turned
    to face it by `bOrientRotationToMovement`. The user found it the most natural: collisions,
    stairs and slopes are the game's. Chosen for filming mode; the input's length (0–1) sets the
    pace, and the way is corrected every tick toward the route's next point.
  - `ControlRotation` turns the camera: 90° over 2 s followed about 6° behind (the rig's own
    smoothing), FOV 70 read from the camera manager.
- **Built:** a "filming" card in the cheats group: route from the guide or the 3D map's points,
  speed, look ahead / at a point, loop, start and stop keys, routes kept by name.
- Input simulation (keys and mouse) is not the way: it takes the cursor and focus, and is not exact.

## 5. Routes indoors

Routes are 2D. The navmesh handles most places; the obstacle-grid fallback can be wrong in two-storey
interiors, on ramps and on stairs (ROUTES.md, D15).

- First: record where routes go wrong in play (position, target, what the route did).
- If it is common, D15's fallback: a small in-game module that only answers physics queries, with the
  panel unchanged.

## 6. Phase 1 — inside the game (large; needs its own design)

- Candidate: RE-UE4SS (Nexus #43, with a UE 5.5 preset). Lua could revive `CharlieCheatManager`, apply
  GameplayEffects for the cheats that cannot work from outside (damage, defence, healing, i-frames,
  cooldowns, XP — CHEATS.md, D12), and block achievements via
  `CharlieAchievementsUnlockerSubsystem`.
- Read D1 again before deciding: the external design's main benefit is touching no game file.
- **Done when:** there is a design document with a decision recorded in DECISIONS.md.

## 7. Housekeeping

- D10's open question: whether `Mods\` should become `Mods\hiumod\` if other mod loaders are used.
- Keep `.spec/` current: a feature is not done until its document says how it works.
