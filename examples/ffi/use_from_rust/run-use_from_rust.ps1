#!/usr/bin/env pwsh
# Builds the exported AdeshLang libraries with adesh (AOT), links them into
# a Rust binary with rustc, then runs it.

$ErrorActionPreference = 'Continue'
Set-Location $PSScriptRoot

$Adesh = $null
if ($env:ADESH_HOME) {
    $cand = Join-Path $env:ADESH_HOME 'bin\adesh.exe'
    if (Test-Path $cand) { $Adesh = $cand }
}
if (-not $Adesh) {
    $cmd = Get-Command adesh.exe -ErrorAction SilentlyContinue
    if ($cmd) { $Adesh = $cmd.Source }
}
if (-not $Adesh) {
    foreach ($p in @("$PSScriptRoot\..\..\..\target\release\adesh.exe", "$PSScriptRoot\..\..\..\target\debug\adesh.exe")) {
        if (Test-Path $p) { $Adesh = $p; break }
    }
}
if (-not $Adesh) { throw 'adesh.exe not found. Install AdeshLang or build the repo (cargo build --release).' }
Write-Host "[use_from_rust] Using $Adesh"

# 1) Compile the AdeshLang libraries (use_from_rust.rs calls into both)
Write-Host "[use_from_rust] Compiling math_lib.adesh..."
& $Adesh compile-aot ..\math_lib\math_lib.adesh math_lib.o -c
if ($LASTEXITCODE -ne 0) { throw "compile-aot failed for math_lib.adesh" }

Write-Host "[use_from_rust] Compiling advanced_math.adesh..."
& $Adesh compile-aot ..\advanced_math\advanced_math.adesh advanced_math.o -c
if ($LASTEXITCODE -ne 0) { throw "compile-aot failed for advanced_math.adesh" }

# 2) Build the Rust program with rustc (link the two object files via link-arg)
Write-Host "[use_from_rust] Building use_from_rust.exe with rustc..."
rustc use_from_rust.rs -o use_from_rust.exe -C link-arg=math_lib.o -C link-arg=advanced_math.o -C link-arg=legacy_stdio_definitions.lib
if ($LASTEXITCODE -ne 0) {
    Write-Host ""
    Write-Host "[use_from_rust] NOTE: rustc (MSVC) could not link the AdeshLang AOT objects: they import the"
    Write-Host "[use_from_rust] AdeshLang runtime (adesh_rt_*) which this release does not bundle as an import lib."
    Write-Host "[use_from_rust] Options:"
    Write-Host "[use_from_rust]   1. rustup target add x86_64-pc-windows-gnu, then re-run this script."
    Write-Host "[use_from_rust]   2. Ship the AdeshLang runtime as a static/dynamic library next to the .o files."
    Write-Host ""
    Write-Host "[use_from_rust] Verifying the same exported functions via the C test drivers instead:"
    & .\..\math_lib\test_math.exe | Out-Host
    & .\..\advanced_math\test_advanced_math.exe | Out-Host
    throw "rustc link failed - see note above (AdeshLang runtime import lib not bundled; use the GNU rust target)."
}

# 3) Run the program
Write-Host "`n===== use_from_rust.exe ====="
& .\use_from_rust.exe
if ($LASTEXITCODE -ne 0) { throw "use_from_rust.exe failed with exit code $LASTEXITCODE" }
Write-Host "[use_from_rust] Done."
