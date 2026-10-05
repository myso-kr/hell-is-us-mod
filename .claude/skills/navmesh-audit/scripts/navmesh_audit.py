"""Rebuild the game's walking navmesh from Mods\\doctor\\navtiles.bin and explain connectivity.

`hiumod doctor nav` writes every loaded tile as u32 LE length + the raw Detour tile (UE 5.5,
version 7, doubles): header 0x58 (u16 version, layer, polyCount, vertCount; i32 x, y; ...);
verts at 0x58 as double[3] in Recast space (x, up, z) -> Unreal (-x, -z, up); then 32-byte
polys: u32 firstLink, u16 verts[6], u16 neis[6] (1-based in-tile neighbour; 0x8000 =
external, across the tile side), u16 flags, u8 vertCount, u8 area | type << 6 (type 1 =
off-mesh link, skipped). Joined as src/read/navmesh.rs does: the same tile from several
chunks once; border edges on the same axis line, overlapping >= 10 cm, heights within 60 cm.

This is the BAKED mesh: every door is closed in it (ROUTES.md §9). The running guide joins it
through doors it judges passable (`NavMesh::bridged`, `Graph::passable`); offline, a gap of
about 1.3-1.5 m between pieces is almost always a door.

Commands (positions are Unreal cm, feet height; north is -Y):
  stats                         tiles, polys, pieces (connected components)
  where X Y Z                   the poly and piece a point stands on
  why AX AY AZ BX BY BZ         whether A reaches B; if not, the gaps (likely doors) between
  why --trace                   A = hero, B = auto target of the last guide.jsonl state
Options: --file navtiles.bin, --graph graph.json (names the doors at gaps), --game DIR,
         --gap 200 (cm), --margin 15000 (cm around A and B searched for gaps)
"""
import argparse
import collections
import heapq
import json
import math
import struct
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from hiu_paths import doctor_dir  # noqa: E402

HEADER, POLY, EXTERNAL, STEP = 0x58, 32, 0x8000, 60.0
UPDOWN, NEAR = 300.0, 500.0


def read_tiles(path):
    b = path.read_bytes()
    out, o, seen = [], 0, set()
    while o + 4 <= len(b):
        n = struct.unpack_from("<I", b, o)[0]
        t = b[o + 4:o + 4 + n]
        o += 4 + n
        if len(t) >= 0x10:
            key = (struct.unpack_from("<H", t, 2)[0], t[8:16])
            if key in seen:
                continue
            seen.add(key)
        out.append(t)
    return out


