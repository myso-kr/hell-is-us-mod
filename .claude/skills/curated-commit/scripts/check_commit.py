"""Check what is staged, and optionally a commit message, against the repository's rules.

  python check_commit.py                 the staged files and added lines
  python check_commit.py --message FILE  also the message in FILE (UTF-8)

Errors (exit 1): a game file or binary staged (.exe .dll .pdb .pak .ucas .utoc .uasset .uexp
.sav .Cache .Headers .acf), anything under Mods/, an added line holding this machine's user
profile path or user name, a drive path into a user's folder, or a 32-byte hex string (the
AES key's shape); a subject over 72 characters, no blank line after it, or a last line other
than the Co-Authored-By trailer. Warnings: Hangul outside the Korean tables and help (game
text or Korean prose in code/docs), and a git author other than the project's.
"""
import argparse
import os
import re
import subprocess
import sys

TRAILER = "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
AUTHOR = ("Choi Won", "help@myso.kr")
BAD_EXT = (".exe", ".dll", ".pdb", ".pak", ".ucas", ".utoc", ".uasset", ".uexp", ".sav", ".cache", ".headers", ".acf")
HANGUL = re.compile(r"[가-힣ᄀ-ᇿ㄰-㆏]")
KOREAN_OK = ("assets/i18n/ko.tsv", "assets/i18n/help/ko.txt")
HEX_KEY = re.compile(r"\b(0x)?[0-9A-Fa-f]{64}\b")
USER_PATH = re.compile(r"[A-Za-z]:[\\/]+Users[\\/]+(?!Public\b|Default\b)[^\\/\s\"'`<>%$]+", re.I)


def git(*args):
    return subprocess.run(["git", *args], capture_output=True).stdout.decode("utf-8", "replace")


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--message", help="a file holding the commit message")
    a = ap.parse_args()
    sys.stdout.reconfigure(encoding="utf-8")
    errors, warns = [], []

    files = [f for f in git("diff", "--cached", "--name-only", "--diff-filter=ACMR").splitlines() if f]
    for f in files:
        low = f.lower()
        if low.endswith(BAD_EXT):
            errors.append(f"{f}: a game file or binary")
        if low.startswith("mods/") or "/mods/" in low:
            errors.append(f"{f}: a player's Mods data folder")

    me = [s for s in (os.environ.get("USERNAME"), os.environ.get("USERPROFILE")) if s and len(s) > 2]
    cur = None
    for line in git("diff", "--cached", "-U0", "--no-color").splitlines():
        if line.startswith("+++ "):
            cur = line[6:] if line.startswith("+++ b/") else None
            continue
        if not line.startswith("+") or cur is None:
            continue
        text = line[1:]
        for s in me:
            if s.lower() in text.lower():
                errors.append(f"{cur}: holds this machine's user name or profile path: {text.strip()[:100]}")
        if USER_PATH.search(text):
            errors.append(f"{cur}: a path into a user's folder (write %USERPROFILE%): {text.strip()[:100]}")
        if HEX_KEY.search(text):
            errors.append(f"{cur}: a 64-hex-digit string (an AES key?): {text.strip()[:60]}")
        if HANGUL.search(text) and cur not in KOREAN_OK:
            warns.append(f"{cur}: Hangul (game text? docs and code are English): {text.strip()[:80]}")

    name, email = git("config", "user.name").strip(), git("config", "user.email").strip()
    if (name, email) != AUTHOR:
        warns.append(f"git author is {name} <{email}>, the project's is {AUTHOR[0]} <{AUTHOR[1]}>")

    if a.message:
        msg = open(a.message, encoding="utf-8").read().rstrip("\n").split("\n")
        if len(msg[0]) > 72:
            errors.append(f"subject is {len(msg[0])} characters (at most 72)")
        if msg[0].endswith("."):
            warns.append("subject ends with a period")
        if len(msg) > 1 and msg[1].strip():
            errors.append("no blank line after the subject")
        if len(msg) < 3:
            warns.append("no body: say why")
        if msg[-1] != TRAILER:
            errors.append(f"the last line must be: {TRAILER}")
        if any(HANGUL.search(m) for m in msg):
            errors.append("the message must be English")

    errors, warns = list(dict.fromkeys(errors)), list(dict.fromkeys(warns))
    print(f"{len(files)} staged file(s)")
    for w in warns:
        print("warning:", w)
    for e in errors:
        print("ERROR:", e)
    print("ok" if not errors else "NOT ok")
    sys.exit(1 if errors else 0)


if __name__ == "__main__":
    main()
