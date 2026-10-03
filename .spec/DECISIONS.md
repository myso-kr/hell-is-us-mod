# Decisions

Each entry: what was decided, why, and what was given up. Numbers never change; a later decision that
replaces an earlier one says so.

## D1. Phase 0 runs as an external process, reusing dungeons2-mod

- Decision: no DLL injection — `ReadProcessMemory` / `WriteProcessMemory`, with code copied from
  dungeons2-mod and adapted.
- Why: both are UE5 + GAS games, so the name pool, attribute resolution, holds and the panel carry
  over. The user approved the research's recommendation (external panel → minimap overlay → UE4SS only
  if needed). With no anti-cheat, injection would also work, but the external route touches no game
  file and cannot clash with UE4SS's dwmapi.dll proxy.
- Given up: starting with UE4SS Lua. An in-game UI is nicer, but the cheat verification tooling (the
  debug page, restoring originals) would have to be rebuilt. Deferred to ROADMAP.
- combolands-mod (MelonLoader/Unity) and big-dragon-mod (CDP/web) are different engines and do not
  apply.

## D2. No per-build anchor table

- Decision: drop dungeons2-mod's `anchors.rs` table (version → GEngine RVA and chain offsets). Find
  the FNamePool and GEngine on every attach and learn the chain by reflected names.
- Why: Steam updates often (the game had just been patched during research and the Cheat Engine table
  was already stale). Nobody should have to paste `doctor` output for each build. Finding things by name
  also lowers the risk of writing to the wrong object: when a name does not match, the tool refuses.
- Cost: scanning `.data` (about 6 MiB) on attach. If a search is not unique, the tool does not run.
- Unknown builds are not refused; instead every step has to pass its name and class checks.

## D3. A hero gate instead of a solo gate

- Decision: this game is single-player only, so the gate is "the controlled pawn is a
  `CharlieCharacterHero`", not a player count. While the gate is closed, toggles **pause**; they are
  not switched off and restored as in dungeons2-mod.
- Why: the gate closes for every load, menu and cinematic. Switching off each time would make the user
  switch everything back on. The point is to guarantee that writes land on the hero — never on an
  enemy's or NPC's ASC.
- Restoring (`restore`, closing the panel) also happens only while the gate is open.

## D4. Attributes by name; `*` for the set

- Decision: as dungeons2-mod D4, attributes are found by name. When the set's name is not known, `*`
  means "the one set that has this name"; more than one is refused, with a request to name the set.
- Why: the set classes are known from the executable (ARCHITECTURE.md), but which attribute is in
  which set is only known with the game running. Uniqueness keeps it safe, and `doctor` shows what
  resolved.

## D5–D9. Carried over from dungeons2-mod

Cheats live in one table (D5); originals go to disk first (D6); remembered settings follow what the
user switched on (D8); the panel is its own window with a polled hotkey (D9). The reasons are in
`~/dungeons2-mod/.spec/DECISIONS.md`.

## D10. Data lives in the install root's `Mods\`