class Mesh:
    def __init__(self, tiles):
        self.corners, self.links = [], []
        self.records = len(tiles)
        border = []
        for t in tiles:
            if len(t) < HEADER or struct.unpack_from("<H", t, 0)[0] != 7:
                continue
            pc, vc = struct.unpack_from("<HH", t, 4)
            po = HEADER + vc * 24
            if po + pc * POLY > len(t):
                continue
            vert = [(-x, -z, y) for x, y, z in struct.iter_unpack("<3d", t[HEADER:po])]
            base = len(self.corners)
            tp = []
            for k in range(pc):
                q = po + k * POLY
                n = t[q + 30]
                if t[q + 31] >> 6 == 1 or not 3 <= n <= 6:
                    tp.append(([], []))
                    continue
                vs = struct.unpack_from("<6H", t, q + 4)[:n]
                ns = struct.unpack_from("<6H", t, q + 16)[:n]
                tp.append(([vert[i] for i in vs], ns))
            for c, _ in tp:
                self.corners.append(c)
                self.links.append([])
            for k, (c, ns) in enumerate(tp):
                for e, nei in enumerate(ns):
                    a, d = c[e], c[(e + 1) % len(c)]
                    if nei & EXTERNAL:
                        border.append((base + k, a, d))
                    elif nei and nei - 1 < len(tp) and tp[nei - 1][0]:
                        self.links[base + k].append(base + nei - 1)
        self.border = len(border)
        self._join(border)
        self.centre = [tuple(sum(p[i] for p in c) / len(c) for i in range(3)) if c else (0, 0, -1e9)
                       for c in self.corners]
        self.grid = collections.defaultdict(list)
        for i, c in enumerate(self.corners):
            if not c:
                continue
            xs, ys = [p[0] for p in c], [p[1] for p in c]
            for gy in range(int(math.floor(min(ys) / 1000)), int(math.floor(max(ys) / 1000)) + 1):
                for gx in range(int(math.floor(min(xs) / 1000)), int(math.floor(max(xs) / 1000)) + 1):
                    self.grid[(gx, gy)].append(i)
        self._pieces()

    def _join(self, border):
        lines = collections.defaultdict(list)
        for i, (_, a, d) in enumerate(border):
            if abs(a[0] - d[0]) < 1:
                lines[(0, round(a[0]))].append(i)
            elif abs(a[1] - d[1]) < 1:
                lines[(1, round(a[1]))].append(i)
        for (axis, _), lst in lines.items():
            al = 1 if axis == 0 else 0
            for x, i in enumerate(lst):
                pi, a1, b1 = border[i]
                for j in lst[x + 1:]:
                    pj, a2, b2 = border[j]
                    if pi == pj:
                        continue
                    lo = max(min(a1[al], b1[al]), min(a2[al], b2[al]))
                    hi = min(max(a1[al], b1[al]), max(a2[al], b2[al]))
                    if hi - lo < 10:
                        continue

                    def z(a, d, t):
                        k = 0 if abs(d[al] - a[al]) < 1e-6 else (t - a[al]) / (d[al] - a[al])
                        return a[2] + (d[2] - a[2]) * k

                    if abs(z(a1, b1, lo) - z(a2, b2, lo)) > STEP or abs(z(a1, b1, hi) - z(a2, b2, hi)) > STEP:
                        continue
                    self.links[pi].append(pj)
                    self.links[pj].append(pi)

    def _pieces(self):
        self.piece = [-1] * len(self.corners)
        self.sizes = []
        for s in range(len(self.corners)):
            if self.piece[s] >= 0 or not self.corners[s]:
                continue
            k, st = len(self.sizes), [s]
            self.piece[s] = k
            n = 0
            while st:
                u = st.pop()
                n += 1
                for v in self.links[u]:
                    if self.piece[v] < 0:
                        self.piece[v] = k
                        st.append(v)
            self.sizes.append(n)

    def _height_in(self, i, x, y):
        c = self.corners[i]
        sides = [(c[(k + 1) % len(c)][0] - c[k][0]) * (y - c[k][1]) - (c[(k + 1) % len(c)][1] - c[k][1]) * (x - c[k][0])
                 for k in range(len(c))]
        if not (all(s >= -1 for s in sides) or all(s <= 1 for s in sides)):
            return None
        for k in range(1, len(c) - 1):
            a, b, d = c[0], c[k], c[k + 1]
            det = (b[1] - d[1]) * (a[0] - d[0]) + (d[0] - b[0]) * (a[1] - d[1])
            if abs(det) < 1e-6:
                continue
            l1 = ((b[1] - d[1]) * (x - d[0]) + (d[0] - b[0]) * (y - d[1])) / det
            l2 = ((d[1] - a[1]) * (x - d[0]) + (a[0] - d[0]) * (y - d[1])) / det
            l3 = 1 - l1 - l2
            if min(l1, l2, l3) >= -0.01:
                return l1 * a[2] + l2 * b[2] + l3 * d[2]
        return self.centre[i][2]

    def near(self, p, cells=1):
        gx, gy = int(math.floor(p[0] / 1000)), int(math.floor(p[1] / 1000))
        out = set()
        for y in range(gy - cells, gy + cells + 1):
            for x in range(gx - cells, gx + cells + 1):
                out.update(self.grid.get((x, y), ()))
        return out

    def locate(self, p):
        """(poly, how) as navmesh.rs `locate`: under the point nearest in height, else nearest centre."""
        best = None
        for i in self.near(p):
            z = self._height_in(i, p[0], p[1])
            if z is not None and abs(z - p[2]) <= UPDOWN and (best is None or abs(z - p[2]) < best[0]):
                best = (abs(z - p[2]), i)
        if best:
            return best[1], "on"
        for i in self.near(p):
            c = self.centre[i]
            d = math.hypot(c[0] - p[0], c[1] - p[1])
            if d <= NEAR and abs(c[2] - p[2]) <= UPDOWN and (best is None or d < best[0]):
                best = (d, i)
        return (best[1], "near") if best else (None, "off the navmesh")


