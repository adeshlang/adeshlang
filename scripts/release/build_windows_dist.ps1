# Script: build_windows_dist.ps1
# Automates the creation of AdeshLang Windows Distribution Assets (Portable ZIP + Inno Installer)
# Supports x86_64 and ARM64 (aarch64) Windows targets.
#
# The default AI model (adesh-coder-0.5b-q4_0.gguf, ~275 MB) is bundled into
# the distribution under ai\models\. It is taken from the local ai\models\
# checkout when present; otherwise it is downloaded from the URL pinned in
# ai\models\manifest.json and verified against that manifest's SHA-256. Set
# -SkipAiModel (or ADESH_SKIP_AI_MODEL=1) to build a model-less distribution.

param(
    [ValidateSet("x86_64", "arm64", "aarch64")]
    [string]$Arch = "x86_64",
    [switch]$SkipAiModel
)

$ErrorActionPreference = "Stop"

if ($Arch -eq "arm64") { $Arch = "aarch64" }
if ($env:ADESH_SKIP_AI_MODEL -eq "1") { $SkipAiModel = $true }

Write-Host "====================================================" -ForegroundColor Cyan
Write-Host " Building AdeshLang Windows SDK Distribution ($Arch)" -ForegroundColor Cyan
Write-Host "====================================================" -ForegroundColor Cyan

$RootDir = Resolve-Path "$PSScriptRoot\..\.."
$DistDir = "$RootDir\dist\windows-$Arch"

if ($Arch -eq "aarch64") {
    $TargetFlag = @("--target", "aarch64-pc-windows-msvc")
    $TargetBinDir = "$RootDir\target\aarch64-pc-windows-msvc\release"
} else {
    $TargetFlag = @()
    $TargetBinDir = "$RootDir\target\release"
}

$TargetBinAdesh = "$TargetBinDir\adesh.exe"

# 1. Build the official `adesh` CLI binary and libraries (static & dynamic).
Write-Host "[1/6] Compiling AdeshLang Release Binaries & Libraries ($Arch)..." -ForegroundColor Yellow
Set-Location $RootDir
cargo build --release @TargetFlag --bin adesh --bin adl --lib
if (Test-Path "$RootDir\als\Cargo.toml") {
    Push-Location "$RootDir\als"
    cargo build --release @TargetFlag --bin als
    Pop-Location
}
if (Test-Path "$RootDir\editor\Cargo.toml") {
    Push-Location "$RootDir\editor"
    cargo build --release @TargetFlag --bin adesh-editor
    Pop-Location
}

# 2. Recreate Staging Directory Structure
Write-Host "[2/6] Preparing Staging Directory Structure..." -ForegroundColor Yellow
if (Test-Path $DistDir) {
    Remove-Item -Recurse -Force $DistDir
}

New-Item -ItemType Directory -Force -Path "$DistDir\bin" | Out-Null
New-Item -ItemType Directory -Force -Path "$DistDir\lib" | Out-Null
New-Item -ItemType Directory -Force -Path "$DistDir\include" | Out-Null
New-Item -ItemType Directory -Force -Path "$DistDir\licenses" | Out-Null
New-Item -ItemType Directory -Force -Path "$DistDir\config" | Out-Null
New-Item -ItemType Directory -Force -Path "$DistDir\std" | Out-Null

# 3. Copy Executables, Dynamic & Static Libraries, Standard Library, Licenses, and Toolchain Manifest
Write-Host "[3/6] Copying Core Binaries, Dynamic & Static Libraries, and Manifest..." -ForegroundColor Yellow
Copy-Item $TargetBinAdesh "$DistDir\bin\adesh.exe" -Force
if (Test-Path "$TargetBinDir\adl.exe") { Copy-Item "$TargetBinDir\adl.exe" "$DistDir\bin\adl.exe" -Force }

