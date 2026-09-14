# Build the WiX Burn bootstrapper with the WiX v5 CLI (WiX v4 is compatible).
# The MSI and adesh.exe are payloads; the Visual Studio Build Tools package is
# downloaded by Burn only when VSBuildToolsInstall is enabled.

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
if (-not $MsiPath) {
    $MsiPath = Join-Path $DistDir "AdeshLang-0.3.0-$Arch-windows.msi"
}
if (-not $OutputDir) {
    $OutputDir = $DistDir
}
$OutputDir = [System.IO.Path]::GetFullPath($OutputDir)
$BundlePath = Join-Path $OutputDir "AdeshLang-0.3.0-$Arch-windows-bootstrapper.exe"

$AdeshExePath = Join-Path $DistDir "bin\adesh.exe"
if (-not (Test-Path $MsiPath)) {
    throw "MSI not found at $MsiPath. Run installer\windows\msi\build-msi.ps1 first."
}
if (-not (Test-Path $AdeshExePath)) {
    throw "adesh.exe not found at $AdeshExePath. Run scripts\release\build_windows_dist.ps1 first."
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
$UtilRoot = Join-Path $env:USERPROFILE ".wix\extensions\WixToolset.Util.wixext"
$UtilDll = $null
if (Test-Path $UtilRoot) {
    $Candidate = Get-ChildItem -Path $UtilRoot -Recurse -Filter "WixToolset.Util.wixext.dll" -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($Candidate) { $UtilDll = $Candidate.FullName }
}
if (-not $UtilDll) {
    & $Wix.Source extension add -g "WixToolset.Util.wixext/5.0.2" | Out-Null
    $Candidate = Get-ChildItem -Path $UtilRoot -Recurse -Filter "WixToolset.Util.wixext.dll" -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($Candidate) { $UtilDll = $Candidate.FullName } else { throw "Could not resolve the WixToolset.Util.wixext extension DLL." }
}

# The VS Build Tools web bootstrapper must exist at build time so Burn can
# embed and hash it. It is the small official stub (~2 MB); the heavy VS
# payload is only downloaded when the package actually runs.
$VsBuildToolsPath = Join-Path $PSScriptRoot "staging\vs_buildtools.exe"
$VsBuildToolsUrl = "https://aka.ms/vs/17/release/vs_buildtools.exe"
if (-not (Test-Path $VsBuildToolsPath) -or (Get-Item $VsBuildToolsPath).Length -lt 1MB) {
    New-Item -ItemType Directory -Force -Path (Split-Path $VsBuildToolsPath) | Out-Null
    Write-Host "Downloading the Visual Studio Build Tools web bootstrapper..."
    try {
        $OldProgress = $ProgressPreference
        $ProgressPreference = "SilentlyContinue"
        Invoke-WebRequest -UseBasicParsing -Uri $VsBuildToolsUrl -OutFile $VsBuildToolsPath
        $ProgressPreference = $OldProgress
    } catch {
        throw "Could not download $VsBuildToolsUrl. Download it manually to $VsBuildToolsPath and retry."
    }
}

Write-Host "Building $BundlePath..."
# WiX v4/v5 CLI rejects the WiX v3 attached form "-dName=Value"; each define
# must be passed as a separate "-d" switch followed by "Name=Value".
& $Wix.Source build (Join-Path $PSScriptRoot "Bundle.wxs") `
    "-arch" $WixArch `
    "-ext" $BalDll `
    "-ext" $UtilDll `
    "-d" "MsiPath=$MsiPath" `
    "-d" "AdeshExePath=$AdeshExePath" `
    "-d" "VsBuildToolsPath=$VsBuildToolsPath" `
    "-o" $BundlePath
if ($LASTEXITCODE -ne 0) {
    throw "WiX failed to build the Burn bootstrapper (exit code $LASTEXITCODE)."
}
Write-Host "Bootstrapper created: $BundlePath"
