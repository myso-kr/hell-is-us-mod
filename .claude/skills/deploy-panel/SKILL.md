---
name: deploy-panel
description: Rebuilds hiumod and swaps the running panel for the new build safely - build into target/next, close the panel through its own × via UI Automation (never killed, so cheats are restored), copy over target/release/hiumod.exe and start it detached with Start-Process. Also presses panel controls and captures the panel or game window without touching the cursor or focus. Use when the user asks to deploy, redeploy, restart or relaunch the panel, apply a build, or check/screenshot the panel or game screen.
---

# Deploy the panel

The panel (`target\release\hiumod.exe`) is usually running with the game, possibly with cheats on.
Closing it any way other than its own × skips putting the game's original values back; losing god
mode mid-fight is not acceptable (`.spec/RUNBOOK.md`, "Replacing a running panel").

## Hard rules

- Never `Stop-Process`/`taskkill` the panel. Never move the user's cursor or take the focus: press
  controls through UI Automation by name, capture with PrintWindow (panel) or CopyFromScreen (game).
- Start the panel with PowerShell `Start-Process`, never as a shell background task (a time limit
  kills it with cheats on).
- If the panel runs without a window (hidden in the tray) or does not exit after ×, stop and ask
  the user. If the user may be in a fight, ask before closing: × turns cheats off until the panel
  is back (they return only if "keep these cheats on next time" is set).

## Steps

1. Check first (fmt, clippy, tests: the verify skill), then from the repository root:
   ```
   powershell -NoProfile -ExecutionPolicy Bypass -File .claude/skills/deploy-panel/scripts/deploy.ps1
   ```
   It runs `cargo build --release --target-dir target/next`, presses the panel's `" × "`, waits up to
   20 s for the process to go, copies `target\next\release\hiumod.exe` over
   `target\release\hiumod.exe` and starts it. `-NoBuild` skips the build, `-NoStart` stops after the
   copy. Exit 3 means the panel could not be closed: ask the user, do not force it.
2. Confirm it came back and looks right:
   ```
   powershell -NoProfile -File .claude/skills/deploy-panel/scripts/panel.ps1 -Shot "$env:TEMP\hiumod\panel.png"
   powershell -NoProfile -File .claude/skills/deploy-panel/scripts/panel.ps1 -Press "<page or button name>" -Shot "$env:TEMP\hiumod\page.png"
   powershell -NoProfile -File .claude/skills/deploy-panel/scripts/panel.ps1 -Game "$env:TEMP\hiumod\game.png"
   ```
   Then look at the PNGs with Read. Control names are the panel's labels in the game's language
   (Korean on this machine, e.g. "퀘스트"); `-Find` only checks that a control exists; `-Close` is the
   × button. Exit codes: 1 no panel window, 2 control not found, 3 no usable pattern, 4 no game window.
3. Tell the user (in Korean) what changed and that the new build is running. Captures are local
   only: never commit them.
