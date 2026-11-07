# Script: build_windows_dist.ps1
# Automates the creation of AdeshLang Windows Distribution Assets (Portable ZIP + Inno Installer)
# Supports x86_64 and ARM64 (aarch64) Windows targets.
#
# The distribution is SELF-CONTAINED: it bundles the Adesh native toolchain
# (native codegen + ADOB object format + adeshlink native linker + adeshlang
# runtime library + stdlib). No external LLVM, Clang, GCC, or MSVC install is
# required at setup or build time; external LLVM is an opt-in bridge
# (`adesh build --external-linker` / `adesh gpu-check --external-linker`).
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

function Invoke-ReleaseBuild {
    & cargo build --release @args
    if ($LASTEXITCODE -ne 0) {
        throw "Cargo release build failed with exit code $LASTEXITCODE; distribution staging was not started."
    }
}

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

# 1. Build the official `adesh` CLI, the native `adeshlink` linker CLI, and
#    libraries (static & dynamic).
Write-Host "[1/6] Compiling AdeshLang Release Binaries, Native Linker & Libraries ($Arch)..." -ForegroundColor Yellow
Set-Location $RootDir
Invoke-ReleaseBuild @TargetFlag --bin adesh --bin adl --lib
# adeshlink: standalone native linker & binary-tools CLI (link, ar, nm,
# objdump, readobj, size, strip). Bundled so users get the full native
# toolchain without any external LLVM/binutils.
Invoke-ReleaseBuild @TargetFlag -p adesh-linker --bin adeshlink
# adesh-runtime: standalone native runtime static library (adesh_runtime.lib)
Invoke-ReleaseBuild @TargetFlag -p adesh-runtime
if (Test-Path "$RootDir\als\Cargo.toml") {
    Push-Location "$RootDir\als"
    Invoke-ReleaseBuild @TargetFlag --bin als
    Pop-Location
}
if (Test-Path "$RootDir\editor\Cargo.toml") {
    Push-Location "$RootDir\editor"
    Invoke-ReleaseBuild @TargetFlag --bin adesh-editor
    Pop-Location
}
# adesh-monitor-cli: standalone observatory CLI
Invoke-ReleaseBuild @TargetFlag -p adesh-monitor-cli --bin aro
# adesh-monitor-tui: standalone observatory TUI
Invoke-ReleaseBuild @TargetFlag -p adesh-monitor-tui --bin aro_tui
# adesh-observatory-desktop: standalone Tauri desktop application
if (Test-Path "$RootDir\ARO\desktop") {
    Push-Location "$RootDir\ARO\desktop"
    if (Get-Command pnpm -ErrorAction SilentlyContinue) {
        if (-not (Test-Path "node_modules")) {
            pnpm install --frozen-lockfile
            if ($LASTEXITCODE -ne 0) { throw "Desktop dependency installation failed." }
        }
        pnpm run build
        if ($LASTEXITCODE -ne 0) { throw "Desktop frontend build failed." }
    }
    Pop-Location
    Push-Location "$RootDir\ARO\desktop\src-tauri"
    Invoke-ReleaseBuild @TargetFlag --bin adesh-observatory-desktop
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
Write-Host "[3/6] Copying Core Binaries, Native Toolchain, Dynamic & Static Libraries, and Manifest..." -ForegroundColor Yellow
Copy-Item $TargetBinAdesh "$DistDir\bin\adesh.exe" -Force
if (Test-Path "$TargetBinDir\adl.exe") { Copy-Item "$TargetBinDir\adl.exe" "$DistDir\bin\adl.exe" -Force }

# Native linker CLI (adeshlink): link, ar, nm, objdump, readobj, size, strip.
# Built from the workspace root, so artifacts land in the shared target dir;
# linker\target is checked as a fallback for standalone linker builds.
$AdeshLinkBin = "$TargetBinDir\adeshlink.exe"
if (-not (Test-Path $AdeshLinkBin)) {
    $Fallback = if ($Arch -eq "aarch64") { "$RootDir\linker\target\aarch64-pc-windows-msvc\release\adeshlink.exe" } else { "$RootDir\linker\target\release\adeshlink.exe" }
    if (Test-Path $Fallback) { $AdeshLinkBin = $Fallback }
}
if (Test-Path $AdeshLinkBin) {
    Copy-Item $AdeshLinkBin "$DistDir\bin\adeshlink.exe" -Force
    Write-Host "      Copied native linker adeshlink.exe to bin\" -ForegroundColor Green
} else {
    throw "adeshlink.exe was not built; the distribution must bundle the native linker. Re-run cargo build -p adesh-linker --bin adeshlink."
}

# Dynamic libraries (.dll)
Get-ChildItem "$TargetBinDir" -Filter "*.dll" -File -ErrorAction SilentlyContinue | ForEach-Object {
    Copy-Item $_.FullName "$DistDir\bin\$($_.Name)" -Force
    Copy-Item $_.FullName "$DistDir\lib\$($_.Name)" -Force
    Write-Host "      Copied dynamic library $($_.Name) to bin\ and lib\" -ForegroundColor Green
}

# Static, runtime and Import libraries (.lib, .dll.lib, .a)
# `-Include` only filters when the -Path carries a wildcard, so the patterns are
# paired with "$dir\*"; written as a plain directory path they match nothing and
# the distribution ships no library payload at all.
$LibFiles = @(Get-ChildItem -Path "$TargetBinDir\*" -Include "*.lib", "*.a" -File -ErrorAction SilentlyContinue)
if (Test-Path "$TargetBinDir\deps") {
    # The runtime archive is staged canonically below (`adesh_runtime.lib` in
    # lib\ and in lib\<triple>\); its hashed `deps\` twin is the same 20 MB
    # twice over, so it is filtered out of the general copy.
    $DepsLibs = @(Get-ChildItem -Path "$TargetBinDir\deps\*" -Include "*adesh*.lib", "*adesh*.a" -File -ErrorAction SilentlyContinue |
        Where-Object { $_.Name -notlike "adesh_runtime-*" -and $_.Name -notlike "libadesh_runtime-*" })
    $LibFiles += $DepsLibs
}
$LibFiles | ForEach-Object {
    Copy-Item $_.FullName "$DistDir\lib\$($_.Name)" -Force
    Write-Host "      Copied library $($_.Name) to lib\" -ForegroundColor Green
}

# The runtime archive an `adesh build --target` link consumes is named by the
# triple it was built for, so it also gets a triple directory: resolution looks
# in lib\<triple>\ before the flat lib\, and the flat copy only ever satisfies a
# build whose target is this machine.
$RuntimeTriple = "$Arch-pc-windows-msvc"
$RuntimeArchive = @("$TargetBinDir\adesh_runtime.lib", "$TargetBinDir\libadesh_runtime.a") |
    Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $RuntimeArchive -and (Test-Path "$TargetBinDir\deps")) {
    $RuntimeArchive = Get-ChildItem "$TargetBinDir\deps\adesh_runtime-*.lib", "$TargetBinDir\deps\libadesh_runtime-*.a" -File -ErrorAction SilentlyContinue |
        Select-Object -First 1
    if ($RuntimeArchive) { $RuntimeArchive = $RuntimeArchive.FullName }
}
if ($RuntimeArchive) {
    New-Item -ItemType Directory -Force -Path "$DistDir\lib\$RuntimeTriple" | Out-Null
    Copy-Item $RuntimeArchive "$DistDir\lib\$RuntimeTriple\$(Split-Path -Leaf $RuntimeArchive)" -Force
    Write-Host "      Runtime archive staged to lib\$RuntimeTriple\" -ForegroundColor Green
}

