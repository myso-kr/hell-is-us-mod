# Architecture

How the code is laid out, and how it finds its way through the game's memory.

## The game

- *Hell Is Us* (Rogue Factor / Nacon), Steam app **1620730**. The executable is
  `HellIsUs\Binaries\Win64\HellIsUs-Win64-Shipping.exe` (155 MiB). It is not encrypted and reads fine
  from disk. There is no Denuvo and no anti-cheat.
- Unreal Engine **5.5.4** (PCGamingWiki; consistent with MegaLights in the executable). The version
  resource only says `UE5-CL-0`, so the exact engine build string is not in the binary.
- The internal codename is **Charlie**: `CharliePlayerController`, `CharlieCharacterHero`,
  `CharlieHeroAbilitySystemComponent`, `CharlieGameInstance`, `CharlieCheatManager`, …
- Stats are GAS attributes in native sets (their UTF-16 registration names are in the executable):
  `EnduranceAttributeSet`, `LymbicAttributeSet`, `DamageAttributeSet`, `WeaponAttributeSet`,
  `LocomotionAttributeSet`, `PoiseAttributeSet`, `PlayerDefenseAttributeSet`, `PlayerAttributeSet`,
  `StanceAttributeSet`, `HealthAttributeSet`, `CharlieAttributeSet`, …
- **Health is the ceiling on stamina.** A hit lowers `EnduranceCap`, and `Endurance` moves underneath
  it. The Cheat Engine table's "health" is `EnduranceCap`.
- The player blueprint is `StoryHero_BP_C`. The Cheat Engine table puts the ASC at +0x688 and
  `SpawnedAttributes` at ASC +0x1088. Those numbers are a cross-check only; the code finds both by
  reflection.
- Paks are IoStore with utoc flags Compressed | Encrypted | Signed | Indexed. No AES key has been
  published; the survey tool finds it in the executable at run time (SURVEY.md, MAP.md §1).

## Following memory to the hero

```
scan the image's writable data
  ├─ FNamePool: the one pool whose block 0 starts "None", "ByteProperty"      (names::discover)
  └─ GEngine:   the one global pointing at a non-default object whose class
                derives from GameEngine                                       (anchors::find_engine)
FField layout: of the 4 candidates in LAYOUTS, the one under which
               GameEngine.GameInstance leads to a GameInstance

GEngine ─GameInstance→ ─LocalPlayers[0]→ ─PlayerController→ ─Pawn→ hero
   hero ─(the property pointing at an AbilitySystemComponent)→ ASC ─SpawnedAttributes→ TArray<set>
   hero ─RootComponent→ ─RelativeLocation (f64×3, cm)
   controller ─ControlRotation (f64×3: pitch, yaw, roll)
```

`player::learn` resolves names to offsets once, the first time a hero exists after attaching
(`Attached.chain`). After that every tick follows pointers by those offsets.

Engine layout constants (`src/unreal/names.rs`): UObject `ClassPrivate` +0x10, `NamePrivate` +0x18,
`OuterPrivate` +0x20; UStruct `SuperStruct` +0x40, `ChildProperties` +0x50. The FField members
(Next, Name, ElementSize, Offset_Internal) differ between engine versions and are chosen from
candidates: UE 5.6 (measured in dungeons2-mod) uses 0x18/0x20/0x34/0x48; **this game (UE 5.5) uses
0x18/0x20/0x34/0x44** (measured 2026-10-02).

An FName index is `(block << 16) | (offset in block / 2)`. An entry is a u16 header (bit 0 = wide,
top 10 bits = length) followed by the characters. The block array sits at pool +0x10.

Other paths the guide uses:

- World name: pawn → `OuterPrivate` (level) → `OuterPrivate` (world).
- Loaded levels: `World.Levels` (+0x178, reflected). A level's actors: `ULevel::Actors` (+0xA0, not
  reflected — found as the one TArray in the hero's level that contains the hero). Build 24045435:
  140 levels, 10,671 actors.
- GUObjectArray, the save objects and the quest data: GUIDE.md §2. Navmesh tiles: ROUTES.md §6.
  Terrain heightfields: ROUTES.md §3.

Coordinates: UE X is forward, Y right, Z up; yaw runs from +X towards +Y (clockwise seen from above).
**The game's north is world −Y (yaw 270)**, checked against the game's own compass
(`MapState.north_yaw`). East is +X.

## Safety checks before any write, in order

1. The hero gate: the controlled pawn's class derives from `CharlieCharacterHero`
   (`player::Chain::hero`).
2. Attributes are found by set class name plus property name (`*` = the one set that has that name),
   and must be declared 16 bytes.
