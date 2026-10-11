# Build the WiX Burn bootstrapper with the WiX v5 CLI (WiX v4 is compatible).
# The bundle chains only the self-contained AdeshLang MSI: no toolchain
# download, no VS Build Tools, no extra payloads at install time.

[CmdletBinding()]
param(
    [ValidateSet("x86_64", "arm64", "aarch64")]
    [string]$Arch = "x86_64",
    [string]$MsiPath,
    [string]$OutputDir
)

$ErrorActionPreference = "Stop"
if ($Arch -eq "arm64") { $Arch = "aarch64" }

$RootDir = (Resolve-Path (Join-Path $PSScriptRoot "..\..\..")).Path
$DistDir = Join-Path $RootDir "dist\windows-$Arch"

$cargoToml = Join-Path $RootDir "Cargo.toml"
$Version = "0.3.0"
if (Test-Path -LiteralPath $cargoToml) {
    $versionLine = Get-Content -LiteralPath $cargoToml | Where-Object { $_ -match '^version\s*=\s*"([^"]+)"' } | Select-Object -First 1
    if ($versionLine -and $versionLine -match '^version\s*=\s*"([^"]+)"') {
        $Version = $Matches[1]
    }
}

if (-not $MsiPath) {
    $CandidateMsi = Join-Path $DistDir "AdeshLang-$Version-$Arch-windows.msi"
    if (Test-Path $CandidateMsi) {
        $MsiPath = $CandidateMsi
    } else {
        $FoundMsi = Get-ChildItem -Path $DistDir, (Join-Path $RootDir "dist") -Filter "AdeshLang-*-$Arch-windows.msi" -File -ErrorAction SilentlyContinue | Select-Object -First 1
        if ($FoundMsi) {
            $MsiPath = $FoundMsi.FullName
        } else {
            $MsiPath = $CandidateMsi
        }
    }
}
if (-not $OutputDir) {
    $OutputDir = $DistDir
}
$OutputDir = [System.IO.Path]::GetFullPath($OutputDir)
$BundlePath = Join-Path $OutputDir "AdeshLang-$Version-$Arch-windows-bootstrapper.exe"

if (-not (Test-Path $MsiPath)) {
    throw "MSI not found at $MsiPath. Run installer\windows\msi\build-msi.ps1 first."
}
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

$Wix = Get-Command "wix.exe" -ErrorAction SilentlyContinue
if (-not $Wix) {
    throw "WiX v5 CLI (wix.exe) was not found on PATH. Install WiX Toolset v5 and retry."
}
$WixArch = if ($Arch -eq "aarch64") { "arm64" } else { "x64" }

# Resolve the Burn (Bal) extension. The official WixToolset.Bal.wixext 5.0.2
# package ships its assembly as WixToolset.BootstrapperApplications.wixext.dll,
# so the ID-based "-ext WixToolset.Bal.wixext" lookup fails (WiX looks for
# WixToolset.Bal.wixext.dll inside the cache). Pass the actual DLL path.
$BalRoot = Join-Path $env:USERPROFILE ".wix\extensions\WixToolset.Bal.wixext"
$BalDll = $null
if (Test-Path $BalRoot) {
    $Candidate = Get-ChildItem -Path $BalRoot -Recurse -Filter "WixToolset.Bal.wixext.dll" -ErrorAction SilentlyContinue | Select-Object -First 1
    if (-not $Candidate) {
        $Candidate = Get-ChildItem -Path $BalRoot -Recurse -Filter "*.wixext.dll" -ErrorAction SilentlyContinue | Select-Object -First 1
    }
    if ($Candidate) { $BalDll = $Candidate.FullName }
}
if (-not $BalDll) {
    & $Wix.Source extension add -g "WixToolset.Bal.wixext/5.0.2" | Out-Null
    $Candidate = Get-ChildItem -Path $BalRoot -Recurse -Filter "*.wixext.dll" -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($Candidate) { $BalDll = $Candidate.FullName } else { throw "Could not resolve the WixToolset.Bal.wixext extension DLL." }
}

Write-Host "Building $BundlePath..."
# WiX v4/v5 CLI rejects the WiX v3 attached form "-dName=Value"; each define
# must be passed as a separate "-d" switch followed by "Name=Value".
& $Wix.Source build (Join-Path $PSScriptRoot "Bundle.wxs") `
    "-arch" $WixArch `
    "-ext" $BalDll `
    "-d" "MsiPath=$MsiPath" `
    "-o" $BundlePath
if ($LASTEXITCODE -ne 0) {
    throw "WiX failed to build the Burn bootstrapper (exit code $LASTEXITCODE)."
}
Write-Host "Bootstrapper created: $BundlePath"

# Ensure the bootstrapper is available directly in dist\
$DistRoot = Join-Path $RootDir "dist"
if (Test-Path -LiteralPath $DistRoot) {
    Copy-Item -LiteralPath $BundlePath -Destination (Join-Path $DistRoot (Split-Path $BundlePath -Leaf)) -Force
}
