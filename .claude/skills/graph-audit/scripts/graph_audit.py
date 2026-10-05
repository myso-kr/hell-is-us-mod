"""Audit the requirement graph that `hiumod doctor graph` writes (Mods\\doctor\\graph.json).

Each node: world, name, class, at [x,y,z], guid (has a save GUID), scripted, needs (a list,
all of which must hold; each a tree of {used: actor} | {fact: tag, has: bool} | {item: path}
| {all: [...]} | {any: [...]}), gives_items, gives_tags (facts and tags), activators, logic,
round (null = not reachable from nothing known). Same rules as src/guide/graph.rs `reach`:
a `used` naming an actor not in the graph reads as met; {fact, has: false} always holds.

Checks (GRAPH.md §11): dangling references, untracked links (target without a save GUID),
cycles over "used" edges, order inversions, needs given by nothing, not-yet-known windows,
isolated nodes, unreachable nodes.

Usage: python graph_audit.py [--show N] [--world W] [--json OUT] [--file PATH] [--game DIR]
"""
import argparse
import collections
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from hiu_paths import doctor_dir  # noqa: E402


def leaves(need, under="all"):
    """(kind, value, has, under) for each leaf; under is 'any' below an any."""
    if "all" in need:
        for x in need["all"]:
            yield from leaves(x, under)
    elif "any" in need:
        for x in need["any"]:
            yield from leaves(x, "any")
    elif "used" in need:
        yield "used", need["used"], True, under
    elif "fact" in need:
        yield "fact", need["fact"], need.get("has", True), under
    elif "item" in need:
        yield "item", need["item"], True, under


def load(args):
    path = Path(args.file) if args.file else doctor_dir(args.game) / "graph.json"
    if not path.is_file():
        sys.exit(f"no {path}: run `target\\release\\hiumod.exe doctor graph` first (needs Mods\\survey)")
    return path, json.load(open(path, encoding="utf-8"))


