"""Why is X not reached, or reached so late? Walks Mods\\doctor\\graph.json backwards.

X is a fact or tag, an item path, or a node (a part of its name or class; --world narrows).
For a fact or item: its givers, earliest round first. For a node: each need with the round it
first holds ("ready"), then, recursively, the givers of what holds last or never: the chain
that sets the node's round. Rounds follow src/guide/graph.rs `reach`: what round r makes
doable counts from r + 1; `used` of an actor not in the graph and {fact, has: false} always
hold; `any` holds with its earliest child, `all` with its latest.

Usage: python why_not.py X [--world W] [--depth 4] [--givers 3] [--file PATH] [--game DIR]
"""
import argparse
import collections
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from hiu_paths import doctor_dir  # noqa: E402

NEVER = float("inf")


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("what", help="fact/tag, item path, or part of a node's name or class")
    ap.add_argument("--world", help="only nodes of this world when matching a node")
    ap.add_argument("--depth", type=int, default=4)
    ap.add_argument("--givers", type=int, default=3, help="givers followed per fact or item")
    ap.add_argument("--file")
    ap.add_argument("--game")
    a = ap.parse_args()
    sys.stdout.reconfigure(encoding="utf-8")
    path = Path(a.file) if a.file else doctor_dir(a.game) / "graph.json"
    if not path.is_file():
        sys.exit(f"no {path}: run `target\\release\\hiumod.exe doctor graph` first")
    nodes = json.load(open(path, encoding="utf-8"))
    by = {(n["world"], n["name"]): i for i, n in enumerate(nodes)}
    givers = collections.defaultdict(list)
    for i, n in enumerate(nodes):
        for x in n["gives_items"] + n["gives_tags"]:
            givers[x].append(i)

    def rnd(i):
        r = nodes[i]["round"]
        return NEVER if r is None else r

    def ready(need, world):
        """The round from which a need holds (NEVER if it does not)."""
        if "all" in need:
            return max((ready(x, world) for x in need["all"]), default=0)
        if "any" in need:
            return min((ready(x, world) for x in need["any"]), default=0)
        if "used" in need:
            j = by.get((world, need["used"]))
            return 0 if j is None else rnd(j) + 1
        if "fact" in need and not need.get("has", True):
            return 0
        key = need.get("fact", need.get("item"))
        return min((rnd(j) for j in givers.get(key, [])), default=NEVER) + 1

    def show_r(r):
        return "never" if r == NEVER else str(r)

    def label(i):
        n = nodes[i]
        return f"{n['world']}:{n['name']} [{n['class']}] round {show_r(rnd(i))}"

    seen = set()

    def leaf_text(need):
        if "used" in need:
            return f"used {need['used']}"
        if "fact" in need:
            return f"{'' if need.get('has', True) else 'NOT '}fact {need['fact']}"
        return f"item {need['item']}"

    def explain_thing(key, depth, pad):
        gs = sorted(givers.get(key, []), key=rnd)
        if not gs:
            print(f"{pad}{key}: given by NOTHING in the graph (a script, a scene, the code? see GRAPH.md §6, §13)")
            return
        print(f"{pad}{key}: {len(gs)} giver(s), earliest round {show_r(rnd(gs[0]))}")
        for j in gs[: a.givers]:
            explain_node(j, depth + 1, pad + "  ")

    def explain_need(need, world, target_round, depth, pad):
        r = ready(need, world)
        crit = r == NEVER or r >= target_round
        mark = "!" if crit else " "
        if "all" in need or "any" in need:
            kind = "all" if "all" in need else "any"
            print(f"{pad}{mark} {kind} (ready {show_r(r)})")
            for x in need[kind]:
                explain_need(x, world, target_round if kind == "all" else r, depth, pad + "    ")
            return
        print(f"{pad}{mark} {leaf_text(need)} (ready {show_r(r)})")
        if not crit or depth >= a.depth:
            return
        if "used" in need:
            j = by.get((world, need["used"]))
            if j is None:
                print(f"{pad}    (not in the graph: reads as met)")
            else:
                explain_node(j, depth + 1, pad + "    ")
        elif need.get("has", True):
            explain_thing(need.get("fact", need.get("item")), depth, pad + "    ")

    def explain_node(i, depth, pad):
        print(f"{pad}{label(i)}")
        if i in seen:
            print(f"{pad}  (shown above)")
            return
        seen.add(i)
        n = nodes[i]
        if n["scripted"]:
            print(f"{pad}  scripted: the story going on (a quest listener or the story's code)")
        if not n["needs"]:
            print(f"{pad}  needs nothing")
        if depth >= a.depth:
            return
        for need in n["needs"]:
            explain_need(need, n["world"], rnd(i), depth, pad + "  ")

    w = a.what
    if w in givers:
        explain_thing(w, 0, "")
        return
    hits = [i for i, n in enumerate(nodes)
            if (w in n["name"] or w in n["class"]) and (not a.world or n["world"] == a.world)]
    if not hits:
        near = sorted(k for k in givers if w.lower() in k.lower())[:10]
        sys.exit(f"nothing named {w!r}" + (f"; facts/items containing it: {near}" if near else ""))
    if len(hits) > 5:
        print(f"{len(hits)} nodes match; the 5 latest (narrow with --world or a longer name):")
        hits = sorted(hits, key=rnd, reverse=True)[:5]
    print("'!' marks the need that holds last or never: the one that sets the round.\n")
    for i in hits:
        explain_node(i, 0, "")
        print()


if __name__ == "__main__":
    main()
