param(
    [Parameter(Mandatory)][ValidateSet('note4', '154g')][string]$Target,
    [string]$OutputDir = "$PSScriptRoot/../artifacts/release-ci/release-dist"
)
$ErrorActionPreference = 'Stop'
$repo = Split-Path $PSScriptRoot -Parent
$OutputDir = [IO.Path]::GetFullPath($OutputDir)
$pioEnv = if ($Target -eq 'note4') { 'zectrix-note4-b' } else { 'esp32-s3-epaper-154g' }
Push-Location $repo
try {
    & pwsh -NoProfile -File "$PSScriptRoot/pio-target.ps1" -Target $Target
    if ($LASTEXITCODE -ne 0) { throw "$pioEnv build failed: $LASTEXITCODE" }
    & python -B tools/check-wifi-stack.py ".pio/build/$pioEnv/firmware.elf" --objdump ".pio-pkgs/$Target/toolchain-xtensa-esp-elf/bin/xtensa-esp32s3-elf-objdump.exe" --no-online-wifi-migration --no-legacy-management-entry
    if ($LASTEXITCODE -ne 0) { throw "$pioEnv Wi-Fi stack gate failed" }
    & python -B tools/test-release-rom.py
    if ($LASTEXITCODE -ne 0) { throw 'ROM staging regression failed' }
    & python -B tools/stage-release-rom.py --env $pioEnv --out $OutputDir
    if ($LASTEXITCODE -ne 0) { throw "$pioEnv ROM verification/staging failed" }
} finally { Pop-Location }
