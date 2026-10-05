# Map — background research and drawing

§1–6: research into whether a background map can be laid under the minimap, and the approach that
was chosen. §7–11: how the map is drawn. Started 2026-10-02, Steam build 24045435.

## 1. Opening the assets

- The pak/IoStore files are AES-encrypted (utoc flags Compressed|Encrypted|Signed|Indexed).
- **The AES key was found in the executable itself**, AESDumpster-style: search `.text` for code that
  writes 32-bit immediates (`C7 4x dd imm32`) to 8 consecutive offsets from the same base register,
  which gives 126 candidates. Decrypting the first block of the encrypted index of
  `pakchunk0-Windows.pak` with each candidate, **exactly one** yields an FString mount point
  (`int32 length` + `../../../`).
- **The key is never written into the repository or the documents.** Re-derive it with the same
  procedure when needed. It is also never sent off the user's PC.
- Tool: retoc v0.1.5 (https://github.com/trumank/retoc; release zip sha256 verified).
  `retoc --aes-key <key> list --path --size <utoc>` lists paths;
  `retoc --aes-key <key> to-legacy --no-shaders --no-script-objects -f <name> <Paks> <out>` extracts
  just the needed assets as .uasset/.uexp.
- Listing sizes: pakchunk0 19,897 / pakchunk1 77,568 / pakchunk2 4,794 chunks.

## 2. Map-related assets

| Asset | Contents |
|---|---|
| `UI/Interaction/APC/WorldMap/APC_WorldMap_CloseBG_Img` | 15360×8640 DXT1, 1 mip (66 MB) |
| `…/APC_WorldMap_MiddleBG_Img` | 7680×4320 DXT1 |
| `…/APC_WorldMap_FarBG_Img`, `…_Holder_Img` | 3840×2160 DXT1 |
| `GameData/StoryUnits/WMA_<region>/…` | Per-region (World Map Area) name, description, portrait, travel data |
| `Editor/Tools/MiniMap/Icons/*` | Only 2 icons from a development minimap tool — no regional map images ship in the release build |
| `UI/HUD/HUD_CompassDial_*_Img` | Per-language compass dial images |

Texture decoding: in a cooked `.uexp`, the block data starts right after `PF_DXT1\0` plus 12 bytes
(FirstMipToSerialize, NumMips, bulk flags). The size is the SizeX/SizeY before the string. BC1 was
decoded with numpy.

## 3. What the map shows

`APC_WorldMap_*BG` is **the whole-country map (Hadea) used to pick a region from inside the armoured
vehicle (APC)**: cities (Trisk, Kastel, Libane, Loblina, Lethe, Dalmask, Valde, Losilus, Yvel,
Pyrean, Golmore), region borders and contour lines. Each explorable region (e.g. Senedra Forest) is a
**separate world** (`SenedraForest_Root_WP`), so its coordinates do not map onto this image, where it
is only a small dot.

So **it is of little use as a minimap background**: it has neither the resolution for finding a way
inside a region nor any correspondence to world coordinates.

## 4. Options — **user decision (2026-10-02): B**

- **A. Country map overlay:** a separate key opens the country map in a large window and marks the
  current region (WMA). Not a minimap background. The image would be extracted on the user's PC into
  `Mods\` (cannot be distributed).
- **B. Build the map from the game world:** draw the top-down outlines of the loaded levels' static
  meshes (buildings, rocks, walls — 3,396 within 80 m in the test region) from their positions and
  bounds as the minimap background. No asset extraction, live, automatic for every region, and it
  reuses the stage 2 actor walk.
- **C. Drop stage 3 and move to Phase 1 (UE4SS).**

## 5. Implementing B (2026-10-02)

- Inputs (confirmed via reflection): 9,696 static mesh actors whose root is a `StaticMeshComponent`
  (`RelativeLocation`, `RelativeRotation` (pitch, **yaw**, roll), `RelativeScale3D`, `StaticMesh`,
  `AttachParent`). The mesh's `UStaticMesh.ExtendedBounds` is at +0x200: 56 bytes, Origin · BoxExtent
  · Radius, as doubles.
- `src/read/geometry.rs`: the mesh box rotated by yaw and scaled gives a top-down rectangle
  (`Footprint`) plus a height range. Attached components (with an `AttachParent`) are in relative
  coordinates and are skipped, as are footprints under 30 cm or over 150 m. Each actor is read once
  and cached; every 3 s only newly loaded actors are added. The first read takes about 290 ms, later
  ones almost nothing.
- Drawing (`raster::draw_map` in `src/map/raster.rs`): only meshes spanning −1.5 m to +2.5 m around
  the hero's feet (capsule centre −90 cm) are drawn, which drops ceilings and other floors. Under
  80 cm tall is floor, otherwise wall. Walls wider than 25 m (cliffs, large rocks) were dropped
  because their boxes are far larger than the real shape and cover walkable ground (§6 reversed this).
- The first attempt overlaid translucent rectangles directly and produced blobs. Now floors and walls
  are each drawn as a union into a mask, laid down once with a fixed opacity, and outlined at the
  mask edge — overlaps no longer accumulate.
- Preview: `examples/preview.rs` (not committed) renders one frame of real game data to an image. In
  the test region (outdoors, z≈1261) ruin walls and building outlines came out cleanly.
- Panel Map tab switch "Wall and floor outlines" (minimap.txt `terrain`).
- **Open at the time:** checking in the game, checking dungeons (underground), frame time where
  there are many meshes.

## 6. First user feedback → height bands and contours (2026-10-02)

- Feedback: good overall, but (1) with a smaller radius, terrain objects that do not fit fully inside
  the radius were not drawn; (2) heights were hard to tell apart — something like contour lines was
  needed.
- (1) Clipping at the circle edge turned out to be correct. The cause was the rule dropping standing
  meshes wider than 25 m: the larger the terrain feature, the less likely it fits inside a small
  radius, and those were exactly the ones being dropped. They are now kept, faded, as a
  "cliffs and rocks" band.
- (2) Bands by height relative to the feet (`raster::band`): deep (< −4 m) · low (−4 to −1.2 m) ·
  level (−1.2 to +0.6 m) · high (+0.6 to +3 m) · cliffs and rocks (standing, wider than 25 m) · wall
  (standing at the hero's height). Each band is a union mask with an outline in its own colour, so a
  line appears wherever height changes — acting as contours. The vertical range widened from −1.5 to
  +2.5 m to −8 to +3 m. Flat things overhead (ceilings) are still excluded.
- A band colour legend sits under "Wall and floor outlines" in the Map tab.
- Checked with the preview: stairs and pits (blue), ledges and platforms (sand), walls (bright lines)
  are distinguishable. Cliff and rock boxes show as wide rectangles because the boxes are large, but
  they are faded and do not hide what lies beneath.

## 7. Big map; hiding when a game menu opens (2026-10-02)

- Default key F3 (rebindable in the panel; if any of the five keys collide, all reset to defaults).
  A square 80% of the game window's height, centred, north up, radius 250 m by default (50–1000).
  Many pixels to draw, so it renders every third frame. The small minimap hides while the big map is
  shown. (§10 later removed the dedicated big map key.)

Hiding when a game menu is open:
- Two signals: (1) the system cursor is visible while the game window has focus (`GetCursorInfo`; it
  is hidden during play); (2) the game is paused — `World.PersistentLevel.WorldSettings.Pauser` is not
  null.
- Either one hides the minimap, compass and big map. The panel's Map tab has a game-menu box with the
  on/off switch and the live signals (setting `hide_in_menus`).
- **Unverified:** whether the inventory (Datapad) actually shows the cursor or pauses, and whether it
  does so when opened with a gamepad. If not, look for another signal such as HUD widget visibility.

## 8. Terrain shading and contour map modes (2026-10-02)

Requested: a toggle for hill-shading or contour rendering. It draws the routing heightmap
(ROUTES.md §3) on the map.

- Setting `relief` = off / shading / contours / both (default both). Map tab "Terrain view". Separate
  from the existing "Wall and floor outlines" (structure height bands).
- `src/map/relief.rs` **bakes** a square around the hero (the larger map radius + 150 m) into a
  world-space grid: cells ≥ 1 m, at most 1024 per side. Per cell: height (bilinear from the source),
  shading (light from the north-west at 45°, relative to flat), water (the ROUTES.md §3 water runs).
  Baking runs on its own thread and repeats when the hero moves 100 m, the scene (obstacle pass)
  changes or the map grows. Measured: 540² cells in 31 ms.
- Drawing (`raster::draw_relief`): map pixel → world is only rotation and scale, so it is affine
  (first pixel plus row/column increments). Height is bilinear per pixel first, and a line is drawn
  **where a pixel's 2 m / 10 m interval differs from its neighbour's**, giving a 1 px contour at any
  zoom. Shading is tinted over ±15 m around the feet: lower is bluer, higher is ochre. Water is blue.
- Cost (600 px map, 280k pixels inside the circle): 20 ms without terrain → contours 39, shading or
  both 54 ms. The 240 px minimap costs a sixth of that. The big map already draws every third frame,
  and it all runs on the overlay thread, independent of the game's frames. (§9 cut these costs.)

## 9. Rendering performance (2026-10-02)

Requested: research performance improvements for rendering. A benchmark on real data (195 things,
15,747 structures, 55 goals, a 67-point route, 1024² terrain) broke down the cost per feature — one
`draw_map` frame, release build, average of 10 runs.

| Size | State | Before (ms) | After (ms) |
|---|---|---|---|
| 240 px (minimap) | Empty map | 3.2 | 0.6 |
| | Wall and floor outlines | 50 | 3.5 |
| | Everything (outlines + shading + contours) | 38 | 6.2 |
| 864 px (big map) | Empty map | 34 | 3.6 |
| | Wall and floor outlines | 190 | 11 |
| | Shading / contours | 90 / 81 | 14 / 16 |
| | Everything | 261 | 27 |

Causes and fixes:
1. **Lines, circles and rings scanned their whole bounding box** — one diagonal 600 px route line
   touched 360k pixels. Now only the x-span each row actually touches (`Canvas::rows` in
   `src/map/canvas.rs`). Rings skip the inner hole.
2. **The background disc was drawn every frame** (an 864 px disc is 580k pixels). Now drawn once per
   size, kept in a `thread_local`, and memcpy'd.
3. **Floating-point blending** → integer source-over (`over`).
4. **Wall and floor outlines**: six map-sized masks per frame (864² × 4 B × 6 ≈ 18 MB) plus 15k
   anti-aliased polygons. Now a single u8 class buffer filled with non-AA scanlines (`fill_convex`);
   on overlap the later band wins; colouring and edges are done once.
5. **Terrain**: per pixel, an inverse world transform (trig), a bilinear sample and a colour
   computation. Now colours are precomputed per texel at bake time (rebaked when foot height changes
   by 3 m), pixel → texel uses row/column increments, and only contour heights are bilinear.
6. **Row-parallel work**: both terrain passes and outline colouring are split into row chunks with
   `std::thread::scope`, one per core (≤ 8).
7. **Loop**: "sleep 50 ms + work" became deadline-based (sleep minus the work time), so drawing time
   no longer adds to the frame interval.

In the same change: big map **opacity** (20–100%), applied as `UpdateLayeredWindow`'s
SourceConstantAlpha, so it costs nothing.

## 10. Outline (Diablo-style) style; a key that cycles display modes (2026-10-02)

Requested: the big map covered too much of the screen — make it like Diablo's overlay map, with a
transparent background and solid outlines only. Also: switch between big map and minimap in the
panel settings, and have the single map key cycle through display modes, as most games do.

- `View.outline` (`src/map/view.rs`): no disc background (fully transparent), no border ring. Lines
  only, no fill:
  - Structure bands: solid edges for **walls and high ground only**, plus a 1 px dark outer border so
    they read on any background. Cliff and rock boxes are left out because they are larger than the
    real shape, and the blue of low/deep ground because it is confused with water — contours show
    the terrain instead.
  - Terrain: contours (the thick 10 m lines a little darker) and a **solid blue shoreline** (a dry
    pixel next to a wet one).
  - Route, trail, icons, hero and N are unchanged.
- Settings: `big_outline` (default on), `mini_outline` (default off). Panel: "Style: Outline /
  Filled" on the minimap and big map cards.
- Display mode `display` = minimap / big map / off (the old `show false` reads as off).
  `cycle_modes` = which modes the map key cycles through (a bit set, at least one). Each press of the
  map key (default F2; F9 until 0.2.1) steps minimap → big map → off, through the chosen modes only. **The dedicated
  big map key was removed**, leaving four keys: map display mode, marker, compass, next goal.

## 11. Floors and compass distance cues (2026-10-03)

- Requested: layered above/below-ground rendering (the current floor sharp, other floors faded);
  compass icon size and opacity conveying distance; up/down arrows with the height difference for
  goals.
- Compass (`pin_look` in `src/map/compass.rs`): on a log scale from 10 m to 200 m, size goes 1.15 →
  0.7 and opacity 1 → 0.35 (the target only down to 0.7). A height difference ≥ 3 m (`FLOOR_DZ`) adds
  ▲/▼ next to the pin. Under the target, a label like `85m ▼12m` (above in sky blue, below in orange).
- Map: icons and goals more than 3 m above or below the feet fade over 3–6 m down to one-third
  opacity and get ▲/▼ (`floor_alpha`, `floor_badge`: since 2026-10-05 a badge on the mark's bottom-right corner, not a loose arrow that strayed onto neighbours). The structure bands now draw standing things
  (walls) on **the floor above** (base 3–12 m up) and **the floor below / underground** (top 1.5–15 m
  down) very faintly, like ghosts — previously they were not drawn at all. Included in outline mode.
- Measured (Jova, the Vitalis house): the Family Reunion goal is 11–14 m below (underground). The
  grid route was flat and did not know the underground entrance, which led to using the game navmesh
  (ROUTES.md §6).

## 12. Water out to its shore; the ground as dots (2026-10-03)

Requested: where the ground suddenly rises out of the water is a fall that cannot be climbed back
from, so fill the space up to there with water; and the solid map hides the game, so draw it as dots
with gaps between them, as Diablo's and Path of Exile's maps do.

- **Water floods to its shore** (`relief.rs` `flood`): the game's deadly-water boxes cover only part of
  a lake, so ground below the surface around them showed as low land and the shore as a jagged edge of
  boxes. Water obstacles now keep their surface (the hazard box's top) in `zmax`; from every wet
  texel the water spreads to neighbours whose ground lies below that surface and stops where the land
  rises above it (the shore, or a cliff out of the water) or is not loaded. Route finding is unchanged:
  water blocks at any height (`pathfind.rs` tests `o.water` first).
- **Dots** (`raster.rs` `dots`, setting `dots`, on by default; the big map's option on the Map page
  "Draw as dots", 2026-10-04: the big map only, as it covers the middle of the screen; the minimap is
  small and stays solid): after the
  ground layers (disc, relief, terrain bands) every other pixel on every other row is kept and the rest
  cleared, so three quarters of the ground is gap; kept dots are drawn 1.7× stronger so the ground
  still reads. The trail, route, icons, pins and the hero mark are drawn after it and stay solid.
- **The big map over the whole screen** (2026-10-04): its canvas is the game window, centred on the
  hero, the radius reaching 86 % of the way to the short side (`FULL_FILL`, a margin above and
  below); it is a circle that fades from full strength at 45 % of its radius to nothing at its edge
  (`raster.rs` `fade_edges`), as Diablo's and Path of Exile's overlay maps do. Not an ellipse of the
  screen's shape: on a wide screen its sides reached 2.4 radii and were cut off. `View::full`: no
  disc, rim or north mark. The ground (`paint_ground`: disc, relief and the terrain's bands, most
  of the work and soft anyway) is drawn at half size and doubled (`upscale2`: weights 9/3/3/1,
  integer, rows in parallel); everything over it (`draw_above`: trail, pins, things, goals, route,
  hero) at full size, so lines and icons stay sharp. Dots grow with the screen (2 px on a 1440 px
  one; doubled, they had blurred into a haze); on a screen under 600 px tall (the Map page's
  preview, shown smaller still) they are drawn as the tone they average to, since a 1 px mask beat
  with the panel's pixels. The fade mask is cached per size. Both maps' radius is at most 400 m
  (`RADIUS_MAX`): at 500 m the edge of the loaded land showed, cut off.
- **The big map scrolls its ground** (`overlay/bigmap.rs`, 2026-10-04): north up, walking only
  slides it, so the ground is drawn once larger than the window by a margin (a tenth of its height,
  about 40 m at 400 m), the dots put on it then (they move with the land), and copied at the hero's
  offset each frame. It is drawn again, on a thread of its own while the old one keeps sliding,
  when the hero is halfway out of the margin, has climbed 1.5 m since, or what it is drawn from
  changed: the footprints by content (the list is made anew whenever any actor comes or goes), the
  relief, the settings. The game streams the land in as the hero walks, so that is every second or
  two; on the overlay's thread each took 80–140 ms and stopped the map.
- **The hero glides** (`overlay/glide.rs`): the worker reads the game ten times a second; between
  readings the pose is interpolated from where it is shown to the latest reading, over the time
  readings take (60–200 ms), so the maps move smoothly a reading behind at most. Since §13 the
  overlay reads the pose itself every frame, and the glide is only the fallback. Over 30 m is a
  jump, shown at once; yaw turns the short way. While the big map shows the overlay draws every
  30 ms (else 50).
- **The trail is sliced to draw**: the latest 2,000 points, fading to nothing at the slice's start,
  a segment only every 4 px on screen. The trail itself keeps every point.
- **What it cost** (3440×1440, big map, a frame): before, about 140–170 ms (terrain supersampled at
  full size 111–120 ms, doubling 20–45, ground 1–20, fade 2–3); after, 14–26 ms, at most about
  30 (copy 2–8, trail 4–11, things 2–3, fade 2–7, present 4–12).
- **Other maps' ground is cached** (`raster.rs` `GROUND`): the disc, relief and terrain depend only
  on where the map stands and how it is drawn, so while the hero stands still (or moves under a
  pixel) the last frame's ground of that size is copied instead of drawn.
- **Two previews** on the Map page: the big map as it covers the game window (made small,
  `raster::downscale`) and the minimap beside its settings; the preview no longer follows the
  display mode.
- **Layer opacity** (2026-10-04, setting `opacity` = ground, lines, icons in percent; Map page "Layer
  opacity"): the ground (disc, relief and terrain fills), the lines (contours, shore, terrain edges,
  trail, route) and the icons (things, pins, goals, north, the hero) are faded separately at the point
  each is drawn (`raster.rs` `faded`); the big map's own opacity multiplies them. Presets: Solid
  100/100/100, Balanced 60/90/100, Subtle 30/65/90, Icons only 0/35/100; anything else shows as
  Custom.

## 13. Latency and CPU (2026-10-04)

Reported: the minimap ran about a second behind the hero. Asked: research memory, CPU and tick
optimisation, then do all of it.

**Why it was late.** The worker read the pose first in its step, but published the snapshot only at
the step's end, after the actors, the guide and the toggles; and it then waited a whole `STEP`
before the next. The glide added up to 200 ms. On top, steps that coincided with the periodic rescans
(the actors every 1 s, the guide's triggers and knowledge every 2 s) ran to 300 ms or more.

**Research** (what applied here):

- Reading another process's memory costs per call, not per byte, and Windows serialises the calls
  against one process, so more threads do not help: fewer, larger reads do (HunterPie cut ~474
  calls a tick to 50–80, github.com/HunterPie/HunterPie/issues/828; memflow's page cache,
  docs.rs/memflow).
- Fast and slow work at their own rates: what the screen needs every frame read every frame, the
  rest time-sliced and paced (allenchou.net/2021/05/time-slicing; gafferongames.com "Fix your
  timestep").
- A layered window is composed on the CPU and its cost grows with its area; keep it small and update
  it only when it changed (learn.microsoft.com, UpdateLayeredWindow and "Windows with C++: High
  performance window layering", MSDN Magazine 2014-06). DirectComposition with a flip-model swap
  chain avoids the copy altogether: not done, a larger change.
- Share snapshots by `Arc` rather than cloning large lists each frame (docs.rs/arc-swap).
- Rust's `thread::sleep` already uses a high-resolution waitable timer on Windows
  (rust-lang/rust#116461); no `timeBeginPeriod`.

**Done, measured with `doctor profile` in play (steady state, after an 8 s warm-up):**

| Change | A worker step, mean / worst |
|---|---|
| Before | 55 ms / 152 |
| The two object walks rest between passes (quests 5 s, ground 3 s) | 22 / 97 |
| Their class pointers read a 4 KiB page at a time, objects in address order (`mem::Paged`) | 19 / 100 |
| Names, properties and lineages kept once read (`names.rs` `Cache`) | 10.7 / 64 |

1. **The pose every frame** (`player::PoseSource`): the worker hands over the controller, the pawn and
   the offsets; the overlay reads the pose through a read-only handle each frame (four reads,
   checked against the controller still holding that pawn) and draws it as read. The worker's pose
   remains the fallback, glided.
2. **Shared, not copied**: the snapshot's things and goals are `Arc`s; the overlay keeps what it shows
   (the survey's things merged, the consent's filter, the pins added) and works it out again only
   when the worker brings new lists, the consent or the pins change (`hud::Shown`, `hud::Pinned`).
3. **Steps on a beat**: steps start `STEP` apart, not `STEP` after the last ended.
4. **Fewer calls**: as in the table. The attributes the toggles hold went from 4.2 to 0.4 ms a step,
   the once-a-second derived lists from 31 to 7.8 ms; the first step from 1.4 to 1.1 s.
5. **The big map**: its window is as wide as it is tall (the short side, centred), since past the
   circle every pixel faded to nothing; at 1440p, drawing 4.8 → 2.8 ms a frame and showing it 5.3 →
   3.2 ms (ignored benchmarks in `bigmap.rs` and `layered.rs`). A frame that would show what the last
   did is neither drawn nor shown again, but one is drawn at least every 250 ms for what its key
   leaves out (the settings, the trail).

The worker and the overlay log their spans every 30 s (`worker time …`, `overlay time …`).

## 14. The scene kept on disk (2026-10-05)

Every run started with no ground on the maps (and no obstacles for the routes) until the first
pass over the game's objects ended: about 6 s from launch, measured. Like an engine's shader
cache, each world's scene is now kept in `Mods\cache\<world>.bin` (`scene_cache.rs`, under
`paths::data_dir()` like every other file of the mod) and shown as soon as the world is known.
Captured: the Guide page's map had its ground 4 s after launch, before the first pass ended.

- **Ground is gathered.** The game holds only the landscape near the hero, so each pass's
  heightfields are merged into the kept ones (a live field replaces its square; the rest stay).
  The maps show ground walked before even where it is not loaded now.
- **Obstacles are not merged** (they can move): the kept ones stand in only until the run's first
  pass, which replaces them.
- **Written** when the ground grew, and once a run after the first pass: off the worker thread,
  to a `.part` file renamed over the old one. Acasa Marshes: 9.4 MB.
- **Format**: little-endian, `HIUS` and a version, the heightfields, then the obstacles. Another
  version, a cut-short file or a count past the end is ignored and written anew.
- A world name that does not read (loading) keeps the scene; another world resets the pass and
  loads that world's kept scene.
- The file is made from the game's memory: it stays on the player's machine, never in the repo.


## 15. The landscape from the cooked maps (2026-10-05)

The user: draw the requirement graph in 3D over the game's own map, first as an artifact, then as a 3D
map tab, then routes and pins on the game screen. No map image fits (§3: the country map is not to
scale, and no region map ships). The landscapes are in the cooked maps instead: each
`LandscapeComponent` has its own 256×256 `HeightmapTexture` (B8G8R8A8, height = R·256 + G, 32768 is 0,
128 a unit; `HeightmapScaleBias` picks its part), placed at its `SectionBaseX/Y` (quads) less the
proxy's `LandscapeSectionOffset`, through the `LandscapeStreamingProxy`'s root transform (scale about
50 × 50 × 100, a 90° turn). `survey --terrain <dir> [--world W] [--cell cm]` samples every component
into one grid per world (`<World>.terrain.json`: origin, cell, size, heights as base64 little-endian
int16 decimetres, −32768 where there is none), kept in `Mods\terrain` on the player's PC. Checked
against the save points and the APC door of Lake Cynon: within a few centimetres.

Coverage of the graph's nodes by the landscape (nodes within 15 m of the ground): Acasa Marshes 88 %,
Jeljin 90 %, Talju 84 %, Marastan 79 %, Vyssa Hills 55 %, Plains of Mist 21 %, Lake Cynon 11 % (the
forge and the Eye of God are interiors and another area), Senedra Forest 1 % (to look into), the Lethe
buildings and Auriga none (interiors). Interiors need the buildings' meshes or the navmesh.

The prototype (an artifact, Acasa Marshes): the terrain as a shaded mesh, the graph's nodes where they
stand (a stalk down to the ground for those above or below it), its edges as arcs, and a story-round
scrubber that lights what can be done by then.

### 15.1 Underground: the navmesh from the cooked maps

The user asked whether the underground can be had too, and to show it as an ant farm. Interiors are
not landscape, but the walkable floor is in the cooked maps: the `NavigationDataChunkActor`s'
`RecastNavMeshDataChunk`s (Lake Cynon: two packages). The survey tool's reader does not parse them,
so `survey --raw <package> --out <file>` writes a package's bytes, and the tiles are found in them:
the header is written field by field (u16 version 7, i32 tile x, i32 y, u16 layer, u16 polys, u16
verts, …, then bmin/bmax as six doubles ending 8 bytes before the vertices, which start 87 bytes after
the header), and the vertices (three doubles, Recast space) and polys (32 bytes) are as the live tiles
have them (ROUTES.md §6). Rebuilt into the live layout, they read with `navmesh.rs` as they are. Lake
Cynon: 1663 tiles, 29930 polys, 93 % of the tiles a live read of the same place found, byte for byte;
Acasa Marshes: 10965 tile records. Each floor poly is then on the ground, under it (more than 3 m below
the landscape) or indoors with no landscape. The prototype's ant-farm view thins the ground, shows the
floors under it, and cuts away everything above a chosen height.

## 16. The 3D map page (2026-10-05)

`hiumod doctor map3d` runs the survey tool's `--terrain` and `--navmesh` for every world into
`Mods\terrain` and `Mods\navmesh` (`<World>.navmesh.bin`: [u32 length][tile] in the live layout; the C#
scanner gives Lake Cynon's 1663 tiles byte for byte as the prototype did; every world has floors,
interiors included: Lethe Library 111 tiles, Lethe Propaganda 1780, Auriga 268).

The panel's 3D map page (`panel/map3d.rs`, in the Wayfinding group, behind the map consent) loads the
hero's world off the panel's thread — the landscape (none for an interior: the floors' bounds centre
the view), the floors split into on the ground, under it (coloured by depth) and indoors, and the
requirement graph's places with their rounds — and draws it with OpenGL in an egui paint callback (the
window now asks for a 24-bit depth and an 8-bit stencil buffer; the callback scissors to its own rect
and puts egui's blend state back). The techniques are the prototype's, from the research on seeing the
ground and the underground at once:

- the ground opaque, pixels dropped by a screen-door dither (interleaved gradient noise) in a keyhole
  along the line from the eye to the point looked at, and everywhere by "Ground" (Cesium's ground
  translucency, BG3-style occluder fading, without blending's sorting trouble);
- X-ray: floors and places drawn again with the depth test GREATER as faint silhouettes, one layer per
  pixel by the stencil;
- contour lines (5 m, 25 m), hillshade, distance fog;
- the route in focus (`Shared::route3d`, the overlay's route with its heights: the navmesh's floor, or
  the ground on the grid's way) as a band over the floors, faint where it is hidden.

Drag turns, a right drag moves, the wheel zooms, a click picks the nearest place (its class, round and
depth). Story round filters the places.

## 17. The route on the game's view, hidden where the world hides it (2026-10-05)

The user: the route over the game should be hidden where meshes hide it, as a game's own pathfinding
line looks, and the 3D map should show the route in 3D (§16). The overlay cannot read the game's depth
buffer, but it has the world's shape the routes use: the obstacles (convex outlines over height
ranges, from the static meshes' collision) and the landscape. `overlay/screenroute.rs` samples the
route in focus (`Shared::route3d`, with its heights) every 50 cm up to 80 m ahead and casts a line from
the game's camera to each point, short of the point's own floor: hidden when it crosses an obstacle's
prism (Cyrus–Beck clipping of the line against the outline, then its heights there against the
obstacle's range) or, with neither end under the landscape, dips under it (underground the landscape is
overhead everywhere, and walls and floors are obstacles). What is seen is laid on the floor as a 70 cm
band in perspective, a brighter chevron every 3 m, fading ahead, in the route's colour. It is drawn
into a click-through window over the band's bounds (in 128 px steps, not the whole screen), at most 30
times a second, with the guide's consent and the route shown. Tests: a line through a box is clipped
to it; a wall hides what is behind it but not what is seen over it.

Limits: occlusion is as good as the obstacles: thin or skipped meshes (foliage, small props) do not
hide the band, and moving doors are where the last pass of the objects saw them.

### 16.1 Map-app controls over the view (2026-10-05)

The user: the hero's position, and the controls inside the 3D canvas as in Naver or Google Maps. The
view now fills the card and its controls float over it in glass panels: the layers (X-ray, keyhole,
ground) top left with the picked place's card under them, a compass top right (it turns with the view;
a click puts north up), zoom and "follow me" bottom right, the story round along the bottom, the hint
faint bottom left. The hero is a blue dot with a white rim, a cone the way they face (the pose's yaw,
projected) and a pulse; the route's end is a pin. Following (on by default, a right drag lets go,
"follow me" takes it back), the view eases after the hero. Places are drawn 3–14 px whatever the
distance, as a map's icons are.