def label(n):
    r = n["round"]
    return f"{n['world']}:{n['name']} [{n['class']}] r{'-' if r is None else r}"


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--show", type=int, default=5, help="examples printed per check")
    ap.add_argument("--world", help="only report nodes of this world (the checks still use all)")
    ap.add_argument("--json", help="write every finding to this file (keep it out of the repository)")
    ap.add_argument("--file", help="a graph.json elsewhere")
    ap.add_argument("--game", help="the game's install dir (default: found through Steam)")
    a = ap.parse_args()
    sys.stdout.reconfigure(encoding="utf-8")
    path, nodes = load(a)
    by = {(n["world"], n["name"]): i for i, n in enumerate(nodes)}
    give_item, give_tag = collections.defaultdict(list), collections.defaultdict(list)
    for i, n in enumerate(nodes):
        for x in n["gives_items"]:
            give_item[x].append(i)
        for x in n["gives_tags"]:
            give_tag[x].append(i)

    def rnd(i):
        r = nodes[i]["round"]
        return 10**9 if r is None else r

    found = collections.defaultdict(list)
    used_adj = collections.defaultdict(set)
    degree = collections.Counter()
    for i, n in enumerate(nodes):
        r = n["round"]
        for need in n["needs"]:
            for kind, v, has, under in leaves(need):
                if kind == "used":
                    j = by.get((n["world"], v))
                    if j is None:
                        found["dangling"].append({"node": i, "actor": v})
                        continue
                    degree[i] += 1
                    degree[j] += 1
                    used_adj[j].add(i)
                    if not nodes[j]["guid"]:
                        found["untracked"].append({"node": i, "target": j})
                    if r is not None and under == "all" and rnd(j) >= r:
                        found["order"].append({"node": i, "needs": j, "kind": "used"})
                    continue
                givers = (give_item if kind == "item" else give_tag).get(v, [])
                for j in givers:
                    degree[i] += 1
                    degree[j] += 1
                if kind == "fact" and not has:
                    if givers:
                        found["window"].append({"node": i, "fact": v, "given_by": sorted(givers, key=rnd)[:3]})
                    continue
                if not givers:
                    found["no_giver"].append({"node": i, kind: v})
                    continue
                if r is not None and under == "all" and min(rnd(j) for j in givers) >= r:
                    found["order"].append({"node": i, "needs": min(givers, key=rnd), "kind": kind, "what": v})

    # Cycles over "used" (iterative Tarjan).
    index, low, on, stack, sccs, counter = {}, {}, set(), [], [], [0]
    for root in list(used_adj):
        if root in index:
            continue
        work = [(root, iter(sorted(used_adj[root])))]
        index[root] = low[root] = counter[0]
        counter[0] += 1
        stack.append(root)
        on.add(root)
        while work:
            v, it = work[-1]
            for w in it:
                if w not in index:
                    index[w] = low[w] = counter[0]
                    counter[0] += 1
                    stack.append(w)
                    on.add(w)
                    work.append((w, iter(sorted(used_adj[w]))))
                    break
                if w in on:
                    low[v] = min(low[v], index[w])
            else:
                work.pop()
                if work:
                    low[work[-1][0]] = min(low[work[-1][0]], low[v])
                if low[v] == index[v]:
                    comp = []
                    while True:
                        w = stack.pop()
                        on.discard(w)
                        comp.append(w)
                        if w == v:
                            break
                    if len(comp) > 1:
                        sccs.append(comp)
    found["cycle"] = [{"nodes": c} for c in sccs]
    found["isolated"] = [{"node": i} for i, n in enumerate(nodes)
                         if degree[i] == 0 and not n["gives_items"] and not n["gives_tags"]]
    found["unreachable"] = [{"node": i} for i, n in enumerate(nodes) if n["round"] is None]

    def in_world(f):
        if not a.world:
            return True
        ids = f.get("nodes") or [f["node"]]
        return any(nodes[k]["world"] == a.world for k in ids)

    print(f"{path}: {len(nodes)} nodes, {sum(1 for n in nodes if n['round'] is not None)} reachable, "
          f"max round {max((n['round'] for n in nodes if n['round'] is not None), default=0)}")
    meaning = {
        "dangling": "a need names an actor not in the graph (reads as met)",
        "untracked": "the actor needed has no save GUID (always met live: holds nothing back)",
        "cycle": "nodes that need each other over 'used'",
        "order": "needs what comes no earlier than its own round",
        "no_giver": "a fact or item needed that no node gives",
        "window": "a fact that must NOT be known yet, but something gives it (the window can close)",
        "isolated": "no edge in or out and gives nothing",
        "unreachable": "round null: not doable from nothing known",
    }
    for k in meaning:
        rows = [f for f in found[k] if in_world(f)]
        print(f"\n== {k}: {len(rows)}  ({meaning[k]})")
        if k == "isolated":
            print("   by class:", collections.Counter(nodes[f["node"]]["class"] for f in rows).most_common(8))
        for f in rows[: a.show]:
            if k == "cycle":
                print("   " + " -> ".join(label(nodes[x]) for x in f["nodes"][:6]))
            elif k == "order":
                print(f"   {label(nodes[f['node']])}\n      needs {label(nodes[f['needs']])}"
                      f"{' via ' + f['what'] if f.get('what') else ''}")
            elif k == "untracked":
                print(f"   {label(nodes[f['node']])} <- {label(nodes[f['target']])}")
            elif k == "window":
                print(f"   {label(nodes[f['node']])} needs NOT {f['fact']}; given by "
                      + ", ".join(label(nodes[x]) for x in f["given_by"]))
            else:
                extra = {x: y for x, y in f.items() if x != "node"}
                print(f"   {label(nodes[f['node']])} {extra if extra else ''}")
    if a.json:
        Path(a.json).write_text(json.dumps(found, ensure_ascii=False, indent=1), encoding="utf-8")
        print(f"\nfindings written to {a.json}")


if __name__ == "__main__":
    main()