# Dynamic libraries (.dll)
if (Test-Path "$TargetBinDir\adeshlang.dll") {
    Copy-Item "$TargetBinDir\adeshlang.dll" "$DistDir\bin\adeshlang.dll" -Force
    Copy-Item "$TargetBinDir\adeshlang.dll" "$DistDir\lib\adeshlang.dll" -Force
    Write-Host "      Copied dynamic library adeshlang.dll to bin\ and lib\" -ForegroundColor Green
}

# Static and Import libraries (.lib, .dll.lib, .a)
$LibFiles = @(Get-ChildItem "$TargetBinDir" -Include "adeshlang.lib", "adeshlang.dll.lib", "libadeshlang.a", "*.lib" -File)
if ($LibFiles.Count -eq 0 -and (Test-Path "$TargetBinDir\deps")) {
    $LibFiles = @(Get-ChildItem "$TargetBinDir\deps" -Filter "*adeshlang*.lib" -File)
}
$LibFiles | ForEach-Object {
    Copy-Item $_.FullName "$DistDir\lib\$($_.Name)" -Force
    Write-Host "      Copied library $($_.Name) to lib\" -ForegroundColor Green
}

$AlsBin = if ($Arch -eq "aarch64") { "$RootDir\als\target\aarch64-pc-windows-msvc\release\als.exe" } else { "$RootDir\als\target\release\als.exe" }
if (-not (Test-Path $AlsBin)) { $AlsBin = "$TargetBinDir\als.exe" }
if (Test-Path $AlsBin) { Copy-Item $AlsBin "$DistDir\bin\als.exe" -Force }

$EditorBin = if ($Arch -eq "aarch64") { "$RootDir\editor\target\aarch64-pc-windows-msvc\release\adesh-editor.exe" } else { "$RootDir\editor\target\release\adesh-editor.exe" }
if (-not (Test-Path $EditorBin)) { $EditorBin = "$TargetBinDir\adesh-editor.exe" }
if (Test-Path $EditorBin) { Copy-Item $EditorBin "$DistDir\bin\adesh-editor.exe" -Force }

Copy-Item "$RootDir\LICENSE" "$DistDir\licenses\LICENSE.txt" -Force
Get-ChildItem "$RootDir\src\stdlib\*.adesh" -File | ForEach-Object {
    Copy-Item $_.FullName "$DistDir\std\$($_.Name)" -Force
}

Copy-Item "$RootDir\installer\manifests\toolchain-manifest.json" "$DistDir\config\toolchain-manifest.json" -Force
if (-not (Test-Path "$DistDir\std\*.adesh")) {
    Write-Warning "No flat stdlib .adesh sources were found in src\stdlib."
}