def gaps(mesh, lo, hi, gap):
    """Pairs of pieces whose corners come within `gap` across (and 100 cm in height) in the box."""
    cell = gap
    buckets = collections.defaultdict(list)
    for i, c in enumerate(mesh.corners):
        if not c:
            continue
        cx, cy, _ = mesh.centre[i]
        if not (lo[0] <= cx <= hi[0] and lo[1] <= cy <= hi[1]):
            continue
        for p in c:
            buckets[(int(p[0] // cell), int(p[1] // cell))].append((p, mesh.piece[i]))
    best = {}
    for (bx, by), pts in buckets.items():
        others = [q for dy in (-1, 0, 1) for dx in (-1, 0, 1) for q in buckets.get((bx + dx, by + dy), ())]
        for p, a in pts:
            for q, b in others:
                if a >= b or abs(p[2] - q[2]) > 100:
                    continue
                d = math.hypot(p[0] - q[0], p[1] - q[1])
                if d <= gap and d < best.get((a, b), (1e9,))[0]:
                    best[(a, b)] = (d, tuple((p[k] + q[k]) / 2 for k in range(3)))
    return best


def door_names(graph_path, world):
    if not graph_path or not Path(graph_path).is_file():
        return []
    nodes = json.load(open(graph_path, encoding="utf-8"))
    keys = ("Door", "Gate", "Lock", "Panel", "Elevator", "Lift", "Bridge")
    w = (world or "").replace("_Root_WP", "")
    return [n for n in nodes if any(k in n["class"] for k in keys) and (not w or n["world"] == w)]


def named(at, doors):
    best = min(doors, key=lambda n: math.dist(n["at"], at), default=None)
    if best and math.dist(best["at"], at) <= 400:
        return f"{best['class']} ({math.dist(best['at'], at) / 100:.1f} m)"
    return "no door in the graph within 4 m"


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("cmd", choices=["stats", "where", "why"])
    ap.add_argument("xyz", nargs="*", type=float)
    ap.add_argument("--trace", action="store_true", help="why: hero -> auto target of the last trace state")
    ap.add_argument("--file")
    ap.add_argument("--graph", help="graph.json to name the doors at gaps (default: beside navtiles.bin)")
    ap.add_argument("--game")
    ap.add_argument("--gap", type=float, default=200.0)
    ap.add_argument("--margin", type=float, default=15000.0)
    a = ap.parse_args()
    sys.stdout.reconfigure(encoding="utf-8")
    d = Path(a.file).parent if a.file else doctor_dir(a.game)
    path = Path(a.file) if a.file else d / "navtiles.bin"
    if not path.is_file():
        sys.exit(f"no {path}: run `hiumod doctor nav` with the game running and the hero in the area")
    mesh = Mesh(read_tiles(path))
    print(f"{path}: {mesh.records} tiles once each, {len(mesh.corners)} polys, {mesh.border} border edges, "
          f"{len(mesh.sizes)} pieces (largest {sorted(mesh.sizes, reverse=True)[:5]}, "
          f"single polys {sum(1 for s in mesh.sizes if s == 1)})")
    if a.cmd == "stats":
        return
    world = None
    if a.cmd == "why" and a.trace:
        st = None
        for line in open(d / "guide.jsonl", encoding="utf-8"):
            if '"kind":"state"' in line:
                st = line
        st = json.loads(st)
        if not st.get("auto"):
            sys.exit("the last trace state has no auto target")
        A, B, world = st["hero"], st["auto"]["at"], st.get("world")
        print(f"trace {st['t']}: hero {A} -> auto {st['auto']['label']} {B}")
    else:
        need = 3 if a.cmd == "where" else 6
        if len(a.xyz) != need:
            sys.exit(f"{a.cmd} takes {need} numbers")
        A, B = a.xyz[:3], a.xyz[3:] if need == 6 else None
    pa, how_a = mesh.locate(A)
    print(f"A {[round(v) for v in A]}: poly {pa} ({how_a})"
          + (f", piece {mesh.piece[pa]} of {mesh.sizes[mesh.piece[pa]]} polys" if pa is not None else ""))
    if a.cmd == "where":
        return
    pb, how_b = mesh.locate(B)
    print(f"B {[round(v) for v in B]}: poly {pb} ({how_b})"
          + (f", piece {mesh.piece[pb]} of {mesh.sizes[mesh.piece[pb]]} polys" if pb is not None else ""))
    if pa is None or pb is None:
        print("a point is off the loaded navmesh: the guide snaps a goal to a floor within 3 m across, 8 m "
              "below or 1.5 m above (ROUTES.md §9 floor_under); else the last leg reads as through something")
        return
    ka, kb = mesh.piece[pa], mesh.piece[pb]
    if ka == kb:
        print("connected on the baked mesh: a 'blocked' guide here is not the navmesh's doing")
        return
    lo = (min(A[0], B[0]) - a.margin, min(A[1], B[1]) - a.margin)
    hi = (max(A[0], B[0]) + a.margin, max(A[1], B[1]) + a.margin)
    g = gaps(mesh, lo, hi, a.gap)
    adj = collections.defaultdict(list)
    for (x, y), (dist, at) in g.items():
        adj[x].append((y, dist, at))
        adj[y].append((x, dist, at))
    # Fewest gaps first, then shortest gaps.
    q, prev, seen = [(0, 0.0, ka)], {ka: None}, set()
    while q:
        n, tot, k = heapq.heappop(q)
        if k in seen:
            continue
        seen.add(k)
        if k == kb:
            break
        for k2, dist, at in adj[k]:
            if k2 not in prev:
                prev[k2] = (k, dist, at)
                heapq.heappush(q, (n + 1, tot + dist, k2))
    doors = door_names(a.graph or d / "graph.json", world)
    print(f"not connected on the baked mesh; {len(adj[ka])} gap(s) <= {a.gap / 100:.1f} m lead out of A's piece:")
    for k2, dist, at in sorted(adj[ka], key=lambda x: x[1]):
        print(f"   {dist / 100:.2f} m at {[round(v) for v in at]} -> piece {k2} ({mesh.sizes[k2]} polys): "
              f"{named(at, doors)}")
    if kb not in prev:
        print(f"no chain of gaps <= {a.gap / 100:.1f} m within {a.margin / 100:.0f} m joins them: a lift, a drop, "
              "water, or an unloaded area lies between (try a larger --gap or --margin, or `doctor nav` nearer B)")
        return
    chain, k = [], kb
    while prev[k]:
        k0, dist, at = prev[k]
        chain.append((k0, k, dist, at))
        k = k0
    print(f"the way A -> B crosses {len(chain)} gap(s), each shut in the bake (passable live only if the guide "
          "judges the door passable: used, or nothing more than a press):")
    for k0, k1, dist, at in reversed(chain):
        print(f"   piece {k0} -> {k1}: {dist / 100:.2f} m at {[round(v) for v in at]}: {named(at, doors)}")


if __name__ == "__main__":
    main()
