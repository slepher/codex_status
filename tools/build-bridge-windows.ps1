param(
    [string]$TargetDir = "$PSScriptRoot/../artifacts/release-ci/target",
    [string]$OutputDir = "$PSScriptRoot/../artifacts/release-ci/dist"
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if (-not $IsWindows) { throw 'Windows native build required.' }
$repo = Split-Path $PSScriptRoot -Parent
$TargetDir = [IO.Path]::GetFullPath($TargetDir)
$OutputDir = [IO.Path]::GetFullPath($OutputDir)
if (-not $env:CODEX_STATUS_ARDUINOJSON -or
    -not (Test-Path "$env:CODEX_STATUS_ARDUINOJSON/ArduinoJson.h")) {
    throw 'Set CODEX_STATUS_ARDUINOJSON to the pinned ArduinoJson src directory.'
}
function Invoke-Checked([string]$Program, [string[]]$Arguments) {
    & $Program @Arguments
    if ($LASTEXITCODE -ne 0) { throw "$Program failed with exit code $LASTEXITCODE" }
}
Push-Location $repo
try {
    Invoke-Checked python @('-B', 'tools/test-unicode-font.py')
    Invoke-Checked python @('-B', 'tools/test-release-rom.py')
    Set-Location "$repo/bridge"
    Invoke-Checked cargo @('test', '--locked', '--target-dir', $TargetDir,
        '-p', 'bridge-core', '-p', 'bridge-ble', '-p', 'bridge-render', '-p', 'bridge-app')
    Invoke-Checked cargo @('build', '--locked', '--target-dir', $TargetDir, '-p', 'bridge-app', '--release')
    Set-Location $repo
    # A unique staging directory prevents stale files or runtime data entering the ZIP.
    $stage = Join-Path $OutputDir ([guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path "$stage/seed" -Force | Out-Null
    Copy-Item "$TargetDir/release/bridge-app.exe" $stage
    Copy-Item "$repo/tools/test-bridge/templates" "$stage/seed/templates" -Recurse
    Copy-Item "$repo/LICENSE", "$repo/README.md" $stage
    $zip = Join-Path $OutputDir 'codex-status-bridge-windows-x64.zip'
    Compress-Archive -Path "$stage/*" -DestinationPath $zip -Force
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $archive = [IO.Compression.ZipFile]::OpenRead($zip)
    try {
        $expected = @(Get-ChildItem $stage -File -Recurse | ForEach-Object {
            [IO.Path]::GetRelativePath($stage, $_.FullName).Replace('\', '/')
        } | Sort-Object)
        $actual = @($archive.Entries | Where-Object { $_.Name } | ForEach-Object {
            $_.FullName.Replace('\', '/')
        } | Sort-Object)
        if (Compare-Object $expected $actual) { throw 'ZIP content mismatch.' }
        if ($actual -notcontains 'bridge-app.exe') { throw 'ZIP missing executable.' }
    } finally { $archive.Dispose() }
    $hash = (Get-FileHash $zip -Algorithm SHA256).Hash.ToLowerInvariant()
    "$hash  $([IO.Path]::GetFileName($zip))" | Set-Content "$zip.sha256" -Encoding utf8NoBOM
    Get-Item $zip | Select-Object FullName, Length
    Write-Output "SHA256 $hash"
} finally { Pop-Location }
