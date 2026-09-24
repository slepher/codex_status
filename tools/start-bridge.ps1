# Start the tray bridge detached and return immediately.
#
# Why not plain Start-Process: when a spawned process inherits the caller's
# stdio handles, the calling tool/shell can stay blocked until the child exits.
# Here UseShellExecute=true fully detaches the child (no handle inheritance),
# cmd.exe redirects output to files, and the hidden window keeps it quiet.
#
# Usage: pwsh tools/start-bridge.ps1
param(
    [ValidatePattern('^[a-z][a-z0-9-]{0,26}$')][string]$Instance = 'default',
    [ValidateRange(0, 65535)][int]$Port = 0,
    [ValidateRange(0, 65535)][int]$McpPort = 0,
    [ValidateSet('square', 'circle', 'diamond')][string]$IconShape = 'square',
    [string]$DeviceMac,
    [string]$DeviceIp,
    [string]$ExePath
)
$ErrorActionPreference = "Stop"

$root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
$exe = if ($ExePath) { (Resolve-Path -LiteralPath $ExePath).Path } else { Join-Path $root "bridge\target\debug\bridge-app.exe" }
$logDir = Join-Path $root "artifacts"
$out = Join-Path $logDir "bridge-app-$Instance.out"
$err = Join-Path $logDir "bridge-app-$Instance.err"
$pidFile = Join-Path $logDir "bridge-app-$Instance.pid"
if ($Instance -eq 'default') {
    $out = Join-Path $logDir 'bridge-app-run.out'
    $err = Join-Path $logDir 'bridge-app-run.err'
    $pidFile = Join-Path $logDir 'bridge-app-run.pid'
    if (-not $Port) { $Port = 8765 }
    if (-not $McpPort) { $McpPort = 8766 }
} elseif (-not $Port -or -not $McpPort) {
    throw 'Named instances require -Port and -McpPort.'
}
if ($Port -eq $McpPort) { throw 'HTTP and MCP ports must differ.' }
if ($DeviceMac -and $DeviceMac -notmatch '^[0-9A-Fa-f]{12}$') { throw 'DeviceMac must be a 12-digit Wi-Fi MAC.' }
if ($DeviceIp) {
    $address = $null
    if (-not [System.Net.IPAddress]::TryParse($DeviceIp, [ref]$address) -or
        $address.AddressFamily -ne [System.Net.Sockets.AddressFamily]::InterNetwork) {
        throw 'DeviceIp must be an IPv4 address.'
    }
}

if (-not (Test-Path $exe)) {
    throw "bridge-app.exe not built: $exe (run: cargo build -p bridge-app)"
}
New-Item -ItemType Directory -Path $logDir -Force | Out-Null

if (Test-Path $pidFile) {
    $savedPid = [int](Get-Content -LiteralPath $pidFile -ErrorAction SilentlyContinue | Select-Object -First 1)
    $existing = Get-Process -Id $savedPid -ErrorAction SilentlyContinue
    $owner = Get-NetTCPConnection -LocalPort $Port -State Listen -ErrorAction SilentlyContinue |
        Where-Object OwningProcess -eq $savedPid | Select-Object -First 1
    if ($existing -and $existing.Path -eq $exe -and $owner) {
        Write-Output "bridge-app instance '$Instance' already running (PID $savedPid)"
        exit 0
    }
}
foreach ($requestedPort in @($Port, $McpPort)) {
    if (Get-NetTCPConnection -LocalPort $requestedPort -State Listen -ErrorAction SilentlyContinue) {
        throw "Port $requestedPort is already in use."
    }
}

$env:CODEX_STATUS_INSTANCE = if ($Instance -eq 'default') { '' } else { $Instance }
$env:CODEX_STATUS_PORT = [string]$Port
$env:CODEX_STATUS_MCP_PORT = [string]$McpPort
$env:CODEX_STATUS_ICON_SHAPE = $IconShape
if ($DeviceMac) { $env:CODEX_STATUS_DEVICE_MAC = $DeviceMac }
if ($DeviceIp) { $env:CODEX_STATUS_DEVICE_IP = $DeviceIp }

$psi = New-Object System.Diagnostics.ProcessStartInfo
$psi.FileName = "cmd.exe"
$psi.Arguments = "/c `"`"$exe`" > `"$out`" 2> `"$err`"`""
$psi.WorkingDirectory = $root
$psi.UseShellExecute = $true
$psi.WindowStyle = [System.Diagnostics.ProcessWindowStyle]::Hidden
[void][System.Diagnostics.Process]::Start($psi)

$deadline = (Get-Date).AddSeconds(10)
while ((Get-Date) -lt $deadline) {
    $listener = Get-NetTCPConnection -LocalPort $Port -State Listen -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($listener) {
        $proc = Get-Process -Id $listener.OwningProcess -ErrorAction SilentlyContinue
        if ($proc -and $proc.Path -eq $exe) {
            Set-Content -Path $pidFile -Value $proc.Id
            Write-Output "bridge-app instance '$Instance' started (PID $($proc.Id), HTTP $Port, MCP $McpPort), logs: $out / $err"
            exit 0
        }
    }
    Start-Sleep -Milliseconds 250
}
throw "bridge-app did not appear within 10s; check $err"
