# Runbook

How to build, run, check and release. Every command runs from the repository root unless it says
otherwise.

## Build and test

```
cargo fmt                                  # rustfmt.toml: 120 columns
cargo clippy --all-targets -- -D warnings  # the same bar as CI
cargo test                                 # no game needed; tests that read a real install skip without one
cargo build --release                      # target\release\hiumod.exe
```

## Replacing a running panel

While `target\release\hiumod.exe` runs, the release link fails. Never kill the process: closing it
another way skips putting cheats back. Either:

- build elsewhere — `cargo build --release --target-dir target/next` — and ask the user to restart
  from it; or
- close the panel with its own × (which restores originals), copy the new build over, start it again.
  With "keep these cheats on next time" set, it switches them back on once the hero can be controlled.

If cheats are on, ask the user first: losing god mode mid-fight is not acceptable. Read-only checks
can use the debug build meanwhile (`target\debug\hiumod.exe doctor`).

## Checking the panel

Layout bugs show up only on screen. Open each sidebar page and capture the window, then look at
every capture. PANEL.md §4 lists what was fixed this way.

- **Never move the user's cursor or take the focus**: the player may be busy in another window. Press
  controls through **UI Automation** by name (the panel builds with eframe's `accesskit` feature, so
  every button is in the tree): `AutomationElement.FromHandle(hwnd)`, `FindFirst(Descendants, Name =
  "퀘스트")`, then `InvokePattern.Invoke()`. Posted `WM_LBUTTONDOWN`/`UP` messages are not reliable:
  winit tracks the mouse on the first move and Windows answers with a leave at once while the real
  cursor is elsewhere.
- Capture with `PrintWindow(hwnd, dc, PW_RENDERFULLCONTENT)`, the window's own rendering, not
  `CopyFromScreen`: windows over the panel do not show and the panel need not come to the front.
- Close the panel with its own × (restores the original values), and start it with `Start-Process`,
  never as a shell's background task (a time limit there kills it with cheats on).

## Checking the game (all read-only)

```
hiumod doctor          # install, build, anchors, layout, chain, gate, position, attributes, cheat table
hiumod list            # every attribute set and attribute on the hero, with current values
hiumod get Endurance EnduranceCap
hiumod pose            # position and heading every 0.5 s (Ctrl+C to stop)
hiumod doctor survey   # the guide's data from the game files (the panel does this itself)
```

- In the main menu, `doctor` gets as far as the anchors (name pool, GEngine, layout) and stops at
  `player:`. That is expected; load a save and run it again.
- Every attach scans the image's writable data. Right after the game starts, GEngine may not exist yet
  ("still starting up?").
- DOCTOR.md has the probes (`inspect`, `find`, `dump`, `watch`, `scan`, `saves`).

## Adding a cheat

1. Find the attribute's exact name with `hiumod list` (or a field with the doctor probes).
2. Add a constant and a row to `src/cheat/cheats.rs` (`any("Name")`, or `attr("SetName", "Name")` when
   the name is in more than one set; targets other than the hero go in `src/cheat/extras.rs`). Start with
   `verified: false`, and add its label key to every `assets/i18n/*.tsv`.
3. `cargo test` — the table tests catch duplicate ids, bad ranges and missing restores.
4. `doctor`'s `cheat table:` line must say ok. Switch it on in the panel, watch the debug page, and let
   the user mark it ✓; then set `verified: true` and update CHEATS.md.

## After a game update

The tool holds no per-build numbers for the engine chain; after an update, checking is the job.

1. Start the game, load a save, run `hiumod doctor`.
2. By the failing line:
   - `name pool` / `GEngine` — a discovery condition broke. Check `names::discover` and
     `anchors::find_engine` (the message says whether 0 or several candidates were found).
   - `FField layout` — the engine moved. Add a candidate to `names::LAYOUTS`.
   - `no property X` — a property was renamed. Fix the name in `player.rs`.
   - `cheat table: not in the game` / `in N sets` — fix the row in `cheats.rs`.
3. Native offsets used by a few cheats (stack counts, weapon XP — ANCHORS.md) need checking with
   `doctor watch`.
4. The panel re-reads the guide's game data by itself when the Steam build changes (SURVEY.md §8).
5. Add the build and the result to ANCHORS.md and `game::TESTED_BUILD`.

## The website and the video

```
python tools/site/build.py               # docs/ from tools/site (CI fails if docs/ differs)
cd tools/video && npm install && npm run render       # docs/assets/media: WebM, MP4, poster
node render.mjs --stills 2,5,11,20       # keyframes to check before a full render
```

Edit `tools/site`, never `docs/` directly. SITE.md has the details.

## Releasing

1. In CHANGELOG.md, rename the newest heading `Unreleased — X.Y.Z` to `X.Y.Z`; set the same version in
   Cargo.toml.
2. `git tag vX.Y.Z && git push origin vX.Y.Z`.
3. `release.yml` checks the three agree, builds the zip with `tools/package.ps1` on Windows, checks
   that nothing from the game is in it, and publishes it with the verified build, install steps, the
   changelog section and a SHA-256.

`powershell tools/package.ps1` builds the same zip locally (`dist\hiumod-<version>.zip`).

## Committing

- English messages; the subject at most 72 characters, details in the body. End with the
  Co-Authored-By line the session specifies.
- Never commit the game's files (.exe/.dll/.pak/.utoc/.ucas/.uasset/.sav), `Mods/`, the survey or locale
  output, extracted textures, or the AES key. `.gitignore` and the CI hygiene job both guard this.
- No local paths or user names in committed files; write `%USERPROFILE%\…` instead.
