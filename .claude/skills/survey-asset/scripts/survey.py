"""Run tools/survey (C# + CUE4Parse) against the installed game with the game dir filled in.

Everything after the script's own options goes to the tool unchanged:
  python survey.py --ls "Gameplay/QuestListeners/.*Jova"
  python survey.py --dump "HellIsUs/Content/<path>.uasset"      (exports as JSON)
  python survey.py --grep "NameA|NameB" --in "HellIsUs/Content/Maps/"
  python survey.py --peek ForgeMachineLever --world LakeCynon    (actor + sub-objects by path part)
  python survey.py --refs <name> --world LakeCynon               (exports whose values mention it)
  python survey.py --world LakeCynon --out "%TEMP%\\survey"      (a survey to a scratch folder)
  python survey.py --tables | --locale                           (writes Mods\\survey / Mods\\locale)
Own options: --save FILE (stdout to a file, UTF-8), --game DIR, --dll PATH.

Without --world, --peek and --refs walk all 11 worlds (minutes). A run without a mode surveys
and WRITES <install>\\Mods\\survey (the panel's data) unless --out is given. Needs the .NET 8
runtime, Mods\\doctor\\HellIsUs.usmap (`hiumod doctor usmap`, game running) and the built tool
(`dotnet build -c Release` in tools/survey). The AES key is found in the game's exe at each run
and never written anywhere. Output is game data: never commit it.
"""
import os
import shutil
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from hiu_paths import game_dir  # noqa: E402


def main():
    argv = sys.argv[1:]
    if not argv or argv[0] in ("-h", "--help"):
        print(__doc__)
        return
    own = {"--save": None, "--game": None, "--dll": None}
    rest = []
    i = 0
    while i < len(argv):
        if argv[i] in own and i + 1 < len(argv):
            own[argv[i]] = argv[i + 1]
            i += 2
        else:
            rest.append(argv[i])
            i += 1
    root = Path(subprocess.run(["git", "-C", str(Path(__file__).parent), "rev-parse", "--show-toplevel"],
                               capture_output=True, text=True).stdout.strip() or ".")
    dll = Path(own["--dll"]) if own["--dll"] else root / "tools/survey/bin/Release/net8.0/survey.dll"
    if not dll.is_file():
        sys.exit(f"no {dll}: run `dotnet build -c Release` in tools/survey (.NET 8 SDK)")
    game = game_dir(own["--game"])
    usmap = game / "Mods" / "doctor" / "HellIsUs.usmap"
    if not usmap.is_file():
        sys.exit(f"no {usmap}: run `hiumod doctor usmap` with the game running")
    modes = {"--ls", "--dump", "--grep", "--peek", "--refs", "--tables", "--locale"}
    if not modes.intersection(rest) and "--out" not in rest:
        print(f"note: a full survey run writes {game / 'Mods' / 'survey'} (pass --out to write elsewhere)",
              file=sys.stderr)
    dotnet = shutil.which("dotnet") or str(Path(os.environ.get("ProgramFiles", r"C:\Program Files")) / "dotnet" / "dotnet.exe")
    cmd = [dotnet, str(dll), "--game", str(game), *rest]
    print("running:", " ".join(f'"{c}"' if " " in c else c for c in cmd), file=sys.stderr)
    if own["--save"]:
        with open(own["--save"], "wb") as f:
            sys.exit(subprocess.run(cmd, stdout=f).returncode)
    sys.exit(subprocess.run(cmd).returncode)


if __name__ == "__main__":
    main()
