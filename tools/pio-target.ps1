<#
.SYNOPSIS
    Build one firmware target against its own PlatformIO package directory.

.DESCRIPTION
    Why this exists (see next.md section 7):

    pioarduino's arduino.py compares MD5(custom_sdkconfig + mcu + board memory
    fingerprint) against the first line of the PROJECT-ROOT sdkconfig.defaults
    ("# TASMOTA__<hash>"). That file is a single project-wide artifact, so two
    targets with different boards/memory types keep overwriting each other's
    expected hash. On mismatch check_reinstall_frwrk() deletes every generated
    sdkconfig.<env> and reinstalls BOTH framework-arduinoespressif32 and
    framework-arduinoespressif32-libs, which costs ~15 minutes per target
    switch (measured: 154g alone 188 s, the following note4-b 926 s).

    The fix is a per-target packages directory, injected from the CALLER because
    "[platformio]" is project-wide and core_dir is "Multiple: No".

    PlatformIO does NOT fall back to the shared core packages directory when
    packages_dir is set (verified: an empty packages_dir makes it reinstall),
    so the per-target directory must be self-sufficient. Re-downloading the
    ~5 GB toolchain set per target is pointless, though: only the two framework
    packages actually conflict. So this script

      * REAL-COPIES the conflicting pair, so each target owns and may reinstall
        its own copy without touching the other; and
      * JUNCTIONS every other package to the shared core packages, so the
        toolchains, espidf, cmake, scons, esptool and gdb are neither copied
        nor re-downloaded (Windows directory junctions need no admin rights).

    Requires full filesystem access only for PlatformIO's own user-level lock at
    <core_dir>\platforms.lock; the per-target package root lives INSIDE the
    repository (default <repo>\.pio-pkgs) so populating it needs no elevated
    access and stays self-contained.

.PARAMETER Target
    Logical target: note4 (= zectrix-note4-b) or 154g (= esp32-s3-epaper-154g).

.PARAMETER Setup
    Only create/populate the per-target package directory; do not build.

.PARAMETER RefreshOwned
    Re-copy the target-owned framework packages even if already present.

.PARAMETER PackagesRoot
    Root holding the per-target package directories. Defaults to
    <repo>\.pio-pkgs (gitignored).

.PARAMETER PioArgs
    Extra arguments forwarded to `pio run` (e.g. -t upload).

.EXAMPLE
    pwsh tools/pio-target.ps1 -Target note4 -Setup
    pwsh tools/pio-target.ps1 -Target note4
    pwsh tools/pio-target.ps1 -Target note4 -t upload
#>
[CmdletBinding()]
param(
    [ValidateSet('note4', '154g')]
    [string]$Target = 'note4',

    [switch]$Setup,
    [switch]$RefreshOwned,
    [switch]$SharedCoreDir,

    [string]$PackagesRoot = '',

    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$PioArgs
)

$ErrorActionPreference = 'Stop'

# Default to a repository-local root so the isolation never needs access outside
# the workspace. Override with -PackagesRoot if the packages should live elsewhere.
$RepoRoot = Split-Path -Parent $PSScriptRoot
if (-not $PackagesRoot) { $PackagesRoot = Join-Path $RepoRoot '.pio-pkgs' }

# Only these two packages ever conflict between targets; everything else is
# shared. Keep this list in sync with next.md section 7.
$OwnedPackages = @(
    'framework-arduinoespressif32',
    'framework-arduinoespressif32-libs'
)

$EnvByTarget = @{
    'note4' = 'zectrix-note4-b'
    '154g'  = 'esp32-s3-epaper-154g'
}

$envName = $EnvByTarget[$Target]
$pkgDir = Join-Path $PackagesRoot $Target

$coreDir = if ($env:PLATFORMIO_CORE_DIR) { $env:PLATFORMIO_CORE_DIR }
           else { Join-Path $env:USERPROFILE '.platformio' }
$sharedDir = Join-Path $coreDir 'packages'

# PlatformIO writes <core_dir>\platforms.lock on every single run, so pointing
# core_dir at the machine-level installation forces every build to need write
# access outside the repository. A repository-local core dir keeps the whole
# build inside the workspace; the platform itself is shared through a junction.
$localCoreDir = Join-Path $RepoRoot '.pio-core'

Write-Host "target env      : $envName"
Write-Host "packages dir    : $pkgDir"
Write-Host "core dir        : $(if ($SharedCoreDir) { $coreDir } else { $localCoreDir })"
Write-Host "shared packages : $sharedDir"

if (-not (Test-Path $sharedDir)) {
    throw "shared packages directory not found: $sharedDir"
}

