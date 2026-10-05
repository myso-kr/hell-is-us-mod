"""Summarise Mods\\doctor\\guide.jsonl: the last target changes and the guide's last state.

The overlay appends one JSON record a line (src/ui/overlay/trace.rs):
  kind "event": t, what, from, to, why, text?, hero [x,y,z], world
  kind "state": t, world, hero, story, auto_guide, auto {label, id, at, tier, gate, source,
                distance_m, height_m, blocked, held, detail, route{points, ends_short_m,
                through_something}}, nearest[] {label, id, at, tier, gate, source, distance_m,
                height_m, blocked, followed, wanted, skipped, detail}, goals, survey_goals,
                blocked, tracks[]
Positions are Unreal cm (north is -Y). Labels and details are in the game's language.

Usage: python guide_trace.py [--events N] [--nearest N] [--grep TEXT] [--file PATH] [--game DIR]
"""
import argparse
import json
import math
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from hiu_paths import doctor_dir  # noqa: E402


def fmt_at(p):
    return "(" + ", ".join(f"{v:.0f}" for v in p) + ")" if p else "-"


def goal_line(g):
    flags = "".join(
        c for c, on in (("B", g.get("blocked")), ("F", g.get("followed")), ("W", g.get("wanted")),
                        ("S", g.get("skipped")), ("H", g.get("held"))) if on
    )
    return (f"{g.get('distance_m', 0):7.1f} m  dz {g.get('height_m', 0):+6.1f}  [{flags or '-':4}] "
            f"{g.get('tier', '?'):7} {g.get('gate', '?'):11} {g.get('source', '?'):6} {g.get('label', '')}")


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--events", type=int, default=15, help="how many of the last events to print")
    ap.add_argument("--nearest", type=int, default=10, help="how many nearest goals of the last state")
    ap.add_argument("--grep", help="only events whose from/to/why/text contain this")
    ap.add_argument("--file", help="a guide.jsonl elsewhere (default: <game>\\Mods\\doctor\\guide.jsonl)")
    ap.add_argument("--game", help="the game's install dir (default: found through Steam)")
    a = ap.parse_args()

    path = Path(a.file) if a.file else doctor_dir(a.game) / "guide.jsonl"
    if not path.is_file():
        sys.exit(f"no trace at {path}: the overlay writes it while the panel runs with the guide on")
    events, last = [], None
    bad = 0
    with open(path, encoding="utf-8") as f:
        for line in f:
            try:
                r = json.loads(line)
            except json.JSONDecodeError:
                bad += 1  # a line cut off while being written
                continue
            if r.get("kind") == "event":
                events.append(r)
            elif r.get("kind") == "state":
                last = r
    sys.stdout.reconfigure(encoding="utf-8")
    print(f"{path}  ({len(events)} events{f', {bad} unreadable lines' if bad else ''})")

    if a.grep:
        events = [e for e in events if any(a.grep in str(e.get(k, "")) for k in ("from", "to", "why", "text"))]
    print(f"\n== last {min(a.events, len(events))} events")
    for e in (events[-a.events:] if a.events > 0 else []):
        print(f"{e.get('t')} {e.get('world', '')} hero {fmt_at(e.get('hero'))} [{e.get('what')}]")
        print(f"    {e.get('from')}  ->  {e.get('to')}")
        if e.get("why"):
            print(f"    why: {e['why']}")
        if e.get("text"):
            print(f"    text: {e['text']}")

    # Ping-pong: the auto target going A -> B and straight back B -> A.
    autos = [e for e in events if e.get("what") == "auto"]
    flips = [(x, y) for x, y in zip(autos, autos[1:]) if x.get("from") == y.get("to") and x.get("to") == y.get("from")]
    if flips:
        x, y = flips[-1]
        print(f"\nping-pong: {len(flips)} times the target went A -> B -> A; last at {y.get('t')}:")
        print(f"    {x.get('from')}  <->  {x.get('to')}")
        print(f"    why there: {x.get('why')}\n    why back:  {y.get('why')}")

    if not last:
        print("\nno state record yet")
        return
    print(f"\n== last state {last.get('t')}  world {last.get('world')}  hero {fmt_at(last.get('hero'))}")
    print(f"story: {last.get('story')}  auto_guide: {last.get('auto_guide')}  goals {last.get('goals')} "
          f"(survey {last.get('survey_goals')}, blocked {last.get('blocked')})")
    au = last.get("auto")
    if au:
        hero = last.get("hero")
        flat = math.dist(hero[:2], au["at"][:2]) / 100 if hero and au.get("at") else None
        print(f"auto: {au.get('label')}  id {au.get('id')}  at {fmt_at(au.get('at'))}"
              f"{f'  ({flat:.1f} m across)' if flat is not None else ''}")
        print(f"      tier {au.get('tier')} gate {au.get('gate')} source {au.get('source')} "
              f"blocked {au.get('blocked')} held {au.get('held')}")
        if au.get("route"):
            print(f"      route {au['route']}")
        if au.get("detail"):
            print(f"      detail: {au['detail']}")
    else:
        print("auto: none")
    near = last.get("nearest") or []
    print(f"\nnearest {min(a.nearest, len(near))} of {len(near)}  (flags: B blocked, F followed, W wanted, S skipped)")
    for g in near[:a.nearest]:
        print("  " + goal_line(g))
        if g.get("detail"):
            print(f"      {g['detail']}")
    if last.get("tracks"):
        print(f"\ntracks: {last['tracks']}")


if __name__ == "__main__":
    main()
