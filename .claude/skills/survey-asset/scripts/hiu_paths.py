"""Where Hell Is Us is installed, found the way hiumod finds it (src/game/locate.rs).

Order: the HIU_GAME_DIR environment variable; else every Steam library (the registry's
SteamPath, the default Steam folder, and each path its libraryfolders.vdf lists) that has
appmanifest_1620730.acf, joined with that manifest's installdir. Never a hard-coded path.
"""
import os
import re
from pathlib import Path

APP_ID = "1620730"


def _steam_roots():
    roots = []
    try:
        import winreg

        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, r"Software\Valve\Steam") as k:
            roots.append(Path(winreg.QueryValueEx(k, "SteamPath")[0]))
    except OSError:
        pass
    pf86 = os.environ.get("ProgramFiles(x86)")
    if pf86:
        roots.append(Path(pf86) / "Steam")
    return roots


def _libraries():
    out = []
    for s in _steam_roots():
        vdf = s / "steamapps" / "libraryfolders.vdf"
        if vdf.is_file():
            text = vdf.read_text(encoding="utf-8", errors="replace")
            out += [Path(p.replace("\\\\", "\\")) for p in re.findall(r'"path"\s+"([^"]+)"', text)]
        out.append(s)
    seen, uniq = set(), []
    for p in out:
        k = str(p).lower().rstrip("\\/")
        if k not in seen:
            seen.add(k)
            uniq.append(p)
    return uniq


def game_dir(override=None):
    """The install folder (the one holding HellIsUs.exe), or raise SystemExit."""
    for cand in (override, os.environ.get("HIU_GAME_DIR")):
        if cand:
            return Path(cand)
    for lib in _libraries():
        acf = lib / "steamapps" / f"appmanifest_{APP_ID}.acf"
        if acf.is_file():
            m = re.search(r'"installdir"\s+"([^"]+)"', acf.read_text(encoding="utf-8", errors="replace"))
            if m:
                d = lib / "steamapps" / "common" / m.group(1)
                if d.is_dir():
                    return d
    raise SystemExit("Hell Is Us not found in any Steam library; pass --game <install dir> or set HIU_GAME_DIR")


def doctor_dir(override=None):
    """<install>\\Mods\\doctor, where hiumod's doctor commands write."""
    return game_dir(override) / "Mods" / "doctor"
