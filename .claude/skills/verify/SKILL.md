---
name: verify
description: Runs hiumod's pre-commit checks the way CI does - cargo fmt, cargo clippy with -D warnings over all targets, and the tests - in target/next so the running panel's exe is untouched, plus the site rebuild check when tools/site changed. Use before every commit or deploy, after editing Rust code, or when the user asks to check, lint, test or "make CI pass".
---

# Verify

CI (`.github/workflows/ci.yml`, Windows) runs `cargo fmt --check`,
`cargo clippy --all-targets --locked -- -D warnings` and `cargo test --locked`; the `hygiene` job
fails on any committed game file or `Mods/` path; the `site` job rebuilds `docs/` from `tools/site`
and fails on a difference. CI uses the newest stable Rust, which can bring lints the local toolchain
does not have yet.

Build in `target/next`: the panel runs from `target\release\hiumod.exe`, and building the tests there
would try to relink it while it is running.

## Steps

From the repository root:

```
powershell -NoProfile -ExecutionPolicy Bypass -File .claude/skills/verify/scripts/verify.ps1
```

which is

```
cargo fmt
cargo clippy --release --target-dir target/next --all-targets --locked -- -D warnings
cargo test --release --target-dir target/next --locked
```

- `-Check` uses `cargo fmt --check` (changes nothing; what CI runs). `-SkipTests` for a quick lint.
- `-Site` also runs `python tools/site/build.py` and `git diff --exit-code docs`: use it when
  `tools/site` changed. Edit `tools/site`, never `docs/` by hand.
- Tests that read a real install skip without one; i18n tests fail on a key missing from any of the
  12 tables (use the i18n-keys skill).
- Fix warnings rather than `#[allow]` them, unless the code around already does so for that lint.

Report what ran and what failed; a clean run is the condition for the curated-commit and
deploy-panel skills.
