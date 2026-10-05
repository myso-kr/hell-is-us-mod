# Routes — from the hero to the goal

How the mod finds a walkable route to the guide target: research, decisions and implementation
notes. Steam build 24045435.

**Current approach:** the game's own navmesh (§6–7) is used first. When the hero is off the navmesh,
the route falls back to A* on an obstacle grid (§1–5). §1–5 are written in the order the grid
evolved; each section fixed a problem found in the one before it. Their conclusion that the game has
no navmesh in memory (§2) was overturned by §6.

## 1. A* movement route (2026-10-02)

Requested: a real walking route using A*. (The same request also covered the big map and hiding
overlays when a game menu opens — MAP.md §7.)

### A* (`src/guide/pathfind.rs`)
- At the time the game NavMesh (Recast `dtNavMesh`) looked unreachable — outside reflection, and no
  public example of reading it from outside the process — so the first grid was **built from data the
  mod already had**. Costs: a wall at the hero's height (the minimap's wall layer) = 400, cliffs and
  rocks (their boxes are larger than the real shape) = 12, open ground = 3, the trail already walked =
  1. Even walls do not fully block, so a goal enclosed by a box still gets a route.
- Grid: the box around hero and goal plus 40 m, at most 400 cells per side, cells at least 1 m.
  8-direction A* with an octile distance heuristic.
- String pulling: two points are joined straight when the line between them crosses no wall. At first
  cliffs also counted as blocking, which gave 135 points outdoors; counting only walls gave 13 points
  and 360 m for the same goal (22 ms).
- Recompute when the goal changes, when the hero moves more than 5 m, or every second (overlay
  thread).
- Drawing: a route line in the goal's colour with a black border on the minimap and big map, clipped
  to the circle. The compass points at the spot 8 m ahead along the route, and the distance shown is
  the route length.
- Limits: no knowledge of landscape height, water or drops — all open ground counts as walkable.
  Goals on another floor (up a staircase) can be misjudged because walls are tested at the hero's
  height.

## 2. Route ignored the terrain → collision-shape obstacles (2026-10-02)

Feedback: the route was drawn without regard to the actual terrain and objects.

### Cause
The §1 grid only knew the **render boxes** of static mesh *actors*. Walls were merely "expensive but
passable"; rocks, cliffs and trees are mostly **instanced meshes** and were missing entirely; terrain
slope was unknown.

### Searching for the game NavMesh (negative at the time — see §6)
- There is one `RecastNavMesh` actor (World Partition, statically generated; reflected properties end
  at +0x650), but no `DNAV` header could be found along actor → implementation → dtNavMesh → tiles.
- Scanning the entire process memory (7–14 GB) for the `DNAV` (Detour tile) and `DTLR` (TileCache)
  magics found **zero**. Conclusion then: no ground navigation data is in memory for this region.
  §6 later found the tiles in a different place, without those magics.
- The `VoxelNavigationData` instance is `DroneNavigationData-Drone` — 3D navigation for drones.

### Fix: collision-shape obstacles (`src/read/obstacles.rs`)
- Read each mesh's **collision shape**, `UStaticMesh.BodySetup.AggGeom` (Box/Sphere/Sphyl/Convex, all
  reflected), as points in mesh space (cached per mesh). Trees become trunk capsules; moss and grass
  have no shape and drop out naturally. If `CollisionTraceFlag` is complex (3), fall back to the render
  bounds.
- Place each instance in world space: the instance matrix (`PerInstanceSMData`, FMatrix of doubles,
  row-major) → the component's **`ComponentToWorld`** (outside reflection, USceneComponent+0x1D0;
  found as the only FTransform on the hero's root whose translation equals `RelativeLocation`). The
  result is a 2D convex hull plus a height range.
- Skip components with collision off (`BodyInstance.CollisionEnabled` = 0) or an overlap-only profile
  (NoCollision, OverlapAll, Trigger, …).
- GUObjectArray is scanned 12k objects per step, and the obstacle set is swapped at the end of each
  full pass. Caching field offsets per class cut a pass from 76 s to 2.5 s.
- Measured: 69,231 obstacles (those with collision among 71k Foliage instances, 10k HISM, 3.7k ISM);
  5,838 of them block at the hero's height.

### A* changes (`src/guide/pathfind.rs`)
- An obstacle that spans the body — above +45 cm from the feet (a step) and below +180 cm (the head)
  — **blocks completely**. Only if no route exists does a second pass allow crossing at a high cost
  (400). A 1.2 m area around start and goal is cleared, for goals whose object is itself a collider.