# ------------------------------------------------------------- core dir ------

function Initialize-CoreDir {
    param([string]$LocalCore, [string]$SharedCore)

    foreach ($sub in @('platforms', 'cache', 'tools')) {
        New-Item -ItemType Directory -Force -Path (Join-Path $LocalCore $sub) | Out-Null
    }
    $sharedPlatforms = Join-Path $SharedCore 'platforms'
    if (-not (Test-Path $sharedPlatforms)) {
        throw "shared platforms directory not found: $sharedPlatforms"
    }
    foreach ($p in Get-ChildItem -Path $sharedPlatforms -Directory) {
        $dst = Join-Path $LocalCore "platforms\$($p.Name)"
        if (Test-Path $dst) { continue }
        New-Item -ItemType Junction -Path $dst -Target $p.FullName | Out-Null
        Write-Host "  link   platforms\$($p.Name)"
    }
}

# ---------------------------------------------------------------- populate ---

function Initialize-TargetPackages {
    param([string]$PkgDir, [string]$SharedDir, [string[]]$Owned, [switch]$Refresh)

    New-Item -ItemType Directory -Force -Path $PkgDir | Out-Null

    foreach ($src in Get-ChildItem -Path $SharedDir -Directory) {
        $name = $src.Name
        $dst = Join-Path $PkgDir $name

        if ($Owned -contains $name) {
            # Real, target-owned copy: this is the one that may be reinstalled.
            if ((Test-Path $dst) -and -not $Refresh) {
                $item = Get-Item $dst -Force
                if ($item.LinkType) {
                    throw "$dst is a $($item.LinkType); expected a real directory (rerun with -RefreshOwned)"
                }
                Write-Host "  keep   $name (owned copy present)"
                continue
            }
            if (Test-Path $dst) { Remove-Item $dst -Recurse -Force }
            Write-Host "  copy   $name ..."
            $null = robocopy $src.FullName $dst /MIR /NFL /NDL /NJH /NJS /NP /R:1 /W:1
            if ($LASTEXITCODE -ge 8) { throw "robocopy failed for $name (exit $LASTEXITCODE)" }
            Write-Host "  copy   $name done"
        }
        else {
            # Shared, never conflicting: junction so nothing is duplicated.
            if (Test-Path $dst) {
                $item = Get-Item $dst -Force
                if ($item.LinkType -eq 'Junction' -and
                    ($item.Target -contains $src.FullName)) {
                    continue
                }
                Write-Host "  relink $name"
                Remove-Item $dst -Recurse -Force
            }
            New-Item -ItemType Junction -Path $dst -Target $src.FullName | Out-Null
        }
    }

    # Report the split so drift is visible on every setup run.
    $owned = Get-ChildItem $PkgDir -Directory -Force |
        Where-Object { -not $_.LinkType } | Select-Object -ExpandProperty Name
    $linked = Get-ChildItem $PkgDir -Directory -Force |
        Where-Object { $_.LinkType } | Select-Object -ExpandProperty Name
    Write-Host "  owned (real) : $($owned -join ', ')"
    Write-Host "  linked       : $($linked.Count) packages"

    $missing = $Owned | Where-Object { -not (Test-Path (Join-Path $PkgDir $_)) }
    if ($missing) { throw "owned packages missing after setup: $($missing -join ', ')" }
}

Write-Host ''
Write-Host 'populating per-target packages ...'
Initialize-TargetPackages -PkgDir $pkgDir -SharedDir $sharedDir -Owned $OwnedPackages -Refresh:$RefreshOwned

if ($SharedCoreDir) {
    $env:PLATFORMIO_CORE_DIR = $coreDir
} else {
    Write-Host ''
    Write-Host 'preparing repository-local core dir ...'
    Initialize-CoreDir -LocalCore $localCoreDir -SharedCore $coreDir
    $env:PLATFORMIO_CORE_DIR = $localCoreDir
}

if ($Setup) {
    Write-Host ''
    Write-Host 'setup only; not building.'
    exit 0
}

# ------------------------------------------------------------------- build ---

$env:PLATFORMIO_PACKAGES_DIR = $pkgDir
Write-Host ''
Write-Host "PLATFORMIO_CORE_DIR     = $env:PLATFORMIO_CORE_DIR"
Write-Host "PLATFORMIO_PACKAGES_DIR = $env:PLATFORMIO_PACKAGES_DIR"
Write-Host "pio run -e $envName $($PioArgs -join ' ')"
Write-Host ''

& pio run -e $envName @PioArgs
exit $LASTEXITCODE
