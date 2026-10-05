<#
.SYNOPSIS
  Press a control of the hiumod panel by name and/or capture the panel or the game, without
  moving the cursor or taking the focus.

.DESCRIPTION
  -Press  finds the control by its accessible name in the panel's UI Automation tree (eframe's
          AccessKit) and invokes it (Invoke, else SelectionItem.Select, else Toggle).
          The close button is named " × " (a space each side).
  -Close  presses the close button (" × ", written as [char]0xD7 so any PowerShell reads it).
  -Find   only says whether the named control (-Press, or the close button with -Close) exists.
  -Shot   captures the panel window with PrintWindow (its own rendering: windows over it do not
          show and it need not be in front).
  -Game   captures the game window's screen area with CopyFromScreen (a DirectX game renders black
          through PrintWindow; this shows whatever is on screen there, the overlay included).

  Exit codes: 0 ok, 1 no panel window (not running, or hidden in the tray), 2 control not found,
  3 control has no usable pattern, 4 no game window.

.EXAMPLE
  powershell -NoProfile -File panel.ps1 -Shot "$env:TEMP\hiumod\panel.png"
  powershell -NoProfile -File panel.ps1 -Press "퀘스트" -Shot "$env:TEMP\hiumod\quests.png"
  powershell -NoProfile -File panel.ps1 -Game "$env:TEMP\hiumod\game.png"
  powershell -NoProfile -File panel.ps1 -Close -Find
  powershell -NoProfile -File panel.ps1 -Close
#>
param(
    [string]$Press = "",
    [string]$Shot = "",
    [string]$Game = "",
    [switch]$Close,
    [switch]$Find,
    [int]$WaitMs = 1000
)
if ($Close) { $Press = " " + [char]0xD7 + " " }
$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type @"
using System; using System.Runtime.InteropServices;
public class HiuWin {
  [DllImport("dwmapi.dll")] public static extern int DwmGetWindowAttribute(IntPtr h, int a, out Rect r, int s);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr dc, uint f);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  public struct Rect { public int L, T, R, B; }
}
"@
[void][HiuWin]::SetProcessDPIAware()

function Get-Bounds([IntPtr]$h) {
    $r = New-Object HiuWin+Rect
    [void][HiuWin]::DwmGetWindowAttribute($h, 9, [ref]$r, 16)   # DWMWA_EXTENDED_FRAME_BOUNDS
    return $r
}

function Save-Png([System.Drawing.Bitmap]$b, [string]$path) {
    $dir = Split-Path -Parent $path
    if ($dir -and -not (Test-Path $dir)) { New-Item -ItemType Directory -Force $dir | Out-Null }
    $b.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
    $b.Dispose()
}

if ($Press -ne "" -or $Shot -ne "") {
    $p = Get-Process -Name hiumod -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
    if (-not $p) {
        if (Get-Process -Name hiumod -ErrorAction SilentlyContinue) { "panel running but has no window (hidden in the tray?)" }
        else { "panel not running" }
        exit 1
    }
    $h = $p.MainWindowHandle

    if ($Press -ne "") {
        $root = [System.Windows.Automation.AutomationElement]::FromHandle($h)
        $cond = New-Object System.Windows.Automation.PropertyCondition(
            [System.Windows.Automation.AutomationElement]::NameProperty, $Press)
        $el = $null
        # AccessKit builds its tree on the first request: ask a few times.
        for ($i = 0; $i -lt 10 -and -not $el; $i++) {
            $el = $root.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $cond)
            if (-not $el) { Start-Sleep -Milliseconds 300 }
        }
        if (-not $el) { "not found: '$Press'"; exit 2 }
        if ($Find) { "found: '$Press'"; exit 0 }
        $pat = $null
        if ($el.TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern, [ref]$pat)) { $pat.Invoke() }
        elseif ($el.TryGetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern, [ref]$pat)) { $pat.Select() }
        elseif ($el.TryGetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern, [ref]$pat)) { $pat.Toggle() }
        else { "no usable pattern: '$Press'"; exit 3 }
        "pressed '$Press'"
        Start-Sleep -Milliseconds $WaitMs
    }

    if ($Shot -ne "") {
        if ($p.HasExited) { "panel closed before the capture"; exit 1 }
        $r = Get-Bounds $h
        $w = $r.R - $r.L; $hh = $r.B - $r.T
        $b = New-Object System.Drawing.Bitmap $w, $hh
        $g = [System.Drawing.Graphics]::FromImage($b)
        $dc = $g.GetHdc()
        [void][HiuWin]::PrintWindow($h, $dc, 2)                     # PW_RENDERFULLCONTENT
        $g.ReleaseHdc($dc); $g.Dispose()
        Save-Png $b $Shot
        "panel ${w}x$hh -> $Shot"
    }
}

if ($Game -ne "") {
    $gp = Get-Process -Name "HellIsUs-Win64-Shipping" -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
    if (-not $gp) { "no game window"; exit 4 }
    $r = Get-Bounds $gp.MainWindowHandle
    $w = $r.R - $r.L; $hh = $r.B - $r.T
    $b = New-Object System.Drawing.Bitmap $w, $hh
    $g = [System.Drawing.Graphics]::FromImage($b)
    $g.CopyFromScreen($r.L, $r.T, 0, 0, (New-Object System.Drawing.Size $w, $hh))
    $g.Dispose()
    Save-Png $b $Game
    "game ${w}x$hh at $($r.L),$($r.T) -> $Game"
}
exit 0
