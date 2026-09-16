$dir = Split-Path -Parent $MyInvocation.MyCommand.Path
$pidFile = Join-Path $dir "bridge.pid"
if (Test-Path $pidFile) {
    $old = Get-Content $pidFile -ErrorAction SilentlyContinue
    if ($old) { Stop-Process -Id $old -Force -ErrorAction SilentlyContinue }
    Remove-Item $pidFile -ErrorAction SilentlyContinue
}
Get-Process python -ErrorAction SilentlyContinue |
    Where-Object { $_.Path -like '*Python314*' } |
    Stop-Process -Force -ErrorAction SilentlyContinue
Write-Host "bridge stopped"