3. The target's first 8 bytes are the shared attribute vtable — at least 4/5 of all attributes must
   agree, and it must lie inside the game image. Plain fields instead check the owning object's class
   and that the reflected size is 4.
4. Finite floats only.
5. The original value is written to disk before overwriting; if that write fails, nothing is written
   (`cheat/hold.rs`).

## Code map

Folders are **layers**. A lower layer never knows about a higher one:
unreal → read → guide/cheat → map → engine → ui. Every module is re-exported at the top of `lib.rs`,
so callers name it by module only (`crate::goals`, `hiumod::mem`) and a move between folders does
not touch them.

```
src/
  main.rs cli.rs        the CLI (doctor/list/get/set/hold/restore/pose/ui) and argument parsing
  engine/               mod.rs: the engine loop (attach, gate, holds; derived lists once a second, shared by Arc)
                        attached.rs: the attached game, its queries and caches (implements extras' Reach)
                        snapshot.rs: the only thing the panel and the overlay read
  unreal/               game memory and Unreal reflection
    mem.rs              the Memory trait, pointer chains, a Fake for tests
    names.rs anchors.rs FNamePool and GEngine discovery; class names, ancestry, properties
    gobjects.rs         GUObjectArray
    player.rs           learning the chain by reflection, the hero gate, position and heading
    usmap.rs probe.rs   writing .usmap mappings; the doctor sub-commands (DOCTOR.md)
  read/                 the world, read through reflection
    actors.rs           map actor classification (6 kinds, 23 sorts)
    attr.rs             attribute sets: read and write (Session)
    knowledge.rs        known facts, tags, investigations from the save
    puzzles.rs          dials, keypads and item placements near the hero (FEATURES.md §3)
    terrain.rs obstacles.rs navmesh.rs geometry.rs   heightfields, obstacle hulls, the game navmesh, static-mesh outlines
  cheat/                cheats.rs (the table) · hold.rs (originals and restore) · extras.rs (targets other than the hero; reaches the game only through Reach)
  guide/                where to go, and why
    goals.rs            guidance goals (payloads, NPCs, quest items) and the gate on them
    quests.rs           the quest journal and secrets (good deeds, mysteries, timeloops), read on a time budget
    survey.rs tables.rs the survey (Mods\survey\*.json) and its tables: spawners, vaults
    missables.rs        good deeds that can be missed (assets/missables.tsv)
    ledger.rs           what is left in each region, by kind; the trip planner's order (FEATURES.md §6)
    pathfind.rs         grid A* round obstacles, when there is no navmesh (ROUTES.md)
    target.rs           choosing the target: auto, skip, blocked → what opens it; the cycle key
  map/                  map state and drawing
    minimap.rs          MapState (trail, settings, layers), minimap.txt; re-exports pins and view
    pins.rs view.rs     map pins (24 PinKinds, Marker) · display modes, terrain modes, projection (View)
    canvas.rs           premultiplied-alpha pixel buffer, shapes, vector font
    compass.rs          the compass strip and floor cues (fading, up/down arrows)
    raster.rs           draw_map (one frame); re-exports canvas and compass
    relief.rs icons.rs  terrain baking; rasterising assets/{icons,pins}/*.svg
    symbols.rs          the vault symbols (assets/symbols), recoloured
  i18n/                 following the game's language (I18N.md): culture.rs (TextCulture from the profile) ·
                        names.rs (the game's own names, Mods\locale) · text.rs (the mod's text tables, assets/i18n) ·
                        fill.rs (runtime fill for trf!) — tr!("KEY") / trf! macros
  infra/                log.rs logfile.rs (hiumod.log) · paths.rs (the Mods\ folder; every file path starts here) ·
                        settings.rs · verify.rs · backup.rs (save backups) · memstat.rs (this process's memory) · prof.rs (where a thread's time goes) · session.rs (where the last session left off) ·
                        gamedata.rs runtime.rs (reading the survey automatically; the .NET 8 runtime — SURVEY.md §8)
  game/                 the game seen from outside: locate.rs (install), launch.rs, process.rs (RPM/WPM),
                        achievements.rs (Steam's achievement cache, FEATURES.md §4)
  ui/                   the panel and the overlays (Windows only)
    mod.rs              Request, Shared, the worker thread, the backup and memory-log thread
    panel/              the eframe panel: mod.rs (frame, header, sidebar, console window) and one file per page:
                        groups · now · map · guide · quests · collect · deep · saves · debug
    overlay/            the overlay thread: mod.rs (frame loop, windows, keys) · route.rs (the route to the goal,
                        navmesh first then grid, own thread) · bake.rs (terrain baking) · hud.rs (pins, compass, tracker lines)
    tracker.rs pen.rs   the quest tracker; GDI text
    console.rs hotkey.rs layered.rs   the drop-down console; hotkeys and window order; layered windows
    tw.rs theme.rs svg.rs             card layout (PANEL.md §1) · palette and spacing (§3–4) · SVG to egui textures
assets/                 icons/ (23 sorts) · pins/ (24) · symbols/ (8 vault symbols) · i18n/ (the mod's text, 12 languages) ·
                        missables.tsv — data that does not belong in code
tests/data_files.rs     integration tests over the data file formats
tools/survey/           the C# + CUE4Parse survey tool (its output is never committed)
tools/site/             the GitHub Pages generator (SITE.md) · tools/video/  the introduction video (SITE.md)
docs/                   generated site — edit tools/site, not this
examples/               throwaway probes against the running game (gitignored)
```