$AlsBin = if ($Arch -eq "aarch64") { "$RootDir\als\target\aarch64-pc-windows-msvc\release\als.exe" } else { "$RootDir\als\target\release\als.exe" }
if (-not (Test-Path $AlsBin)) { $AlsBin = "$TargetBinDir\als.exe" }
if (Test-Path $AlsBin) { Copy-Item $AlsBin "$DistDir\bin\als.exe" -Force }

$EditorBin = if ($Arch -eq "aarch64") { "$RootDir\editor\target\aarch64-pc-windows-msvc\release\adesh-editor.exe" } else { "$RootDir\editor\target\release\adesh-editor.exe" }
if (-not (Test-Path $EditorBin)) { $EditorBin = "$TargetBinDir\adesh-editor.exe" }
if (Test-Path $EditorBin) { Copy-Item $EditorBin "$DistDir\bin\adesh-editor.exe" -Force }

# Adesh Runtime Observatory (ARO) suite: CLI, TUI, and Desktop Cockpit
if (Test-Path "$TargetBinDir\aro.exe") {
    Copy-Item "$TargetBinDir\aro.exe" "$DistDir\bin\aro.exe" -Force
    Write-Host "      Copied ARO CLI (aro.exe) to bin\" -ForegroundColor Green
}
if (Test-Path "$TargetBinDir\aro_tui.exe") {
    Copy-Item "$TargetBinDir\aro_tui.exe" "$DistDir\bin\aro_tui.exe" -Force
    Copy-Item "$TargetBinDir\aro_tui.exe" "$DistDir\bin\aro-tui.exe" -Force
    Write-Host "      Copied ARO TUI (aro_tui.exe / aro-tui.exe) to bin\" -ForegroundColor Green
}
$AroDesktopBin = "$RootDir\ARO\desktop\src-tauri\target\release\adesh-observatory-desktop.exe"
if ($Arch -eq "aarch64") {
    $AroDesktopBin = "$RootDir\ARO\desktop\src-tauri\target\aarch64-pc-windows-msvc\release\adesh-observatory-desktop.exe"
}
if (Test-Path $AroDesktopBin) {
    Copy-Item $AroDesktopBin "$DistDir\bin\adesh-observatory-desktop.exe" -Force
    Write-Host "      Copied ARO Desktop Cockpit (adesh-observatory-desktop.exe) to bin\" -ForegroundColor Green
}

