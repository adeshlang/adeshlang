<#
.SYNOPSIS
    Packages AdeshLang release components and generates the release manifest (stable.json).

.DESCRIPTION
    Creates versioned zip packages for each component (compiler, stdlib, runtime),
    calculates SHA-256 hashes and file sizes, and generates a release manifest
    ready for hosting on the website or Cloudflare R2 / GitHub Pages.

.PARAMETER Version
    The release version (e.g. 0.3.0). Defaults to Cargo.toml version.

.PARAMETER OutputDir
    Output directory for archives and manifest. Defaults to website/releases.
#>

[CmdletBinding()]
param(
    [string]$Version,
    [string]$OutputDir = "$PSScriptRoot\..\..\website\releases"
)

$ErrorActionPreference = "Stop"
$WorkspaceRoot = (Resolve-Path "$PSScriptRoot\..\..").Path

if (-not $Version) {
    # Extract version from Cargo.toml
    $cargoToml = Get-Content "$WorkspaceRoot\Cargo.toml" -Raw
    if ($cargoToml -match 'version\s*=\s*"([^"]+)"') {
        $Version = $matches[1]
    } else {
        $Version = "0.3.0"
    }
}

$OutputDir = (Resolve-Path $OutputDir -ErrorAction SilentlyContinue)
if (-not $OutputDir) {
    $OutputDir = Join-Path $WorkspaceRoot "website\releases"
}

$VersionDir = Join-Path $OutputDir "stable\$Version"
if (-not (Test-Path $VersionDir)) {
    New-Item -ItemType Directory -Path $VersionDir -Force | Out-Null
}

Write-Host "──────────────────────────────────────────────────────" -ForegroundColor Cyan
Write-Host "  AdeshLang Release Manifest Generator" -ForegroundColor Green
Write-Host "──────────────────────────────────────────────────────" -ForegroundColor Cyan
Write-Host "  Version    : $Version"
Write-Host "  Output Dir : $VersionDir"
Write-Host ""

# Helper for SHA-256 calculation
function Get-Sha256($filePath) {
    $hash = Get-FileHash -Path $filePath -Algorithm SHA256
    return $hash.Hash.ToLower()
}

$Components = @{}

# 1. Package Compiler Binaries
$CompilerZip = Join-Path $VersionDir "compiler-windows-x86_64.zip"
$CompilerStaging = Join-Path $env:TEMP "adesh_comp_staging_$Version"
if (Test-Path $CompilerStaging) { Remove-Item -Recurse -Force $CompilerStaging }
New-Item -ItemType Directory -Path $CompilerStaging -Force | Out-Null

$ReleaseBinDir = Join-Path $WorkspaceRoot "target\release"
foreach ($bin in @("adesh.exe", "adl.exe", "adesh-editor.exe")) {
    $src = Join-Path $ReleaseBinDir $bin
    if (Test-Path $src) {
        Copy-Item -Path $src -Destination $CompilerStaging -Force
    }
}

if (Test-Path $CompilerZip) { Remove-Item -Force $CompilerZip }
Compress-Archive -Path "$CompilerStaging\*" -DestinationPath $CompilerZip -CompressionLevel Optimal
Remove-Item -Recurse -Force $CompilerStaging

$compSize = (Get-Item $CompilerZip).Length
$compHash = Get-Sha256 $CompilerZip

$Components["compiler"] = @{
    version     = $Version
    size        = $compSize
    sha256      = $compHash
    url         = "https://downloads.adeshlang.org/releases/stable/$Version/compiler-windows-x86_64.zip"
    target_os   = "windows"
    target_arch = "x86_64"
}
Write-Host "  ✓ Packaged compiler: $([math]::Round($compSize / 1MB, 2)) MB (SHA256: $compHash)" -ForegroundColor Green

# 2. Package Standard Library
$StdlibZip = Join-Path $VersionDir "stdlib.zip"
$StdSrc = Join-Path $WorkspaceRoot "std"

if (Test-Path $StdlibZip) { Remove-Item -Force $StdlibZip }
Compress-Archive -Path "$StdSrc\*" -DestinationPath $StdlibZip -CompressionLevel Optimal

$stdSize = (Get-Item $StdlibZip).Length
$stdHash = Get-Sha256 $StdlibZip

$Components["stdlib"] = @{
    version = $Version
    size    = $stdSize
    sha256  = $stdHash
    url     = "https://downloads.adeshlang.org/releases/stable/$Version/stdlib.zip"
}
Write-Host "  ✓ Packaged stdlib:   $([math]::Round($stdSize / 1MB, 2)) MB (SHA256: $stdHash)" -ForegroundColor Green

# 3. Package Static Runtime Library
$RuntimeZip = Join-Path $VersionDir "runtime-windows-x86_64.zip"
$RuntimeStaging = Join-Path $env:TEMP "adesh_rt_staging_$Version"
if (Test-Path $RuntimeStaging) { Remove-Item -Recurse -Force $RuntimeStaging }
New-Item -ItemType Directory -Path $RuntimeStaging -Force | Out-Null

$LibFile = Join-Path $ReleaseBinDir "adeshlang.lib"
if (Test-Path $LibFile) {
    Copy-Item -Path $LibFile -Destination $RuntimeStaging -Force
}

if (Test-Path $RuntimeZip) { Remove-Item -Force $RuntimeZip }
Compress-Archive -Path "$RuntimeStaging\*" -DestinationPath $RuntimeZip -CompressionLevel Optimal
Remove-Item -Recurse -Force $RuntimeStaging

$rtSize = (Get-Item $RuntimeZip).Length
$rtHash = Get-Sha256 $RuntimeZip

$Components["runtime"] = @{
    version     = $Version
    size        = $rtSize
    sha256      = $rtHash
    url         = "https://downloads.adeshlang.org/releases/stable/$Version/runtime-windows-x86_64.zip"
    target_os   = "windows"
    target_arch = "x86_64"
}
Write-Host "  ✓ Packaged runtime:  $([math]::Round($rtSize / 1MB, 2)) MB (SHA256: $rtHash)" -ForegroundColor Green

# 4. Generate stable.json Release Manifest
$Manifest = @{
    version             = $Version
    channel             = "stable"
    released_at         = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ")
    min_updater_version = "0.3.0"
    components          = $Components
}

$ManifestJson = $Manifest | ConvertTo-Json -Depth 5
$ManifestPath = Join-Path $OutputDir "stable.json"
Set-Content -Path $ManifestPath -Value $ManifestJson -Encoding UTF8

Write-Host ""
Write-Host "  ✓ Generated manifest at $ManifestPath" -ForegroundColor Green
Write-Host "──────────────────────────────────────────────────────" -ForegroundColor Cyan
