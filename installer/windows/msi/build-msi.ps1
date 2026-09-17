# ==============================================================================
# AdeshLang Enterprise MSI Builder
# ==============================================================================
#
# Requires:
#   WiX Toolset v5/v4-compatible CLI
#
# Usage:
#   .\build-msi.ps1
#   .\build-msi.ps1 -Arch x86_64
#   .\build-msi.ps1 -Arch aarch64
#
# ==============================================================================

[CmdletBinding()]
param(
    [ValidateSet("x86_64", "arm64", "aarch64")]
    [string]$Arch = "x86_64",

    [string]$OutputDir
)

$ErrorActionPreference = "Stop"

# Normalize ARM64 naming.
if ($Arch -eq "arm64") {
    $Arch = "aarch64"
}

# ==============================================================================
# PATHS
# ==============================================================================

$RootDir = (
    Resolve-Path (
        Join-Path $PSScriptRoot "..\..\.."
    )
).Path

$DistDir = Join-Path `
    $RootDir `
    "dist\windows-$Arch"

if (-not $OutputDir) {
    $OutputDir = $DistDir
}

$OutputDir = [System.IO.Path]::GetFullPath($OutputDir)

$StageDir = Join-Path `
    $PSScriptRoot `
    "staging"

$SourceDir = Join-Path `
    $StageDir `
    "windows-$Arch"

# ==============================================================================
# DETERMINE VERSION FROM CARGO.TOML
# ==============================================================================

$cargoToml = Join-Path `
    $RootDir `
    "Cargo.toml"

if (-not (Test-Path -LiteralPath $cargoToml)) {
    throw "Cargo.toml was not found at: $cargoToml"
}

$versionLine = Get-Content `
    -LiteralPath $cargoToml |
    Where-Object {
        $_ -match '^version\s*=\s*"([^"]+)"'
    } |
    Select-Object -First 1

if (-not $versionLine) {
    throw "Unable to determine AdeshLang version from Cargo.toml."
}

if ($versionLine -notmatch '^version\s*=\s*"([^"]+)"') {
    throw "Unable to parse AdeshLang version from Cargo.toml."
}

$Version = $Matches[1]

Write-Host "AdeshLang version: $Version"

# ==============================================================================
# OUTPUT
# ==============================================================================

$MsiPath = Join-Path `
    $OutputDir `
    "AdeshLang-$Version-$Arch-windows.msi"

# ==============================================================================
# VALIDATE DISTRIBUTION
# ==============================================================================

$RequiredPaths = @(
    (Join-Path $DistDir "bin\adesh.exe"),
    (Join-Path $DistDir "std"),
    (Join-Path $DistDir "config"),
    (Join-Path $DistDir "licenses")
)

foreach ($requiredPath in $RequiredPaths) {

    if (-not (Test-Path -LiteralPath $requiredPath)) {

        throw `
            "Distribution staging is missing required path: $requiredPath. " +
            "Run scripts\release\build_windows_dist.ps1 first."
    }
}

# AI model is optional.
$AiDir = Join-Path `
    $DistDir `
    "ai"

if (-not (Test-Path -LiteralPath $AiDir)) {

    Write-Warning `
        "Distribution staging has no ai\ payload; the MSI will not bundle the AI model."

}

# ==============================================================================
# CLEAN STAGING
# ==============================================================================

if (Test-Path -LiteralPath $StageDir) {

    Remove-Item `
        -LiteralPath $StageDir `
        -Recurse `
        -Force
}

New-Item `
    -ItemType Directory `
    -Force `
    -Path $SourceDir |
    Out-Null

New-Item `
    -ItemType Directory `
    -Force `
    -Path $OutputDir |
    Out-Null

# ==============================================================================
# COPY DISTRIBUTION
# ==============================================================================

Copy-Item `
    -LiteralPath (Join-Path $DistDir "bin") `
    -Destination $SourceDir `
    -Recurse `
    -Force

Copy-Item `
    -LiteralPath (Join-Path $DistDir "std") `
    -Destination $SourceDir `
    -Recurse `
    -Force

Copy-Item `
    -LiteralPath (Join-Path $DistDir "config") `
    -Destination $SourceDir `
    -Recurse `
    -Force

Copy-Item `
    -LiteralPath (Join-Path $DistDir "licenses") `
    -Destination $SourceDir `
    -Recurse `
    -Force

