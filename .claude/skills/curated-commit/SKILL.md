---
name: curated-commit
description: Makes hiumod's curated git commits - split by concern, partial files staged without interactive git (hash-object + update-index), English messages with a 72-character subject, a body that says why and the Co-Authored-By trailer, CHANGELOG and .spec kept current, and nothing from the game, no AES key, local paths or user names. Use when the user asks to commit, split changes into commits, or tidy the history; never push unless asked.
---

# Curated commits

The history of this public repository (`myso-kr/hell-is-us-mod`) is kept as meaningful commits:
no "fix typo", "confirmed in game" or fix-up noise (fold those into the commit they fix before
anything is pushed). `.spec/RUNBOOK.md` "Committing" is the rule.

## Steps

1. Run the verify skill; it must pass (CI runs fmt --check and clippy -D warnings).
2. Keep the docs current in the same commit as the change they describe: `CHANGELOG.md` under
   `## Unreleased` (Added / Changed / Fixed, user-facing words), and the `.spec/*.md` topic file
   (GRAPH, ROUTES, SURVEY, GUIDE, PANEL, I18N, ...) with what was measured and decided.
3. Group the changes by concern (one reason per commit) with `git status` / `git diff`. Stage whole
   files with `git add <path>`; split a file between commits without `git add -p`:
   ```
   python .claude/skills/curated-commit/scripts/stage_hunks.py list src/guide/graph.rs
   python .claude/skills/curated-commit/scripts/stage_hunks.py stage src/guide/graph.rs --hunks 1,3
   python .claude/skills/curated-commit/scripts/stage_hunks.py stage src/x.rs --content "$env:TEMP\x.rs.part"
   ```
   It writes the chosen content with `git hash-object -w --stdin --path=<path>` and stages it with
   `git update-index --cacheinfo`, bytes only, the working tree untouched. Review every commit's
   `git diff --cached` before committing: each commit should build and read on its own.
4. Write the message to a file (UTF-8, LF) and check it with what is staged:
   ```
   python .claude/skills/curated-commit/scripts/check_commit.py --message "$env:TEMP\msg.txt"
   git commit -F "$env:TEMP\msg.txt"
   ```
   - English; subject at most 72 characters, imperative, no period; blank line; a body wrapped near
     72 that explains why (the problem seen, what was measured), not a list of edits.
   - Last line exactly: `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`
   - Author `Choi Won <help@myso.kr>` (the repository's git config; do not change it).
5. Never push unless the user asks. Before a push of docs, render changed Markdown as GFM
   (`gh api markdown`) when it has tables, `<kbd>` or backticks in HTML.

## Never commit

Game files or binaries (.exe .dll .pak .ucas .utoc .uasset .uexp .sav ...), the AES key, game text
(item, place or character names and lines from `Mods\locale`), extracted data (`Mods\survey`,
`Mods\locale`, `Mods\cache`, `Mods\doctor`, navtiles, graph.json, captures), local paths or user
names (write `%USERPROFILE%\…`). `.gitignore` and CI's hygiene job guard some of it; `git add -f`
walks past both, so `check_commit.py` checks the staged lines too.

Tell the user (in Korean) which commits were made, with their subjects.