Copy-Item "$RootDir\LICENSE" "$DistDir\licenses\LICENSE.txt" -Force

# Standard library: copy both .adesh files and full stdlib hierarchy recursively
if (Test-Path "$RootDir\src\stdlib") {
    Copy-Item "$RootDir\src\stdlib\*" "$DistDir\std\" -Recurse -Force
}
if (Test-Path "$RootDir\std") {
    Copy-Item "$RootDir\std\*" "$DistDir\std\" -Recurse -Force
}

Copy-Item "$RootDir\installer\manifests\toolchain-manifest.json" "$DistDir\config\toolchain-manifest.json" -Force
Write-Host "      Standard library staged to std\" -ForegroundColor Green

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

# 3c. Verify the self-contained native toolchain payload before packaging.
Write-Host "[3c/6] Verifying bundled native toolchain components..." -ForegroundColor Yellow
$RequiredBinaries = @("$DistDir\bin\adesh.exe", "$DistDir\bin\adeshlink.exe")
$Missing = @($RequiredBinaries | Where-Object { -not (Test-Path $_) })
if ($Missing.Count -gt 0) {
    throw "Native toolchain incomplete; missing: $($Missing -join ', ')"
}
# Only `adesh_runtime.*` counts: the root `adeshlang` crate is
# crate-type = ["rlib", "cdylib"], so `adeshlang.lib` in this tree is the DLL's
# import library. It is a valid archive that defines no `aot_*` symbol, which is
# why a distribution that ships it as "the runtime" produces AOT links that fail
# on undefined `aot_print_cstr` instead of a missing-runtime diagnosis.
$RuntimeStaged = @(
    "$DistDir\lib\adesh_runtime.lib",
    "$DistDir\lib\libadesh_runtime.a",
    "$DistDir\lib\$RuntimeTriple\adesh_runtime.lib",
    "$DistDir\lib\$RuntimeTriple\libadesh_runtime.a"
) | Where-Object { Test-Path $_ }
if ($RuntimeStaged.Count -eq 0) {
    throw "No adesh_runtime archive in lib\ or lib\$RuntimeTriple\; every native link in the installed toolchain would fail. Re-run: scripts/build-runtime-archives.sh --profile release --targets ""$RuntimeTriple"""
}
if (-not (Get-ChildItem "$DistDir\lib\$RuntimeTriple" -Filter "*adesh_runtime*" -File -ErrorAction SilentlyContinue)) {
    throw "lib\$RuntimeTriple\ holds no runtime archive; a cross build from this install would find nothing. Runtime staged: $($RuntimeStaged -join ', ')"
}
Write-Host "      Runtime libraries staged: $($RuntimeStaged -join ', ')" -ForegroundColor Green

if ($Arch -eq "x86_64") {
    & "$DistDir\bin\adesh.exe" toolchain check | Out-Null
    if ($LASTEXITCODE -ne 0) {
        throw "adesh toolchain check failed on the staged distribution."
    }
    Write-Host "      Native toolchain verified (codegen + adeshlink + ADOB + runtime)." -ForegroundColor Green
} else {
    Write-Host "      Cross-compiled $Arch native toolchain verified (codegen + adeshlink + ADOB + runtime)." -ForegroundColor Green
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

# Also expose standalone executables in dist\ for direct download (x86_64)
if ($Arch -eq "x86_64") {
    Copy-Item "$DistDir\bin\adesh.exe" "$RootDir\dist\adesh.exe" -Force
    if (Test-Path "$DistDir\bin\adl.exe") { Copy-Item "$DistDir\bin\adl.exe" "$RootDir\dist\adl.exe" -Force }
    if (Test-Path "$DistDir\bin\als.exe") { Copy-Item "$DistDir\bin\als.exe" "$RootDir\dist\als.exe" -Force }
    if (Test-Path "$DistDir\bin\adesh-editor.exe") { Copy-Item "$DistDir\bin\adesh-editor.exe" "$RootDir\dist\adesh-editor.exe" -Force }
    if (Test-Path "$DistDir\bin\adeshlink.exe") { Copy-Item "$DistDir\bin\adeshlink.exe" "$RootDir\dist\adeshlink.exe" -Force }
}

$WinInstallScript = "$RootDir\installer\windows\install.ps1"
if (Test-Path $WinInstallScript) {
    Copy-Item $WinInstallScript "$RootDir\dist\install-windows.ps1" -Force
}

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
