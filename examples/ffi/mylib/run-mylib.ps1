#!/usr/bin/env pwsh
# Builds the C library with gcc, then runs all mylib demo programs via the adesh CLI.

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
Write-Host "[mylib] Using $Adesh"

# 1) Build the C library with gcc
Write-Host "[mylib] Building mylib.dll with gcc..."
gcc -shared -o mylib.dll mylib.c -lm
if ($LASTEXITCODE -ne 0) { throw "gcc failed to build mylib.dll" }

# 2) Run the programs (library lives next to the programs)
$Programs = @(
    'example1_basic_math.adesh'
    'example2_advanced_math.adesh'
    'example3_strings.adesh'
    'example4_random_time.adesh'
    'example5_boolean.adesh'
    'example6_cimport.adesh'
    'test_cimport.adesh'
    'test_bool.adesh'
    'test_bool2.adesh'
    'test_bool3.adesh'
)
foreach ($prog in $Programs) {
    Write-Host "`n===== $prog ====="
    & $Adesh run $prog -L . -l mylib
    if ($LASTEXITCODE -ne 0) { throw "$prog failed with exit code $LASTEXITCODE" }
}
Write-Host "`n[mylib] All programs ran successfully."
