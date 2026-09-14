<#
.SYNOPSIS
    Incrementally updates an existing AdeshLang installation with local build artifacts.

.DESCRIPTION
    Scans the installed AdeshLang directory (e.g. C:\Program Files\AdeshLang or $env:ADESH_HOME),
    compares files with the workspace release build (binaries, static runtime library, stdlib),
    and copies ONLY modified or new files. Preserves installed toolchains (LLVM/Clang), Python
    environments, and AI models. Automatically elevates to Administrator if required.

.PARAMETER TargetDir
    Custom installation path to update. If omitted, automatically discovered.

.PARAMETER SkipBuild
    Skip running `cargo build --release` and use existing target/release artifacts.

.PARAMETER WhatIf
    Dry-run mode: show what would be updated without modifying files.

.EXAMPLE
    .\scripts\update_installed.ps1
    .\scripts\update_installed.ps1 -SkipBuild
    .\scripts\update_installed.ps1 -WhatIf
#>

[CmdletBinding()]
param(
    [string]$TargetDir,
    [switch]$SkipBuild,
    [switch]$WhatIf
)

$ErrorActionPreference = "Stop"
$WorkspaceRoot = (Resolve-Path "$PSScriptRoot\..").Path

# Helper: Check for Administrator rights
function Test-IsAdmin {
    $currentPrincipal = New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
    return $currentPrincipal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

# 1. Discover Target Installation Directory
if (-not $TargetDir) {
    if ($env:ADESH_HOME -and (Test-Path $env:ADESH_HOME)) {
        $TargetDir = $env:ADESH_HOME
    } else {
        # Check Inno Setup registry
        $regKey = "HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\{6E48268F-7B2B-450F-A8C5-18159E4712F2}_is1"
        if (Test-Path $regKey) {
            $regInstall = (Get-ItemProperty -Path $regKey -Name "InstallLocation" -ErrorAction SilentlyContinue).InstallLocation
            if ($regInstall -and (Test-Path $regInstall)) {
                $TargetDir = $regInstall
            }
        }
    }

    if (-not $TargetDir) {
        # Check PATH for adesh.exe
        $cmd = Get-Command "adesh" -ErrorAction SilentlyContinue
        if ($cmd -and $cmd.Source) {
            $binDir = Split-Path $cmd.Source -Parent
            $candidate = Split-Path $binDir -Parent
            if (Test-Path "$candidate\bin\adesh.exe") {
                $TargetDir = $candidate
            }
        }
    }

    if (-not $TargetDir) {
        $defaultPath = "C:\Program Files\AdeshLang"
        if (Test-Path $defaultPath) {
            $TargetDir = $defaultPath
        }
    }
}

if (-not $TargetDir -or -not (Test-Path $TargetDir)) {
    Write-Error "Could not locate an existing AdeshLang installation. Specify -TargetDir manually."
    exit 1
}

$TargetDir = (Resolve-Path $TargetDir).Path

# Check if target directory requires Administrator elevation (skip check in WhatIf mode)
$needsAdmin = $false
if (-not $WhatIf) {
    $testWritePath = Join-Path $TargetDir ".write_test"
    try {
        [IO.File]::WriteAllText($testWritePath, "test")
        Remove-Item -Force $testWritePath -ErrorAction SilentlyContinue
    } catch {
        $needsAdmin = $true
    }
}

if ($needsAdmin -and -not (Test-IsAdmin)) {
    Write-Host "Target directory '$TargetDir' requires Administrator privileges." -ForegroundColor Yellow
    Write-Host "Attempting to re-launch with elevated permissions..." -ForegroundColor Cyan
    
    $argsList = "-NoProfile -ExecutionPolicy Bypass -File `"$PSCommandPath`" -TargetDir `"$TargetDir`""
    if ($SkipBuild) { $argsList += " -SkipBuild" }

    try {
        Start-Process powershell -Verb RunAs -ArgumentList $argsList
        exit 0
    } catch {
        Write-Error "Could not auto-elevate. Please run this script in an Administrator PowerShell window:`n  powershell -ExecutionPolicy Bypass -File `"$PSCommandPath`""
        exit 1
    }
}

Write-Host "──────────────────────────────────────────────────────" -ForegroundColor Cyan
Write-Host "  AdeshLang Incremental Updater" -ForegroundColor Green
Write-Host "──────────────────────────────────────────────────────" -ForegroundColor Cyan
Write-Host "  Installation Path : $TargetDir"
Write-Host "  Workspace Root    : $WorkspaceRoot"
Write-Host ""

# 2. Build Release Artifacts (if requested)
if (-not $SkipBuild) {
    Write-Host "==> Compiling latest release artifacts (cargo build --release)..." -ForegroundColor Cyan
    Push-Location $WorkspaceRoot
    try {
        & cargo build --release
        if ($LASTEXITCODE -ne 0) {
            Write-Error "Cargo build failed with exit code $LASTEXITCODE."
            exit $LASTEXITCODE
        }
    } finally {
        Pop-Location
    }
}

# 3. Define components to incrementally sync
$ReleaseDir = Join-Path $WorkspaceRoot "target\release"

$FileMappings = @()

# Binaries & Dynamic Libraries
if (Test-Path "$ReleaseDir\adesh.exe") {
    $FileMappings += @{ Source = "$ReleaseDir\adesh.exe"; Dest = "$TargetDir\bin\adesh.exe" }
}
if (Test-Path "$ReleaseDir\adl.exe") {
    $FileMappings += @{ Source = "$ReleaseDir\adl.exe"; Dest = "$TargetDir\bin\adl.exe" }
}
if (Test-Path "$ReleaseDir\adesh-editor.exe") {
    $FileMappings += @{ Source = "$ReleaseDir\adesh-editor.exe"; Dest = "$TargetDir\bin\adesh-editor.exe" }
}
if (Test-Path "$ReleaseDir\adeshlang.dll") {
    $FileMappings += @{ Source = "$ReleaseDir\adeshlang.dll"; Dest = "$TargetDir\bin\adeshlang.dll" }
    $FileMappings += @{ Source = "$ReleaseDir\adeshlang.dll"; Dest = "$TargetDir\lib\adeshlang.dll" }
}

# Static & Import Runtime Libraries (.lib, .dll.lib, .a)
Get-ChildItem "$ReleaseDir" -Include "adeshlang.lib", "adeshlang.dll.lib", "libadeshlang.a", "*.lib" -File | ForEach-Object {
    $FileMappings += @{ Source = $_.FullName; Dest = "$TargetDir\lib\$($_.Name)" }
}

# Config / Manifests
if (Test-Path "$WorkspaceRoot\installer\manifests\toolchain-manifest.json") {
    $FileMappings += @{ Source = "$WorkspaceRoot\installer\manifests\toolchain-manifest.json"; Dest = "$TargetDir\config\toolchain-manifest.json" }
}

# Standard Library Files (Recursive)
$StdSrc = Join-Path $WorkspaceRoot "std"
if (Test-Path $StdSrc) {
    Get-ChildItem -Path $StdSrc -Recurse -File | ForEach-Object {
        $relPath = $_.FullName.Substring($StdSrc.Length).TrimStart('\', '/')
        $FileMappings += @{
            Source = $_.FullName
            Dest   = Join-Path "$TargetDir\std" $relPath
        }
    }
}

# 4. Helper for File Hash Comparison
function Get-FileSha256($path) {
    if (-not (Test-Path $path)) { return $null }
    $hash = Get-FileHash -Path $path -Algorithm SHA256
    return $hash.Hash
}

# 5. Incremental Synchronization
$updatedCount = 0
$addedCount = 0
$skippedCount = 0

Write-Host "==> Checking for modified files..." -ForegroundColor Cyan

foreach ($map in $FileMappings) {
    $src = $map.Source
    $dst = $map.Dest

    if (-not (Test-Path $src)) { continue }

    $destExists = Test-Path $dst
    $needsUpdate = $false
    $actionType = ""

    if (-not $destExists) {
        $needsUpdate = $true
        $actionType = "NEW"
    } else {
        $srcHash = Get-FileSha256 $src
        $dstHash = Get-FileSha256 $dst
        if ($srcHash -ne $dstHash) {
            $needsUpdate = $true
            $actionType = "UPDATED"
        }
    }

    if ($needsUpdate) {
        $displayPath = $dst.Substring($TargetDir.Length).TrimStart('\', '/')
        if ($WhatIf) {
            Write-Host "  [WhatIf: $actionType] $displayPath" -ForegroundColor Yellow
            if ($actionType -eq "NEW") { $addedCount++ } else { $updatedCount++ }
        } else {
            $parentDir = Split-Path $dst -Parent
            if (-not (Test-Path $parentDir)) {
                New-Item -ItemType Directory -Path $parentDir -Force | Out-Null
            }

            # Safe Windows atomic replacement for active .exe files
            if ($destExists -and ($dst.EndsWith(".exe") -or $dst.EndsWith(".dll"))) {
                $oldPath = "$dst.old"
                if (Test-Path $oldPath) {
                    Remove-Item -Force $oldPath -ErrorAction SilentlyContinue
                }
                try {
                    Move-Item -Force $dst $oldPath -ErrorAction SilentlyContinue
                } catch {
                    # If rename fails, try direct copy
                }
            }

            Copy-Item -Path $src -Destination $dst -Force
            Write-Host "  [$actionType] $displayPath" -ForegroundColor Green
            if ($actionType -eq "NEW") { $addedCount++ } else { $updatedCount++ }
        }
    } else {
        $skippedCount++
    }
}

# Clean up any leftover .old files
Get-ChildItem -Path "$TargetDir\bin\*.old" -ErrorAction SilentlyContinue | ForEach-Object {
    Remove-Item -Force $_.FullName -ErrorAction SilentlyContinue
}

Write-Host ""
Write-Host "──────────────────────────────────────────────────────" -ForegroundColor Cyan
if ($WhatIf) {
    Write-Host "  [Dry Run Complete] $updatedCount to update, $addedCount new, $skippedCount up to date." -ForegroundColor Yellow
} else {
    Write-Host "  ✓ Update Complete! $updatedCount updated, $addedCount added, $skippedCount unchanged." -ForegroundColor Green
    Write-Host ""
    
    # Run adesh doctor to verify health
    $adeshExe = "$TargetDir\bin\adesh.exe"
    if (Test-Path $adeshExe) {
        Write-Host "==> Verifying installation health (adesh doctor)..." -ForegroundColor Cyan
        & $adeshExe doctor
    }
}
Write-Host "──────────────────────────────────────────────────────" -ForegroundColor Cyan
