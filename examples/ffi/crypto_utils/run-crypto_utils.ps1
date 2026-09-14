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
Write-Host "[crypto_utils] Using $Adesh"

# 1) Compile the AdeshLang library to a C-compatible object file + header
Write-Host "[crypto_utils] Compiling crypto_utils.adesh..."
& $Adesh compile-aot crypto_utils.adesh crypto_utils.o -c --emit-header crypto_utils.h
if ($LASTEXITCODE -ne 0) { throw "compile-aot failed for crypto_utils.adesh" }

# 2) Build the C test driver with gcc
Write-Host "[crypto_utils] Building test_crypto.exe with gcc..."
$prevEA = $ErrorActionPreference
$ErrorActionPreference = 'Continue'
gcc -Wall -O2 test_crypto.c crypto_utils.o -o test_crypto.exe 2>&1 | Out-Host
$ErrorActionPreference = $prevEA
if ($LASTEXITCODE -ne 0) { throw "gcc failed for test_crypto" }

# 3) Run the test program
Write-Host "`n===== test_crypto.exe ====="
& .\test_crypto.exe
if ($LASTEXITCODE -ne 0) { throw "test_crypto.exe failed with exit code $LASTEXITCODE" }
Write-Host "[crypto_utils] Done."
