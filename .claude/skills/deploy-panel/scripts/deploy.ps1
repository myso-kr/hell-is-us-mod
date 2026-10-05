<#
.SYNOPSIS
  Build hiumod into target\next, close the running panel through its own × (never killed),
  copy the build over target\release\hiumod.exe and start it detached.

.DESCRIPTION
  Steps, each stopping the script on failure:
    1. cargo build --release --target-dir target/next   (skipped with -NoBuild)
    2. if a panel runs: press its " × " through UI Automation (panel.ps1), which puts the game's
       original values back, then wait up to -WaitSec for the process to end.
       A panel with no window (hidden in the tray) is NOT closed: exit 3, ask the user.
       A panel still running after the wait: exit 3, ask the user. Never Stop-Process.
    3. copy target\next\release\hiumod.exe over target\release\hiumod.exe
    4. Start-Process target\release\hiumod.exe (detached; never a shell's background task)
  -NoStart stops after the copy.

  Exit codes: 0 done, 1 build failed, 3 panel could not be closed (ask the user), 4 copy failed.
#>
param(
    [switch]$NoBuild,
    [switch]$NoStart,
    [int]$WaitSec = 20
)
$ErrorActionPreference = "Stop"
$repo = (& git -C $PSScriptRoot rev-parse --show-toplevel).Trim()
$next = Join-Path $repo "target\next\release\hiumod.exe"
$live = Join-Path $repo "target\release\hiumod.exe"

if (-not $NoBuild) {
    Push-Location $repo
    try {
        & cargo build --release --target-dir target/next
        if ($LASTEXITCODE -ne 0) { "build failed"; exit 1 }
    } finally { Pop-Location }
}
if (-not (Test-Path $next)) { "no build at $next"; exit 1 }

$running = Get-Process -Name hiumod -ErrorAction SilentlyContinue
if ($running) {
    $withWindow = $running | Where-Object { $_.MainWindowHandle -ne 0 }
    if (-not $withWindow) {
        "the panel runs without a window (hidden in the tray): not closing it. Ask the user to show it or close it."
        exit 3
    }
    & (Join-Path $PSScriptRoot "panel.ps1") -Close -WaitMs 200
    if ($LASTEXITCODE -ne 0) { "could not press the panel's ×: ask the user to close it"; exit 3 }
    $deadline = (Get-Date).AddSeconds($WaitSec)
    while ((Get-Process -Name hiumod -ErrorAction SilentlyContinue) -and (Get-Date) -lt $deadline) {
        Start-Sleep -Milliseconds 500
    }
    if (Get-Process -Name hiumod -ErrorAction SilentlyContinue) {
        "the panel is still running after ${WaitSec}s (a confirmation, or hidden?): ask the user. Not killing it."
        exit 3
    }
    "panel closed"
}

try { Copy-Item -Force $next $live } catch { "copy failed: $_"; exit 4 }
"copied $next -> $live"
if ($NoStart) { exit 0 }
Start-Process -FilePath $live -WorkingDirectory (Split-Path -Parent $live)
"started $live"