Rules:

- A new module goes in its layer's folder, with one line in that folder's `mod.rs` and one in
  `lib.rs`'s `pub use`.
- File paths start from `paths::data_dir()` only. Data and icons live in `assets/` files.
- The UI never reads game memory; it reads the Snapshot. Guidance rules go in `guide/`, drawing in
  `map/`.
- When a file grows past ~700 lines, split it by concern (as panel → `panel/`, raster →
  canvas/compass).

Threads: **only the worker thread touches game memory.** The overlay reads snapshots too.
`Attached` holds `RefCell`s and is not `Sync`; it never leaves the worker.

## Test fixtures

`anchors::tests::image()` builds a PE header, a writable section, a name pool, GEngine and a
GameInstance in fake memory. `player::tests::world()` adds a local player, controller, hero, ASC and
position on top. Classes and properties come from `names::fixture::Pool` (`class`, `inherit`).

## Conventions for documents, examples and tests

- All documentation lives in `.spec/`, in English. `docs/` is only the generated website.
- `examples/` holds throwaway probes against the running game (gitignored). Anything worth keeping
  becomes a `doctor` sub-command (`doctor saves`, `doctor locale`).
- Unit tests sit in each file under `#[cfg(test)]`. Integration tests that exercise a file format
  through the public API go in `tests/` (`data_files.rs`).

## Memory

Measured 2026-10-03 after a long-running panel (an earlier build) showed 1.3 GB:

- Re-measured, the panel sat at 230–275 MB, swinging ±20 MB every 20 s — repeated allocation rather
  than a leak.
- The engine alone, headless at the same 10 Hz for 150 s: flat around 50 MB. Not the cause.
- The overlay's heavy work alone, headless on real data — 300 big-map frames on a 1700² canvas (54 ms
  each), 20 terrain bakes, navmesh routes (2 ms) and grid routes (288 ms): peak 112 MB. Not the cause.
- The panel's usual ~250 MB is egui and OpenGL (the panel and the console viewport), fonts (Malgun
  Gothic 13 MB, Segoe UI Symbol) and the layered windows' DIBs.

The 1.3 GB spike never reproduced, so the tool now records what would catch it: `memstat.rs` logs the
working set, private bytes and peak every minute (`memory: working … · private … · peak …`) and shows
them on the debug page. One cost was cut regardless: the lists derived from the survey (needs,
hand-overs, deadlines, collections, stories, the journal) were rebuilt every engine step (10 Hz); they
are now rebuilt once a second and shared with the snapshot by `Arc`, so the panel and the overlay copy
a pointer per frame instead of the lists.

## Performance (2026-10-06)

Each thread logs where its time goes every 30 s (`prof::span` / `prof::report`): `worker time`,
`overlay time`, `panel time` in `Mods\hiumod.log`. The user saw lag whenever the panel or a tab
opened; measured in play, before and after:

| | before | after |
|---|---|---|
| worker step, mean / worst | 46.8 / 2179 ms | 21.5 / 117 ms |
| obstacles · goals · hold · survey · things | 11.4 · 29.2 · 7.5 · 6.4 · 6.3 ms | 3.9 · 12.7 · 2.9 · 3.4 · 4.2 ms |
| panel frames, idle | ~58 a second | ~12 a second |
| 3D map scene drawn | every frame | when the camera or a layer changes |

The panel:

- Repaint requests of different periods do not line up, so their rates add: the 3D map asked
  every 33 ms, the backdrop every 66 ms, each snapshot (~9 a second) at once, and the panel drew
  ~58 frames a second, each drawing the 3D scene twice (the X-ray pass). The 3D map now asks only
  while its camera glides after the hero; a snapshot asks for a frame within `SNAPSHOT_FRAME`
  (250 ms), so snapshots coming faster share one; the backdrop drifts at 10 frames a second.