# 3b. Bundle the default AI model (adesh-coder-0.5b-q4_0) and the server
# deployment configs. Larger quantizations stay opt-in via `adesh ai setup`.
if ($SkipAiModel) {
    Write-Warning "Skipping the bundled AI model (-SkipAiModel); `adesh ai setup` can fetch it later."
} else {
    Write-Host "[3b/6] Staging the bundled AI model..." -ForegroundColor Yellow
    $AiManifestPath = "$RootDir\ai\models\manifest.json"
    if (-not (Test-Path $AiManifestPath)) {
        throw "ai\models\manifest.json is missing; it pins the AI model URL and SHA-256. Re-run without -SkipAiModel only after adding it."
    }
    $AiManifest = Get-Content $AiManifestPath -Raw | ConvertFrom-Json
    $AiArtifact = @($AiManifest.artifacts) |
        Where-Object { $_.quantization -eq "Q4_0" } |
        Select-Object -First 1
    if (-not $AiArtifact) {
        throw "ai\models\manifest.json does not describe a Q4_0 artifact."
    }
    $ModelName = $AiArtifact.filename
    $LocalModel = "$RootDir\ai\models\$ModelName"

    if (-not (Test-Path $LocalModel)) {
        Write-Host "      Local $ModelName not found; downloading from the manifest URL..." -ForegroundColor Yellow
        $OldProgress = $ProgressPreference
        $ProgressPreference = "SilentlyContinue"
        try {
            Invoke-WebRequest -UseBasicParsing -Uri $AiArtifact.url -OutFile $LocalModel
        } catch {
            throw "Could not download the AI model from $($AiArtifact.url): $($_.Exception.Message)"
        } finally {
            $ProgressPreference = $OldProgress
        }
    }

    $ActualHash = (Get-FileHash -LiteralPath $LocalModel -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($ActualHash -ne $AiArtifact.sha256.ToLowerInvariant()) {
        throw "SHA-256 mismatch for $ModelName (expected $($AiArtifact.sha256), got $ActualHash). Delete the file and retry."
    }
    Write-Host "      Model verified against manifest (SHA-256 ok)." -ForegroundColor Green

    New-Item -ItemType Directory -Force -Path "$DistDir\ai\models", "$DistDir\ai\deploy\ollama", "$DistDir\ai\deploy\openrouter" | Out-Null
    Copy-Item $LocalModel "$DistDir\ai\models\$ModelName" -Force
    Copy-Item $AiManifestPath "$DistDir\ai\models\manifest.json" -Force
    if (Test-Path "$RootDir\ai\deploy\ollama\Modelfile") {
        Copy-Item "$RootDir\ai\deploy\ollama\Modelfile" "$DistDir\ai\deploy\ollama\Modelfile" -Force
    }
    if (Test-Path "$RootDir\ai\deploy\openrouter\config.json") {
        Copy-Item "$RootDir\ai\deploy\openrouter\config.json" "$DistDir\ai\deploy\openrouter\config.json" -Force
    }
}

# 4. Generate Portable ZIP Distribution Package
$CargoToml = Get-Content "$RootDir\Cargo.toml" -Raw
if ($CargoToml -match '(?m)^version\s*=\s*"([^"]+)"') {
    $Version = $Matches[1]
} else {
    $Version = "0.3.0"
}
Write-Host "[4/6] Packaging AdeshLang-$Version-$Arch-windows-portable.zip..." -ForegroundColor Yellow
$ZipPath = "$RootDir\dist\AdeshLang-$Version-$Arch-windows-portable.zip"
if (Test-Path $ZipPath) { Remove-Item $ZipPath -Force }
Compress-Archive -Path "$DistDir\*" -DestinationPath $ZipPath -Force
Write-Host "      Portable package created at: $ZipPath" -ForegroundColor Green

# 5. Build GUI Installer Executable if ISCC.exe is available (x86_64)
if ($Arch -eq "x86_64") {
    Write-Host "[5/6] Checking Inno Setup Compiler (ISCC.exe)..." -ForegroundColor Yellow
    $IsccCmd = Get-Command "iscc.exe" -ErrorAction SilentlyContinue
    $IsccPath = if ($IsccCmd) { $IsccCmd.Source } elseif (Test-Path "C:\Program Files (x86)\Inno Setup 6\ISCC.exe") { "C:\Program Files (x86)\Inno Setup 6\ISCC.exe" } else { $null }

    if ($IsccPath) {
        Write-Host "      Invoking Inno Setup Compiler ($IsccPath)..." -ForegroundColor Green
        try {
            & $IsccPath "$RootDir\installer\windows\installer.iss"
            if ($LASTEXITCODE -ne 0) {
                Write-Warning "Inno Setup Compiler failed with exit code $LASTEXITCODE; continuing with portable distribution."
            }
        } catch {
            Write-Warning "Inno Setup Compiler could not compile the installer: $($_.Exception.Message)"
        }
    } else {
        Write-Host "      (ISCC.exe not found on PATH or Program Files. Installer compilation skipped. Install Inno Setup to produce GUI installer)" -ForegroundColor DarkYellow
    }
}

Write-Host "====================================================" -ForegroundColor Cyan
Write-Host " Distribution Build Complete for $Arch!" -ForegroundColor Cyan
Write-Host "====================================================" -ForegroundColor Cyan
