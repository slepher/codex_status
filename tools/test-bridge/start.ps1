$dir = Split-Path -Parent $MyInvocation.MyCommand.Path
$pidFile = Join-Path $dir "bridge.pid"
if (Test-Path $pidFile) {
    $old = Get-Content $pidFile -ErrorAction SilentlyContinue
    if ($old -and (Get-Process -Id $old -ErrorAction SilentlyContinue)) {
        Write-Host "already running, PID $old"
        exit 0
    }
}
$p = Start-Process -FilePath "python" -ArgumentList "-u", "`"$dir\bridge.py`"" -WorkingDirectory $dir `
    -RedirectStandardOutput "$dir\bridge.log" -RedirectStandardError "$dir\bridge.err" `
    -WindowStyle Hidden -PassThru
$p.Id | Set-Content $pidFile
Write-Host "bridge started, PID $($p.Id)"
Write-Host "log: $dir\bridge.log"