- The 3D map draws its scene into a frame buffer of its own (`map3d::Cached`: colour and
  depth-stencil renderbuffers the view's size) only when what its pixels depend on changes
  (`Frame::key`: camera, layers, round, route, picked place); every other frame copies it into the
  window (`glBlitFramebuffer`, cut by the scissor). The icons and controls are egui's, over it.

The worker (analysed span by span; the step's 100 ms period overran):

- `obstacles`: after the 3 s rest, a pass starts only when the hero has gone 60 m from where the
  last started, or 12 s after it ended; a single mesh out of reach is left before its mesh is read.
- `Names::find` keeps what it found by (class, class name, field): every `field`, dozens a step,
  walked the lineage and cloned each class's properties.
- The graph's answers that depend on the knowledge alone (`known`, logic puzzles, passable and
  one-way doors) are kept until the world or the knowledge's reading (every 2 s) changes; the
  pools are shared, not cloned.
- `Survey::goals` counts the names the survey has twice once per call (`shared_names`), not once
  per entry (it was O(n²) over the world's entries, twice).
- `Scanner::positions` reads what does not walk about (all but enemies and people) every 10th step
  and keeps its last place between.

### Phase 2 (2026-10-06)

Measured with reads per span (phase 1, `prof::count_read`), same 30 s window in play:

| | phase 1 | phase 2 |
|---|---|---|
| hold | 5.5 ms, 4,393 reads a step | 4.4 ms, 2,753 reads |
| survey (graph gate, flood, doors) | 3.9 ms every step | 1.4 ms; gated 31 of 275 steps |
| reads a step | 13,538 | 11,747 |
| panel waiting on the map lock | up to 96 ms | not while the overlay draws |

- `Attached::session` keeps the attribute layout (`attr::Layout`) by (set array data, count,
  hero) for up to 5 s; every write still checks the attribute's vtable.
- Frail enemies: a record holds while its attribute's vtable and its set's class read the same
  (two reads); it is found again (the ability system walked) only when they do not.
- The graph's gating, flooding and door steps are kept by `gated_key` (world, knowledge reading,
  steps, scene, each goal's id and metre).
- The overlay draws the minimap and the big map from `MapState::for_drawing` (this world's trail
  and pins only) with the lock let go.
- `Layered::present` skips a present whose pixels, place and fade are those of the last one.

### Phase 4a: the object census (2026-10-06)

The obstacle and quest passes each read every live object's class (about 500,000) themselves.
`gobjects::Census` keeps (object, serial, class) per GUObjectArray slot: a reading reads the slot
table a chunk at a time and the class only of slots whose object or serial changed, at most once a
second (`CENSUS_FOR`); both passes take its (object, class) pairs in address order. Measured, 30 s
in play: obstacles 5.3 → 3.0 ms (1,506 → 921 reads a step), quests 4.1 → 0.8 ms (1,454 → 125),
the step 29.5 → 17.8 ms. The first reading, at start, reads every class once (about 0.8 s on the
worker's thread).

### Phase 4b: one walk of the levels' actors (2026-10-06)

The scanner (every second) and the goals (every 2 s) each walked every loaded level's actors and
read each one's class. The scanner now keeps each level's (actor, class) pairs while its actor
array's data and count are unchanged (for at most `LEVEL_FOR`, 5 s: an actor replaced in place
shows within it), and the goals take the scanner's pairs (`Scanner::all_actors`). Measured, 30 s
in play: things 4,186 → 2,079 reads a step, goals.refresh 1,503 → 37, the step's reads 10,011 →
6,501 (13,538 at phase 1).

### Phase 5 (partly, 2026-10-06)

- The game view's layer (`screenroute::paint`) sleeps to 5 ms before a frame is due, then waits for
  the composition (`DwmFlush`) and reads the camera right after: a 16 ms timer alone beats against
  the display's refresh. No composition: the timer alone.
- Exclusive fullscreen is asked of the shell once a second (`SHQueryUserNotificationState`,
  `QUNS_RUNNING_D3D_FULL_SCREEN`, as Discord does); the Now page then says to switch the game to
  borderless. Borderless and windowed games are not reported.
- Not done: presenting through DirectComposition (a swap chain on a `WS_EX_NOREDIRECTIONBITMAP`
  window) instead of UpdateLayeredWindow. It needs the COM interfaces of the `windows` crate, a new
  dependency; the measured gain is the big map's present (4.6 ms a frame at 30 fps).
- Done since: the big map and the game view's layer present through DirectComposition
  (`ui/composed.rs`: a premultiplied flip-model swap chain on a `WS_EX_NOREDIRECTIONBITMAP`
  window, `Layered::new_composed`, falling back to UpdateLayeredWindow). Offscreen, a present
  costs about the same either way (1440² 0.93 vs 0.97 ms, 2560×1440 1.97 vs 1.79 ms); confirmed
  in play to show and pass clicks as before.