- Decision: `...\steamapps\common\Hell Is Us\Mods\` — beside `HellIsUs\`, not inside it.
- Why: the same place as dungeons2-mod D7. Steam's integrity check only looks at shipped files.
- Open: if UE4SS or pak mods are ever used too, names could collide — whether to move to
  `Mods\hiumod\` is a question for the user, together with dungeons2-mod.

## D11. The first minimap had no map image (replaced by D16)

- Decision (2026-10-02): the first minimap drew only position, heading, the trail and user markers.
- Why: the map textures are inside AES-encrypted paks, and extracted assets cannot be redistributed. A
  trail and markers already cover most of "not getting lost in a game without a map".
- Later: the game's world map turned out to be a country map, unusable as a background (MAP.md §3–4),
  so the map is drawn from the world itself (D16).

## D12. Coefficient attributes that do nothing are replaced by plain fields

- Decision: when a GAS attribute write holds its value but has no effect, write the plain UPROPERTY
  whose value the game actually uses (the movement component's `MaxWalkSpeed`, the actor's
  `CustomTimeDilation`). `Session::add_fields` checks the owning object's class first (hero /
  CharacterMovementComponent), takes 4-byte floats only and finite values only, and records originals
  through the same path as attributes.
- Why: five coefficient attributes failed in the first play test. GAS recomputes attributes that carry
  modifiers through an FAggregator with its own BaseValue, and damage calculations use values captured
  when a GameplayEffect executes — writing the attribute set's memory reaches neither. Only values the
  game reads directly every time, such as Endurance, had an effect.
- Given up: writing the aggregator's BaseValue. `ActiveGameplayEffects.AttributeAggregatorMap` is not
  a UPROPERTY, so it cannot be found by reflection, and following a TMap at fixed offsets from outside
  is fragile. To revisit in-game (applying GameplayEffects), see ROADMAP.
- `hero_time` replaced `attack_speed` and `dodge_speed`: it speeds up the hero alone (attacks, dodges,
  movement) and leaves enemies as they are. Verified in play.

## D13. The overlays are layered Win32 windows, not eframe

- Decision: the minimap and other overlays are `WS_EX_LAYERED | WS_EX_TRANSPARENT` windows on their
  own thread, drawn with a small rasteriser of our own (`map/raster.rs`) and shown with
  `UpdateLayeredWindow`.
- Why: eframe stops running frames while its root window (the panel) is hidden (dungeons2-mod D9), and
  the minimap has to keep drawing while the panel is hidden. Layered windows take neither focus nor
  clicks, so the game keeps the mouse. Software drawing is enough for a 240 px disc.
- Given up: a second eframe viewport (the reason above); a DX12-hook overlay (needs injection, D1).

## D14. Map icons are SVG files rendered with resvg

- Decision: one SVG per kind in `assets/icons/*.svg`, built into the binary (`include_str!`) and
  rasterised once at start-up with resvg; drawing blits premultiplied bitmaps.
- Why: the user's suggestion. Kinds read at a glance where dots do not, and changing a shape means
  changing a file. resvg is pure Rust, so no build tools are added (default features off — no text or
  raster-image support needed).
- Cost: more dependencies (usvg, tiny-skia, …). If rasterising fails, dots are drawn as before.

## D15. Water and slopes from outside the game (no UE injection)

- Decision (2026-10-02, approved by the user): routes learn about water from memory — the deadly-water
  trigger boxes plus the Chaos terrain heightfields (ROUTES.md §3) — rather than from physics queries
  made inside the game (UE4SS/DLL, LineTrace).
- Why: keeps D1 (no game files touched; a crash in the tool cannot take the game with it). Finding the
  heightfields on the CPU side solved slopes as well.
- To revisit: if routes are often wrong in two-storey buildings, stairs or ramps, keep the panel as it
  is and add a small in-game module that only answers physics queries.

## D16. The map is drawn from the world (option B)

- Decision (2026-10-02, the user's choice of B in MAP.md §4): the map background is the loaded static
  meshes seen from above, later joined by terrain shading and contours from the heightfields
  (MAP.md §5–8).
- Why: the game's only map images are the country map used in the APC, which does not cover the
  explorable areas at a usable scale.

## D17. The panel's layout is CSS Flexbox/Grid (egui_taffy), wrapped in Tailwind names

- Decision (2026-10-03): `ui/tw.rs` wraps egui_taffy with the vocabulary of Tailwind (`col`, `row`,
  `wrap`, `grow`, `card`, `masonry`). One palette and one spacing scale live in `ui/theme.rs`.
- Why: hand-rolled egui columns and a 12-column grid kept overlapping or clipping (PANEL.md §7). A real
  layout engine measures each element instead of guessing widths.

## D18. The guide's game data is read automatically; the .NET 8 runtime is installed on request

- Decision (2026-10-03): once the hero is in control, the panel runs `doctor survey` / `doctor locale`
  in the background when the data is missing or was made on another Steam build. When the .NET 8
  runtime is missing, the panel offers to install it: winget first, then Microsoft's dotnet-install
  script into `Mods\dotnet` (SURVEY.md §8).
- Why: players should not have to type a command for the guide to work.
- Given up: shipping the survey output (it would put data extracted from the game into the
  repository); shipping the survey tool self-contained (+31 MB per download); installing without
  asking (it puts software on the player's machine).

## D19. The website is generated, static, and one page per language

- Decision (2026-10-03): `tools/site/build.py` renders `docs/` from one template and a string table per
  language — 12 static pages with hreflang, JSON-LD and a sitemap. The three.js hero is decoration on
  top of complete HTML (SITE.md).
- Why: search and answer engines read static text; the 3D scene is not needed to read the page.
  Generation keeps 12 languages in step; CI fails when `docs/` drifts from `tools/site`.

## D20. The introduction video is rendered deterministically; no GIF

- Decision (2026-10-03): the video is one HTML scene where every pixel is a function of `t`, captured
  frame by frame and encoded with ffmpeg (SITE.md). WebM (1.7 MB) and MP4 (4.0 MB) only.
- Why: re-rendering after a change gives the same video. The GIF version was 16.2 MB and was dropped at
  the user's request; GitHub's README cannot play a video file from the repository anyway, so the README
  shows the poster linked to the WebM.

## D21. Public repository: history without local paths; authors unchanged

- Decision (2026-10-03): before the repository goes public, the whole history was checked for secrets,
  game files and text, and personal data. The only finding — a local path with the Windows user name in
  two documents — was rewritten out of every commit. Commit authors stay as they are, matching the
  sibling repositories (the user's call). Commit messages are in English with subjects of 72 characters
  or less.
- Why: everything in a public history is permanent once pushed; rewriting is only free before the first
  push.