- 50 cm cells, at most 600 per side. Measured: a route weaving through a gap in a band of rocks and
  cliffs, 14 points, 146 m, 18 ms.
- Remaining limits: landscape slopes and cliff faces are still unknown (no CPU-side height collision
  data was found — the 256 components' heightmaps are GPU textures; §3 found it). Water is unknown.

### 2.1 Routes still cut through obstacles (2026-10-02)
- **Thin obstacles leaked**: a cell was blocked only when its *centre* was inside a polygon, so fences
  and walls thinner than a cell (50 cm) let routes through. Fix: obstacles are stamped inflated by the
  hero's radius, 35 cm (distance to the polygon edge), and routes never squeeze diagonally between two
  blocked corners. String-pulling checks sample 3 times per cell.
- **Search area**: if the box around both ends plus 40 m had no way round, the search went straight
  to the costly crossing pass. Fix: widen the margin 40 m → 150 m → 400 m and search again before
  allowing a crossing.
- **Showing uncertainty**: crossing legs are marked in `Path.through` and drawn as a **yellow dashed
  line** on the map, with a warning in the Guide tab. Measured: 35 points / 176 m (2 crossing legs) →
  18 points / 344 m, with one crossing of a few metres at the edge of a round structure.
- Meshes without a simple collision shape (80 m radius survey): 2,165 are BlockAll with no shape,
  almost all road lines, dead grass and decals (`Road_Line`, `Dead_Grass`). Substituting render bounds
  would block roads, so they **stay excluded**. Only complex-flag (3) meshes fall back to bounds.

## 3. Water, slopes, bridges — the terrain heightmap (2026-10-02)

Feedback: routes did not detect water. The options — adding water from outside the process versus
injecting into UE to run physics queries — were compared, and the **external approach was kept**
(DECISIONS.md D15).

### Water is not collision
- Water surface meshes: `Deep_Water_04_SM` is a 5 km × 5 km plane (z −182, `NoCollision`);
  `Shallow_Water_*` uses profile `BlockMaterialCast&Camera` (blocks only the camera, so it is now on
  the passable list).
- Deadly water: the 12 `TriggerEvent` BoxComponents (`OverlapOnlyPawn`) of
  **`DeadlyWaterAcasa01~09_PassiveInteract_BP`**. They reach from the lake bed (z ≈ −793) to just
  below the surface (−193). The boxes are large (up to 400 × 250 m) and **cover shore land in 2D**,
  so water = "inside a box, and the ground is below the box's top".
- First attempt (using grass and rock instance origins as ground samples) failed: bare ground has no
  samples, so nearly everything became water.

### Finding the terrain heightmap (CPU-side Chaos heightfield)
- `LandscapeHeightfieldCollisionComponent` (256 of them) → `HeightfieldRef`, the **native member right
  after** the reflected `CookedPhysicalMaterials` (+0x580, 16 B), at +0x590 →
  `FHeightfieldGeometryRef::HeightfieldGeometry` (+0x30) → `Chaos::FHeightField`. Code:
  `src/read/terrain.rs`.
- `GeomData`: Heights TArray<u16> +0x20 · Scale (double×3) +0x50 = (100, 100, 0.78125) · MinValue
  +0x80 · MaxValue +0x88 · NumRows/NumCols (u16) +0x90 · Range +0x98 · HeightPerUnit +0xA0
  (= Range/65535).
- Height = `(Min + h·HPU)·Scale.z + component z`; rows run along Y. Four interpretations were checked
  against 1,345 grass and foliage instances: this one had a median error of 28 cm and a bottom-decile
  error of −1 cm (grass sits on the ground). Reads are validated by header consistency (square, counts,
  HPU·65535 ≈ Max−Min).
- Dead end: `LandscapeComponent+0x6C8 → +0x358` differs from component to component — abandoned.

### Use
- **Water**: 2 m cells inside a box where terrain < the box top (1,104 row-wise runs). Cells covered
  by a collider rising above the surface (bridges, piers, rocks) are removed. (§5 narrowed this.)
- **Slope**: precomputed per vertex (neighbour differences). Over 40° costs 12, over 55° costs 60 —
  not blocked, since the terrain might lie under a structure. (§5 replaced this.)
- **Walking surface per cell**: the terrain height, or the top of a **floor** covering the cell
  (thinner than 1.5 m, area ≥ 4 m², top within 1.5 m above the terrain and the hero). An obstacle
  blocks when it spans body height (+45 to +180 cm) relative to the cell's walking surface **or** the
  hero's feet. Obstacles thinner than 20 cm never block. This recognises rocks above and below a
  slope, and **bridge piers no longer block the deck** (found in a measurement taken with the hero
  standing on a stone bridge).
- Measured: starting on the bridge, the route follows the shore north into the village, 355 m, with
  no water crossing. Computing takes ~0.3–0.5 s, so it runs on **its own thread**.
- Remaining limits: floor vs wall is a guess from shape proportions (two-storey structures, ramps and
  stairs may be wrong). Unloaded terrain is unknown.

## 4. Route flipping back and forth (2026-10-03)

- Report: while following the guide, it suddenly said to turn back, and after turning back it pointed
  forward again.
- Cause: the route was recomputed every second (or every 5 m moved) and **always replaced**. Two
  nearly equal detours (onward / back along the trail) won alternately. The trail cost a third of open
  ground, so the backward one often won.
- First fix: (1) `pathfind::better` — replace only if the new route is shorter than 85% of what is left
  of the old one (distance off it + remaining length), or the hero is more than 15 m off the old
  route, or only the old route has an estimated crossing leg. A changed goal replaces immediately.
  (2) Trail cost 1 → 2 (`TRAIL`; open ground `OPEN` is 3).
- Second fix, same day: the route then stuck with a fixed waypoint and stopped updating — the 15%
  rule was too sticky. Cutting a corner left the nearest segment behind the hero, so the compass kept
  pointing at a corner already passed. Now: (1) replace always if the new route heads the **same way
  (within 100°)** as the current direction, always if the hero is more than 6 m off, and a route that
  turns back only if it is 15% shorter. (2) `next_point` picks the next point from the **furthest
  along** of the segments within 3 m of the nearest one.

## 5. Routes over water and steep terrain (2026-10-03)

- Reports: routes often led across water; they cut through steep terrain that cannot be climbed; and
  with no jump, slopes too steep to climb must be avoided.
- **Water**: to keep the ground under bridges dry, the rule was "2 m cells under an obstacle rising
  above the surface are dry". In the Acasa marshes every tree, rock and reed qualified, so the water
  was full of holes and routes crossed through them. Fix: only **decks** are dry — top between 0.5 m
  below and 3 m above the surface, thinner than 2 m, at least 2 m² seen from above. Measured (Acasa):
  water area 22,952 → 28,988 m² (+26%).
- **Slope**: slopes over 40° only cost 12/60, so cliffs were crossed at a price. Fix: **over 45°
  (rise/run > 1.0, Unreal's default walkable angle) blocks**; 35–45° costs 12. A blocked slope, like
  other obstacles, is crossed only as an estimated leg (yellow dashes) when there is no route at all.

## 6. The game navmesh found (2026-10-03)

- The tiles are not reached through a `RecastNavMesh` actor (§2) but through World Partition **`NavigationDataChunkActor` actors** (18 in
  this region) → `NavDataChunks` (+0x2A8) → `RecastNavMeshDataChunk` ("RecastNavMesh_…-Default")
  +0x30 = TArray<FRecastTileData>. **Element size 0x48**: +0x14 TileDataSize, +0x18
  TSharedPtr<FRawData>{obj, ctrl} with obj+0 = raw tile bytes; +0x28/+0x30 the compressed tile cache
  (unused). Reading elements as 0x40 at first made four fifths look like garbage.
- Raw tile header, 0x58 bytes: u16 version (7), layer, polyCount, vertCount; i32 x, y; u16 counts
  (links, detail, BV, off-mesh, …); +0x28 double bmin[3]/bmax[3]. Vertices start at 0x58 as double[3]
  (Recast x, up, z → Unreal (−x, −z, up)), followed by 32-byte polys (u32 firstLink, u16 verts[6],
  u16 neis[6], u16 flags, u8 vertCount, u8 area|type<<6). Only vertices and polys are used — the size
  formulas for the remaining sections were left unfinished (not needed).
- Measured (Jova): 3,906 tiles, 22,898 polys. Of 14,726 tile-border edges, 14,213 connect. The
  basement (58 polys) is **disconnected** from the ground floor because the secret office door
  (`VitalisOfficeOpened`, puzzle/key) is closed.

## 7. Navmesh routes (2026-10-03)

- `src/read/navmesh.rs`: decode tiles → poly graph (neighbours within a tile from `neis`, 1-based;
  across tiles, border edges on the same axis line that overlap and are within 60 cm in height) →
  locate a position (2D containment on the nearest layer by height, otherwise the nearest centre
  within 5 m) → poly A* (portal midpoints) → funnel straightening. A route takes ≈ 0.1–4 ms.
- **Blocked goals** (behind a door or puzzle): walk to the reachable poly nearest the goal (height
  difference weighted ×2), then a final "crossing" leg (yellow dashes). The tracker says: "Past a
  closed door or puzzle — guiding as far as it goes; look for a note, key or device nearby first".
- **Puzzle awareness** (requested: take the puzzle elements that open the door into account): when a
  route ends in a crossing, the goal is remembered as `blocked`. Auto-guide prefers goals reachable on
  foot. If every wanted goal is blocked, it goes to a **reachable Open goal** (note, key, lever, puzzle
  device) within 40 m of the nearest blocked goal, and failing that to the blocked goal itself.
- Engine: the quest pass collects `NavigationDataChunkActor`s; whenever the actor set changes,
  `navmesh::Nav` reads 2 tiles per step, builds the graph when all are read, and publishes it
  (`Snapshot.nav`). The overlay route thread (`src/ui/overlay/route.rs`) tries the navmesh first and
  falls back to the grid when the player is off the navmesh.
- There is also a drone `VoxelNavigationDataChunk` ("DroneNavigationData-Drone") — for flight, not
  used for walking routes.

## 8. Opened doors and the baked navmesh (2026-10-05)

The user: after a door is opened with its key or puzzle, the route keeps going the long way round.
The navmesh is the game's baked one (World Partition chunks, read when the chunk set changes): a door
closed when it was baked is a wall in it for good. Now the graph lists the doors and gates of the
hero's world the save has as used (`Graph::opened`), and `NavMesh::bridged` joins the mesh through
each: the polys within 3 m of the door (2 m up or down) fall in groups by their own links, and the
group nearest the door is joined to each other one through the door's point (a one-point portal).
Where the sides were joined already, nothing is added. `Attached::nav` makes the bridged mesh only
when the mesh or the opened doors change. Test: `an_opened_door_joins_the_two_sides`. A fresh route
that no longer goes through something replaces the old one at once (`pathfind::better`).

## 9. What "blocked" meant, audited (2026-10-05)

The user: at a one-sided door there was surely a way round, but the guide only said the way was
blocked; audit what counts as blocked over the whole data and put it right.

A route is "through something" (`Path::uncertain`) when the navmesh cannot join the hero's poly to
the goal's: it walks to the nearest reachable poly and draws the last leg as through. So "blocked"
is exactly navmesh connectivity. `hiumod doctor nav` writes every loaded tile
(`Mods\doctor\navtiles.bin`, each tile's length and its raw Detour data) to look into offline. At the
Lymbic Forge (LakeCynon, 28 chunk actors):

| Measured | Finding |
|---|---|
| Tiles | 3624 records of 1794 tiles: every tile comes from two or more chunks (1310 groups byte-identical). Harmless to joining, double the work; now read once each. |
| Off-mesh links (ladders, drops) | none in any tile: not a cause. |
| Tile borders | every border edge finds a partner; the refusals are other floors. Not a cause. |
| Components | 1679 pieces (1124 a single poly). The hero's corridor: 82 polys. |
| Between the pieces | gaps of 1.3–1.5 m, each at a door: the navmesh is baked with **every door closed**, ordinary two-way doors included. |

So "blocked" meant "behind any door the save does not have as used". The corridor's three ways out
were a two-way door with no condition (the way round the user meant) and two one-sided doors. Joining
the pieces through doors, as below, took what can be walked to from the hero from 82 polys to 1627, and
the spokes gear became reachable the way round.

The rule now (`Graph::passable`, `NavMesh::bridged`):

- A door or gate is passable when used, or when nothing more than a press opens it (its chain is
  itself: no condition, or its needs met now).
- A one-sided door still shut is passable one way, from the side it opens from (`opens_from`); a
  portal the bake left through it from the locked side is dropped (`one_way`).
- A door that needs something not had yet (a key, a lever, a puzzle) stays a wall: that is what
  "blocked" means now, and what the barrier steps (GRAPH.md §9) go to.
- Polys near a door are found by their outline (a big floor poly's corners can all be far from the
  door it touches).
- A goal within 3 m across of a floor that can be walked to, at most 8 m above it or 1.5 m below, is
  reached from there (`floor_under`): a lever on a wall, a mechanism up high, a receiver on a scrap of
  navmesh of its own (two such at the forge read as blocked before).

What stays blocked at the forge with every door open is real: the large gear and the W wrath rod under
the unsolved flood (4.9 m down), the W fear rod 12 m down (the lift).
