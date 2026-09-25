# Popup-free bridge restart for the current build output directory.
#
# Why this exists: starting the tray app (Tauri/WebView2 + a detached child)
# cannot be done from inside the DSH file sandbox, so `tools/start-bridge.ps1`
# needs a privileged (danger-full-access) command — i.e. an approval prompt per
# start. A plain restart does not need that: the app ships its own watchdog
# (`bridge/crates/app/src/watchdog.rs`), which relaunches the app when its parent
# exits abnormally. So killing ONLY the main process makes the surviving watchdog
# bring the bridge back, entirely inside the sandbox.
#
# Limits, by design of that watchdog:
#   * it restarts only on a non-zero exit code (a forced kill qualifies);
#   * three abnormal exits inside five minutes make it give up and exit
#     (`<data>/logs/watchdog.log` records every attempt). After that the next
#     start needs a privileged command again.
#
# A REBUILD is different: while either process runs it holds
# `bridge-app.exe` open, so `cargo build` cannot replace it. That path must stop
# both processes and then start the app with a privileged command — this script
# refuses to do that rather than leave the bridge down.
#
# Usage:
#   pwsh tools/restart-bridge.ps1                # restart the default instance
#   pwsh tools/restart-bridge.ps1 -WaitSecs 30   # allow a slower relaunch

param(
    [ValidatePattern('^[a-z][a-z0-9-]{0,26}$')][string]$Instance = 'default',
    [ValidateRange(3, 120)][int]$WaitSecs = 20,
    [string]$ExePath
)
$ErrorActionPreference = 'Stop'

$root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
$exe = if ($ExePath) { (Resolve-Path -LiteralPath $ExePath).Path } else { Join-Path $root 'bridge\target\debug\bridge-app.exe' }
$dataDir = if ($Instance -eq 'default') {
    Join-Path (Split-Path -Parent $exe) 'data'
} else {
    Join-Path (Split-Path -Parent $exe) "instances\$Instance\data"
}
$watchdogLog = Join-Path $dataDir 'logs\watchdog.log'
$port = if ($Instance -eq 'default') { 8765 } else { 0 }

if (-not (Test-Path $exe)) { throw "bridge-app.exe not built: $exe" }

# The main process is whoever owns the HTTP listener; the watchdog owns nothing.
function Get-ListenerPid([int]$TargetPort) {
    if ($TargetPort -le 0) { return 0 }
    # Get-NetTCPConnection is not reliable inside the restricted sandbox; netstat is.
    $line = netstat -ano | Select-String -Pattern ":$TargetPort\s+.*LISTENING" | Select-Object -First 1
    if (-not $line) { return 0 }
    $parts = ($line.Line.Trim() -split '\s+')
    return [int]$parts[-1]
}

$running = @(Get-Process -Name bridge-app -ErrorAction SilentlyContinue)
if ($running.Count -eq 0) {
    throw "bridge-app is not running. A restart cannot bring it back (no watchdog is alive); start it with a privileged command: pwsh tools/start-bridge.ps1"
}

$mainPid = Get-ListenerPid $port
if ($mainPid -eq 0) {
    # No listener yet: the app may still be starting, or the port differs.
    $mainPid = ($running | Sort-Object StartTime | Select-Object -First 1).Id
    Write-Output "warning: no listener on port $port; assuming the oldest process ($mainPid) is the main one"
}
$watchdogPids = @($running | Where-Object { $_.Id -ne $mainPid } | Select-Object -ExpandProperty Id)
if ($watchdogPids.Count -eq 0) {
    throw "no watchdog process is alive next to the main process ($mainPid). Killing the main would leave the bridge down; use a privileged start instead."
}

# Report the watchdog's restart budget before spending one (3 per 5 minutes).
$recent = 0
if (Test-Path $watchdogLog) {
    $cutoff = [DateTimeOffset]::UtcNow.ToUnixTimeSeconds() - 300
    $recent = @(Get-Content $watchdogLog | Where-Object {
        $_ -match '^abnormal\s+(\d+)' -and [int]$Matches[1] -gt $cutoff
    }).Count
}
Write-Output "bridge: main=$mainPid watchdog=$($watchdogPids -join ',')  recent abnormal exits=$recent/3"
if ($recent -ge 3) {
    throw "the watchdog already gave up (3 abnormal exits within 5 minutes). Wait for the window to pass, or start the bridge with a privileged command."
}
if ($recent -eq 2) {
    Write-Output "warning: this restart is the third inside 5 minutes; the watchdog will give up on the NEXT one"
}

Write-Output "killing the main process $mainPid (watchdog stays alive) ..."
Stop-Process -Id $mainPid -Force -ErrorAction SilentlyContinue

$deadline = (Get-Date).AddSeconds($WaitSecs)
$newPid = 0
while ((Get-Date) -lt $deadline) {
    Start-Sleep -Milliseconds 400
    $candidate = Get-ListenerPid $port
    if ($candidate -ne 0 -and $candidate -ne $mainPid) { $newPid = $candidate; break }
}
if ($newPid -eq 0) {
    $stillThere = @(Get-Process -Name bridge-app -ErrorAction SilentlyContinue).Count
    throw "no listener on port $port after $WaitSecs s (bridge-app processes now: $stillThere). Check $watchdogLog and start the bridge with a privileged command if needed."
}

$newWatchdog = @(Get-Process -Name bridge-app -ErrorAction SilentlyContinue |
    Where-Object { $_.Id -ne $newPid } | Select-Object -ExpandProperty Id)
Write-Output "restarted: main $mainPid -> $newPid (watchdog: $($newWatchdog -join ',')), port $port is listening"
if (Test-Path $watchdogLog) { Write-Output "watchdog log tail:"; Get-Content $watchdogLog -Tail 2 | ForEach-Object { "  $_" } }
Write-Output "note: a rebuild still needs both processes stopped plus one privileged start; this script only covers the restart."
