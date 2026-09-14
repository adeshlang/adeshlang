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
Write-Host "[math_lib] Using $Adesh"

# 1) Compile the AdeshLang library to a C-compatible object file + header
Write-Host "[math_lib] Compiling math_lib.adesh..."
& $Adesh compile-aot math_lib.adesh math_lib.o -c --emit-header math_lib.h
if ($LASTEXITCODE -ne 0) { throw "compile-aot failed for math_lib.adesh" }

# 2) Build the C test driver with gcc
Write-Host "[math_lib] Building test_math.exe with gcc..."
$prevEA = $ErrorActionPreference
$ErrorActionPreference = 'Continue'
gcc -Wall -O2 test_math.c math_lib.o -o test_math.exe 2>&1 | Out-Host
$ErrorActionPreference = $prevEA
if ($LASTEXITCODE -ne 0) { throw "gcc failed for test_math" }

# 3) Run the test program
Write-Host "`n===== test_math.exe ====="
& .\test_math.exe
if ($LASTEXITCODE -ne 0) { throw "test_math.exe failed with exit code $LASTEXITCODE" }
Write-Host "[math_lib] Done."