if (Test-Path -LiteralPath $AiDir) {

    Copy-Item `
        -LiteralPath $AiDir `
        -Destination $SourceDir `
        -Recurse `
        -Force

}

# ==============================================================================
# LOCATE WIX
# ==============================================================================

$Wix = Get-Command `
    "wix.exe" `
    -ErrorAction SilentlyContinue

if (-not $Wix) {

    $Wix = Get-Command `
        "wix" `
        -ErrorAction SilentlyContinue

}

if (-not $Wix) {

    throw `
        "WiX CLI was not found on PATH. " +
        "Install the supported WiX Toolset version before running this script."

}

Write-Host "WiX executable: $($Wix.Source)"

# ==============================================================================
# ARCHITECTURE
# ==============================================================================

$WixArch = if ($Arch -eq "aarch64") {
    "arm64"
}
else {
    "x64"
}

Write-Host "WiX architecture: $WixArch"

# ==============================================================================
# PRODUCT.WXS
# ==============================================================================

$ProductWxs = Join-Path `
    $PSScriptRoot `
    "Product.wxs"

if (-not (Test-Path -LiteralPath $ProductWxs)) {

    throw `
        "Product.wxs was not found at: $ProductWxs"

}

# ==============================================================================
# BUILD
# ==============================================================================

Write-Host ""
Write-Host "========================================"
Write-Host "Building AdeshLang MSI"
Write-Host "========================================"
Write-Host "Version:      $Version"
Write-Host "Architecture: $Arch"
Write-Host "WiX arch:     $WixArch"
Write-Host "Source:       $SourceDir"
Write-Host "Output:       $MsiPath"
Write-Host "========================================"
Write-Host ""

# WiX v4/v5 CLI syntax:
#
#   wix build Product.wxs `
#       -arch x64 `
#       -d SourceDir=... `
#       -o output.msi
#
# Do NOT use:
#
#   -dSourceDir=...
#
# The define name and value are passed as separate arguments.

& $Wix.Source `
    build `
    $ProductWxs `
    "-arch" `
    $WixArch `
    "-d" `
    "SourceDir=$SourceDir" `
    "-o" `
    $MsiPath

if ($LASTEXITCODE -ne 0) {

    throw `
        "WiX failed to build the MSI (exit code $LASTEXITCODE)."

}

# ==============================================================================
# VERIFY
# ==============================================================================

if (-not (Test-Path -LiteralPath $MsiPath)) {

    throw `
        "WiX reported success but the MSI was not created: $MsiPath"

}

$MsiInfo = Get-Item `
    -LiteralPath $MsiPath

Write-Host ""
Write-Host "========================================"
Write-Host "MSI CREATED SUCCESSFULLY"
Write-Host "========================================"
Write-Host "Path: $($MsiInfo.FullName)"
Write-Host "Size: $($MsiInfo.Length) bytes"
Write-Host "========================================"Copy-Item (Join-Path $DistDir "bin") $SourceDir -Recurse -Force
Copy-Item (Join-Path $DistDir "std") $SourceDir -Recurse -Force
Copy-Item (Join-Path $DistDir "config") $SourceDir -Recurse -Force
Copy-Item (Join-Path $DistDir "licenses") $SourceDir -Recurse -Force
if (Test-Path (Join-Path $DistDir "ai")) {
    Copy-Item (Join-Path $DistDir "ai") $SourceDir -Recurse -Force
}

$Wix = Get-Command "wix.exe" -ErrorAction SilentlyContinue
if (-not $Wix) {
    throw "WiX v5 CLI (wix.exe) was not found on PATH. Install WiX Toolset v5 and retry."
}
$WixArch = if ($Arch -eq "aarch64") { "arm64" } else { "x64" }

Write-Host "Building $MsiPath..."
# WiX v4/v5 CLI rejects the WiX v3 attached form "-dName=Value"; the define
# must be passed as a separate "-d" switch followed by "Name=Value".
& $Wix.Source build (Join-Path $PSScriptRoot "Product.wxs") `
    "-arch" $WixArch `
    "-d" "SourceDir=$SourceDir" `
    "-o" $MsiPath
if ($LASTEXITCODE -ne 0) {
    throw "WiX failed to build the MSI (exit code $LASTEXITCODE)."
}
Write-Host "MSI created: $MsiPath"
