"""Stage part of a file's changes without interactive git (no `git add -p`).

  list  PATH                 number the hunks of PATH's unstaged diff (index -> working tree)
  stage PATH --hunks 1,3     stage only those hunks: the index's content with them applied
  stage PATH --content FILE  stage FILE's bytes as PATH's content (for hand-made splits)

The staged content is written with `git hash-object -w --stdin --path=PATH` (so .gitattributes
filters apply) and put in the index with `git update-index --cacheinfo MODE,SHA,PATH`, which
leaves the working tree alone. All I/O is bytes: no newline or encoding conversion on Windows.
Run `git diff --cached -- PATH` afterwards to see what is staged.
"""
import argparse
import re
import subprocess
import sys

HUNK = re.compile(rb"^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@")


def git(*args, input=None, check=True):
    r = subprocess.run(["git", *args], input=input, capture_output=True)
    if check and r.returncode:
        sys.exit(f"git {' '.join(args)}: {r.stderr.decode('utf-8', 'replace').strip()}")
    return r.stdout


def hunks(path):
    diff = git("diff", "--no-color", "--no-ext-diff", "-U3", "--", path)
    if b"\nBinary files " in diff or diff.startswith(b"Binary files "):
        sys.exit(f"{path}: binary diff; use --content")
    out, cur = [], None
    for line in diff.splitlines(keepends=True):
        m = HUNK.match(line)
        if m:
            a, b = int(m.group(1)), int(m.group(2) or 1)
            cur = {"old_start": a, "old_len": b, "header": line, "lines": []}
            out.append(cur)
        elif cur is not None and line[:1] in (b" ", b"-", b"+", b"\\"):
            cur["lines"].append(line)
    return out


def index_content(path):
    r = subprocess.run(["git", "show", f":{path}"], capture_output=True)
    return r.stdout if r.returncode == 0 else b""


def apply(base, chosen):
    old = base.splitlines(keepends=True)
    out, pos = [], 0
    for h in chosen:
        start = h["old_start"] - 1 if h["old_len"] > 0 else h["old_start"]
        if start < pos:
            sys.exit("hunks overlap")
        out += old[pos:start]
        pos = start
        last = None
        for line in h["lines"]:
            kind, text = line[:1], line[1:]
            if kind == b"\\":
                if last == b"+" and out and out[-1].endswith(b"\n"):
                    out[-1] = out[-1][:-1]
                continue
            if kind in (b" ", b"-"):
                if pos >= len(old) or old[pos].rstrip(b"\r\n") != text.rstrip(b"\r\n"):
                    sys.exit(f"hunk {h['header'].decode().strip()} does not match the index; is part already staged?")
                if kind == b" ":
                    out.append(old[pos])
                pos += 1
            else:
                out.append(text)
            last = kind
    out += old[pos:]
    return b"".join(out)


def stage(path, content):
    sha = git("hash-object", "-w", "--stdin", f"--path={path}", input=content).strip().decode()
    ls = git("ls-files", "-s", "--", path).split()
    mode = ls[0].decode() if ls else "100644"
    git("update-index", "--add", "--cacheinfo", f"{mode},{sha},{path}")
    print(f"staged {path} ({len(content)} bytes, {sha[:10]})")


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("cmd", choices=["list", "stage"])
    ap.add_argument("path", help="repository-relative, forward slashes")
    ap.add_argument("--hunks", help="comma-separated hunk numbers from `list`")
    ap.add_argument("--content", help="a file whose bytes become the staged content")
    a = ap.parse_args()
    out = sys.stdout.buffer
    if a.cmd == "list":
        hs = hunks(a.path)
        if not hs:
            print("no unstaged changes")
        for n, h in enumerate(hs, 1):
            out.write(f"--- hunk {n}: ".encode() + h["header"])
            for line in h["lines"]:
                out.write(b"    " + line if line.endswith(b"\n") else b"    " + line + b"\n")
        return
    if bool(a.hunks) == bool(a.content):
        sys.exit("stage takes exactly one of --hunks or --content")
    if a.content:
        with open(a.content, "rb") as f:
            stage(a.path, f.read())
        return
    hs = hunks(a.path)
    want = sorted({int(x) for x in a.hunks.split(",")})
    if not want or want[0] < 1 or want[-1] > len(hs):
        sys.exit(f"{a.path} has hunks 1..{len(hs)}")
    stage(a.path, apply(index_content(a.path), [hs[i - 1] for i in want]))


if __name__ == "__main__":
    main()
