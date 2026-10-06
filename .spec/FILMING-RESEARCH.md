# Filming: camera language and a director that chooses shots

Research for the filming mode's next step (2026-10-07): which camera moves film, television, drones
and 3D motion graphics use — especially those that move the camera vertically and change its
distance at once — and how a take could choose them by itself from the route and what stands
around it. The design at the end is a proposal; nothing of it is built yet.

## 1. What a take can move

From the probes (CHEATS.md "Filming mode"):

| Control | How | Range seen |
|---|---|---|
| Where the camera turns about (pivot) | camera mode `PivotToViewTarget`, hero's frame | 78 m away with the safe-location checks off |
| Camera yaw and pitch | controller `ControlRotation` | the rig's own smoothing, ~6° behind at 45°/s |
| Distance behind the pivot | exploration/combat/APC config `DefaultDistanceFromPlayer` | live, smooth, 250–1500 cm offered |
| Field of view | the same configs' `FieldOfView` | live, 30–100° offered |
| The hero's walk | `Pawn.ControlInputVector` | the game's own walk |

So the camera is an orbit rig about a movable point: a position on a sphere round the pivot
(azimuth, elevation, distance), a look direction, a lens. Every move below is a path in those few
numbers.

## 2. The moves

### Film and television

- **Dolly / track** — the camera moves toward, away from or alongside the subject. **Truck** is
  sideways. **Arc** circles the subject; without music it reads as something circling, predatory.
  ([StudioBinder, every type of camera movement](https://www.studiobinder.com/blog/different-types-of-camera-movements-in-film/),
  [MasterClass](https://www.masterclass.com/articles/guide-to-camera-moves))
- **Pedestal** — the whole camera rises or lowers close to the subject, controlled and precise.
  **Boom / crane / jib** — the grand version, on an arm: a **crane up** reveals a place, a **crane
  down** arrives in it. A **dolly boom** moves forward or back while rising or falling; a boom
  with a pan sweeps a large scene. ([StudioBinder, crane shot](https://www.studiobinder.com/camera-shots/camera-movements/crane-shot/),
  [boom shot](https://www.studiobinder.com/camera-shots/camera-movements/boom-shot/))
- **Dolly zoom** (Vertigo, Hitchcock 1958) — dolly and zoom opposite ways so the subject keeps its
  size while the background stretches or squeezes: unease, a realisation, fear of height. A
  crane can be combined with it to rise over a place and express distance or closeness.
  ([Wikipedia](https://en.wikipedia.org/wiki/Dolly_zoom),
  [StudioBinder](https://www.studiobinder.com/camera-shots/camera-movements/dolly-zoom-shot/))
- **Angle** — a high angle makes the subject small, vulnerable, alone; a low angle makes it
  strong, dominant, threatening; eye level is equal; a bird's eye establishes the place and the
  subject's place in it. ([Celtx](https://blog.celtx.com/types-of-camera-angles-guide/),
  [Wikipedia, camera angle](https://en.wikipedia.org/wiki/Camera_angle))
- **The 180° rule** — keep the camera on one side of the line of action so screen direction holds;
  a continuous move may cross it, and doing so marks a shift.
  ([Wikipedia](https://en.wikipedia.org/wiki/180-degree_rule))

### Drones (DJI)

QuickShots, each a fixed move with the camera kept on the subject
([DJI support](https://support.dji.com/help/content?customId=en-us03400006482&spaceId=34&re=US&lang=en&documentType=artical&paperDocType=paper),
[Digital Camera World](https://www.digitalcameraworld.com/tutorials/dji-quickshot-modes-explained)):

- **Dronie** — back and up together (distance and height grow at once), 20–50 m.
- **Rocket** — straight up, the camera tilting down to keep the subject.
- **Helix** — a spiral: round the subject while rising and moving away.
- **Boomerang** — an oval out and back, rising at the far side.
- **Asteroid** — back and up to a high panorama, then the descent back down.

**MasterShots** chooses for itself: from the subject's kind and its distance it picks a
Proximity, Landscape or Portrait programme, then flies 10–14 moves — zoom in/out, circles near,
middle and far, dronie, pitch up and fly forward, rocket, camera down circle, descend with the
camera level or down. ([DJI Air 2S guide](https://store.dji.com/guides/dji-air-2s-mastershots/),
[DJI forum](https://forum.dji.com/thread-277800-1-1.html))

### 3D motion graphics and game cameras

- Moves on splines (rails), aligned to them; a separate look-at target; ease in and out on every
  key; **anticipation** before a move and **overshoot** that settles after it give life.
  ([Novedge, Cinema 4D](https://novedge.com/blogs/design-news/cinema-4d-tip-mastering-camera-animation-techniques-in-cinema-4d),
  [Animation Mentor](https://www.animationmentor.com/blog/tutorial-animate-with-timing-and-spacing-in-mind/))
- Game cameras: **look-ahead / lead room** — the look point 2–3 m ahead of a running subject,
  eased so turns do not swing it; **occlusion** must never hide the subject (a sphere cast from the
  target); a hit pulls the camera in along the ray.
  ([Game AI Pro, robust third-person camera](https://www.gameaipro.com/GameAIPro/GameAIPro_Chapter47_Tips_and_Tricks_for_a_Robust_Third-Person_Camera_System.pdf),
  [GDC, real-time camera design](https://gdcvault.com/play/mediaProxy.php?sid=1014099))

### Automatic cinematography

- **Toric space** — a camera parameter space round one or two subjects (two angles and a framing
  distance) in which shot constraints are easy to state and search; extended for drones.
  ([Autonomous execution of cinematographic shots with multiple drones](https://arxiv.org/pdf/2006.12163))
- **Learned shot selection with occlusion** — a signed-distance map of the scene scores how much
  each candidate shot (front, back, left, right…) sees the subject; a learned policy picks the
  shot type for the context; trajectories are optimised for smoothness, safety and visibility.
  ([Bonatti et al., learned artistic decision-making](https://arxiv.org/html/1910.06988v1),
  [artistic principles, occlusion-free](https://arxiv.org/pdf/1808.09563))
- **CineMPC** — zoom and focus planned together with the drone's pose for composition.
  ([arXiv 2401.05272](https://arxiv.org/pdf/2401.05272))

## 3. Moves that change height and distance together

| Move | Height | Distance | Lens | Reads as |
|---|---|---|---|---|
| Crane up reveal | rises 5–20 m | grows | wider, tilts down then level | here is the place |
| Crane down arrival | falls to eye level | shrinks | | we arrive |
| Dronie | rises | grows, behind | | leaving, the end |
| Rocket | rises straight | grows by height | tilts down | above it all |
| Helix | rises | grows | orbiting | a flourish, a finale |
| Asteroid | rises far, then falls | out, then in | | from the sky down to them |
| Dolly boom (push-in rising) | rises a little | shrinks | | going toward something above |
| Low pedestal follow | stays low | close | wide | climbing, strength, threat ahead |
| Vertigo crane | rises | grows | FOV narrows (keeps the hero's size) | the drop below, dizziness |

## 4. Proposal: a director that chooses shots

A new camera mode, **Director**, for walks (and flights). Before the take it reads the route and
the scene, cuts the route into **beats**, picks a **shot** for each, and while rolling blends the
shots' rig numbers with eased transitions — one continuous take, no cuts.

### What it reads along the route (every 2 m)

- **Openness**: obstacles and walls within 6 m either side (the scene's outlines), indoor floors
  (navmesh kind) or a landscape overhead — corridor, room, open ground.
- **Height**: the route's rise and fall over the next 10 m; off-mesh links (ladders, drops), lifts.
- **Turns**: the route's heading change over the next 6 m.
- **Edges and vistas**: the landscape falling away ahead of or beside the route; the floors' free
  edges near it.
- **Interest**: the guide's places near the route (doors, puzzles, saves, items), the destination,
  enemies near.

### Rules (first version)

| Beat | Shot |
|---|---|
| The start, open | crane down arrival from 12 m behind and above, to a follow |
| The start, indoors | low push-in to a close follow |
| Long straight, open | side track (truck) on the outside of the next turn, lead room ahead, a slow boom up over 8 s |
| Corridor | low, close follow (Steadicam), a wider lens |
| Turn | arc round through the turn, keeping to one side of the line (180°) |
| Climbing (stairs, slope up, ladder) | low angle from below, pedestal rising with the hero |
| Going down (slope, drop) | high angle, boom down after the hero |
| Nearing an edge with a view | crane up reveal over the hero toward the view |
| Nearing a door or place of interest | push-in (dolly in, lens a little narrower), the place in frame |
| Lift | helix round the shaft as it rises |
| Enemies near | low angle, a slight vertigo (distance out, lens in) |
| The end | dronie: back and up, the hero small in the place |

### Keeping it watchable

- Each shot is a target in rig space (azimuth to the way, elevation, distance, lens, look offset);
  the rig eases toward it with the gimbal's damped spring (the Cine cap), the change starting 1.5 s
  before its beat (anticipation), and a little overshoot only on reveals.
- A shot holds at least 4 s; a beat shorter than that is merged into its neighbour (hysteresis).
- The side of the line is kept unless a turn or an arc crosses it in one continuous move.
- Visibility: the line from the camera to the hero is tested against the obstacles every tick; if
  hidden, the elevation rises and the azimuth slides toward the open side first, the distance
  closes in last (the room check already in place).
- Variety: the same shot is not used twice in a row where another fits.
