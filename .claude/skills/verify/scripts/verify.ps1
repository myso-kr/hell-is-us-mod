<#
.SYNOPSIS
  Format, lint and test hiumod the way CI does, in target\next so a running panel's
  target\release\hiumod.exe is never locked or replaced.

.DESCRIPTION
  1. cargo fmt            (-Check: cargo fmt --check, as CI; changes nothing)
  2. cargo clippy --release --target-dir target/next --all-targets --locked -- -D warnings
  3. cargo test   --release --target-dir target/next --locked
  4. -Site: python tools/site/build.py, then git diff --exit-code docs (CI's site job)
  Stops at the first failure with its exit code. -SkipTests skips step 3.
#>
param([switch]$Check, [switch]$SkipTests, [switch]$Site)
$ErrorActionPreference = "Stop"
$repo = (& git -C $PSScriptRoot rev-parse --show-toplevel).Trim()
Push-Location $repo
try {
    function Run([string]$what, [scriptblock]$cmd) {
        "== $what"
        & $cmd
        if ($LASTEXITCODE -ne 0) { "FAILED: $what"; exit $LASTEXITCODE }
    }
    if ($Check) { Run "cargo fmt --check" { cargo fmt --check } } else { Run "cargo fmt" { cargo fmt } }
    Run "clippy" { cargo clippy --release --target-dir target/next --all-targets --locked -- -D warnings }
    if (-not $SkipTests) { Run "tests" { cargo test --release --target-dir target/next --locked } }
    if ($Site) {
        Run "site build" { python tools/site/build.py }
        Run "docs/ matches tools/site" { git diff --exit-code --stat docs }
    }
    "all passed"
} finally { Pop-Location }
