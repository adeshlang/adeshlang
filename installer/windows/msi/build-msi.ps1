# Build the enterprise MSI with WiX v5 (the v4 CLI is compatible).
# WiX v3 fallback: install WiX v3, replace the `wix build` invocation with
# `candle.exe Product.wxs` followed by `light.exe Product.wixobj -out ...msi`,
# and adapt the v4 namespace/harvesting syntax if the v3 toolset is used.

[CmdletBinding()]
param(
    [ValidateSet("x86_64", "arm64", "aarch64")]
    [string]$Arch = "x86_64",
    [string]$OutputDir
)

$ErrorActionPreference = "Stop"
if ($Arch -eq "arm64") { $Arch = "aarch64" }

$RootDir = (Resolve-Path (Join-Path $PSScriptRoot "..\..\..")).Path
$DistDir = Join-Path $RootDir "dist\windows-$Arch"
if (-not $OutputDir) {
    $OutputDir = Join-Path $RootDir "dist\windows-$Arch"
}
$OutputDir = [System.IO.Path]::GetFullPath($OutputDir)
$StageDir = Join-Path $PSScriptRoot "staging"
$SourceDir = Join-Path $StageDir "windows-$Arch"
$MsiPath = Join-Path $OutputDir "AdeshLang-0.3.0-$Arch-windows.msi"

if (-not (Test-Path (Join-Path $DistDir "bin\adesh.exe"))) {
    throw "Distribution staging is missing $DistDir\bin\adesh.exe. Run scripts\release\build_windows_dist.ps1 first."
}
if (-not (Test-Path (Join-Path $DistDir "ai"))) {
    Write-Warning "Distribution staging has no ai\ payload; the MSI will not bundle the AI model. Re-run build_windows_dist.ps1 (without -SkipAiModel)."
}

if (Test-Path $StageDir) {
    Remove-Item -LiteralPath $StageDir -Recurse -Force
}
New-Item -ItemType Directory -Force -Path $SourceDir, $OutputDir | Out-Null
Copy-Item (Join-Path $DistDir "bin") $SourceDir -Recurse -Force
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
