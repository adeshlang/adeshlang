#!/usr/bin/env pwsh
# Compiles the AdeshLang library with adesh (AOT), builds the C test driver
# with gcc (-pthread), then runs the concurrent test program.

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
Write-Host "[concurrent_demo] Using $Adesh"

# 1) Compile the AdeshLang library to a C-compatible object file + header
Write-Host "[concurrent_demo] Compiling concurrent_demo.adesh..."
& $Adesh compile-aot concurrent_demo.adesh concurrent_demo.o -c --emit-header concurrent_demo.h
if ($LASTEXITCODE -ne 0) { throw "compile-aot failed for concurrent_demo.adesh" }

# 2) Build the C test driver with gcc (-pthread for the thread library)
Write-Host "[concurrent_demo] Building test_concurrent.exe with gcc..."
$prevEA = $ErrorActionPreference
$ErrorActionPreference = 'Continue'
gcc -Wall -O2 -pthread test_concurrent.c concurrent_demo.o -o test_concurrent.exe 2>&1 | Out-Host
$ErrorActionPreference = $prevEA
if ($LASTEXITCODE -ne 0) { throw "gcc failed for test_concurrent" }

# 3) Run the test program
Write-Host "`n===== test_concurrent.exe ====="
& .\test_concurrent.exe
if ($LASTEXITCODE -ne 0) { throw "test_concurrent.exe failed with exit code $LASTEXITCODE" }
Write-Host "[concurrent_demo] Done."
