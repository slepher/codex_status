# Start the tray bridge detached and return immediately.
#
# Why not plain Start-Process: when a spawned process inherits the caller's
# stdio handles, the calling tool/shell can stay blocked until the child exits.
# Here UseShellExecute=true fully detaches the child (no handle inheritance),
# cmd.exe redirects output to files, and the hidden window keeps it quiet.
#
# Usage: pwsh tools/start-bridge.ps1
$ErrorActionPreference = "Stop"

$root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
$exe = Join-Path $root "bridge\target\debug\bridge-app.exe"
$logDir = Join-Path $root "artifacts"
$out = Join-Path $logDir "bridge-app-run.out"
$err = Join-Path $logDir "bridge-app-run.err"
$pidFile = Join-Path $logDir "bridge-app-run.pid"

if (-not (Test-Path $exe)) {
    throw "bridge-app.exe not built: $exe (run: cargo build -p bridge-app)"
}
New-Item -ItemType Directory -Path $logDir -Force | Out-Null

$existing = Get-Process -Name bridge-app -ErrorAction SilentlyContinue
if ($existing) {
    Set-Content -Path $pidFile -Value $existing[0].Id
    Write-Output "bridge-app already running (PID $($existing.Id -join ', '))"
    exit 0
}

$psi = New-Object System.Diagnostics.ProcessStartInfo
$psi.FileName = "cmd.exe"
$psi.Arguments = "/c `"`"$exe`" > `"$out`" 2> `"$err`"`""
$psi.WorkingDirectory = $root
$psi.UseShellExecute = $true
$psi.WindowStyle = [System.Diagnostics.ProcessWindowStyle]::Hidden
[void][System.Diagnostics.Process]::Start($psi)

$deadline = (Get-Date).AddSeconds(10)
while ((Get-Date) -lt $deadline) {
    $proc = Get-Process -Name bridge-app -ErrorAction SilentlyContinue
    if ($proc) {
        Set-Content -Path $pidFile -Value $proc[0].Id
        Write-Output "bridge-app started (PID $($proc.Id -join ', ')), logs: $out / $err"
        exit 0
    }
    Start-Sleep -Milliseconds 250
}
throw "bridge-app did not appear within 10s; check $err"
