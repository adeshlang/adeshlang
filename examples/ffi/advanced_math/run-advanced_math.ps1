#!/usr/bin/env pwsh
# Compiles the AdeshLang library with adesh (AOT), builds the C test driver
# with gcc, then runs the test program.

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
Write-Host "[advanced_math] Using $Adesh"

# 1) Compile the AdeshLang library to a C-compatible object file + header
Write-Host "[advanced_math] Compiling advanced_math.adesh..."
& $Adesh compile-aot advanced_math.adesh advanced_math.o -c --emit-header advanced_math.h
if ($LASTEXITCODE -ne 0) { throw "compile-aot failed for advanced_math.adesh" }

# 2) Build the C test driver with gcc
Write-Host "[advanced_math] Building test_advanced_math.exe with gcc..."
$prevEA = $ErrorActionPreference
$ErrorActionPreference = 'Continue'
gcc -Wall -O2 test_advanced_math.c advanced_math.o -o test_advanced_math.exe -lm 2>&1 | Out-Host
$ErrorActionPreference = $prevEA
if ($LASTEXITCODE -ne 0) { throw "gcc failed for test_advanced_math" }

# 3) Run the test program
Write-Host "`n===== test_advanced_math.exe ====="
& .\test_advanced_math.exe
if ($LASTEXITCODE -ne 0) { throw "test_advanced_math.exe failed with exit code $LASTEXITCODE" }
Write-Host "[advanced_math] Done."
