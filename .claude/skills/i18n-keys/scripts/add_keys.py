"""Add UI text keys to all 12 tables in assets/i18n/*.tsv, or check the tables.

Input JSON: {"NEW_KEY": {"en": "...", "ko": "...", "de": "...", ...}, ...} with every culture
of CULTURES for every key. Text is written as the tables keep it (src/i18n/text.rs): one
`KEY<TAB>text` line, real newlines/tabs/backslashes escaped as \\n, \\t, \\\\; LF endings,
appended at the end. Refused (nothing written): a key not UPPER_SNAKE, already in a table, a
culture missing, or a translation that drops or adds an English {placeholder} or {g:...} token.

Usage:
  python add_keys.py keys.json [--dry-run]
  python add_keys.py --check            (same key set and line count in every table)
"""
import argparse
import json
import re
import subprocess
import sys
from pathlib import Path

CULTURES = ["en", "de", "es", "fr", "it", "ja", "ko", "pl", "pt-BR", "ru", "tr", "zh-Hans"]
KEY = re.compile(r"^[A-Z][A-Z0-9]*(_[A-Z0-9]+)*$")
TOKEN = re.compile(r"\{[^{}]*\}")


def repo_root():
    here = Path(__file__).resolve().parent
    out = subprocess.run(["git", "-C", str(here), "rev-parse", "--show-toplevel"], capture_output=True, text=True)
    if out.returncode:
        sys.exit("not inside the repository")
    return Path(out.stdout.strip())


def read_table(p):
    raw = p.read_bytes()
    if b"\r\n" in raw:
        sys.exit(f"{p.name} has CRLF line endings: the tables are LF only")
    keys = {}
    for line in raw.decode("utf-8").split("\n"):
        if line and not line.startswith("#"):
            k, _, v = line.partition("\t")
            keys[k] = v
    return raw, keys


def escape(s):
    return s.replace("\\", "\\\\").replace("\n", "\\n").replace("\t", "\\t")


def tokens(s):
    """{g:ns/key} whole; {name} and {name:spec} by name (a translation may change the spec)."""
    return sorted(t if t.startswith("{g:") else t.split(":")[0].rstrip("}") for t in TOKEN.findall(s))


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("json", nargs="?")
    ap.add_argument("--dry-run", action="store_true")
    ap.add_argument("--check", action="store_true")
    a = ap.parse_args()
    sys.stdout.reconfigure(encoding="utf-8")
    d = repo_root() / "assets" / "i18n"
    tables = {c: read_table(d / f"{c}.tsv") for c in CULTURES}

    if a.check or not a.json:
        lines = {c: raw.count(b"\n") for c, (raw, _) in tables.items()}
        en = set(tables["en"][1])
        ok = True
        for c, (raw, keys) in tables.items():
            missing, extra = en - set(keys), set(keys) - en
            if missing or extra or not raw.endswith(b"\n"):
                ok = False
                print(f"{c}: missing {sorted(missing)[:10]} extra {sorted(extra)[:10]} "
                      f"ends with LF: {raw.endswith(b'\n')}")
        if len(set(lines.values())) != 1:
            ok = False
            print("line counts differ:", lines)
        print(f"{'ok' if ok else 'NOT ok'}: {len(CULTURES)} tables, {len(en)} keys, {lines['en']} lines each")
        sys.exit(0 if ok else 1)

    new = json.load(open(a.json, encoding="utf-8"))
    errors = []
    for k, texts in new.items():
        if not KEY.match(k):
            errors.append(f"{k}: not UPPER_SNAKE")
        for c in CULTURES:
            if c not in texts or not texts[c].strip():
                errors.append(f"{k}: no {c} text")
            elif k in tables[c][1]:
                errors.append(f"{k}: already in {c}.tsv")
        if "en" in texts:
            want = tokens(texts["en"])
            for c, t in texts.items():
                if c not in CULTURES:
                    errors.append(f"{k}: unknown culture {c}")
                elif tokens(t) != want:
                    errors.append(f"{k}: {c} tokens {tokens(t)} differ from English {want}")
    if errors:
        print("\n".join(errors))
        sys.exit("nothing written")
    for c in CULTURES:
        raw, _ = tables[c]
        add = "".join(f"{k}\t{escape(new[k][c])}\n" for k in new)
        out = raw + (b"" if raw.endswith(b"\n") else b"\n") + add.encode("utf-8")
        if a.dry_run:
            print(f"{c}.tsv +{len(new)}: " + add.splitlines()[0])
        else:
            (d / f"{c}.tsv").write_bytes(out)
    if not a.dry_run:
        counts = {c: (d / f"{c}.tsv").read_bytes().count(b"\n") for c in CULTURES}
        same = len(set(counts.values())) == 1
        print(f"added {len(new)} key(s) to {len(CULTURES)} tables; line counts {'equal' if same else 'DIFFER'}: "
              f"{counts['en']}")
        sys.exit(0 if same else 1)


if __name__ == "__main__":
    main()
