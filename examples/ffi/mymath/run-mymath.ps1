#!/usr/bin/env pwsh
# Builds mymath.dll with gcc, then runs the $cImport example that uses it.

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
Write-Host "[mymath] Using $Adesh"

# 1) Build the C library with gcc
Write-Host "[mymath] Building mymath.dll with gcc..."
gcc -shared -o mymath.dll mymath.c -lm
if ($LASTEXITCODE -ne 0) { throw "gcc failed to build mymath.dll" }

# 2) Run the program (c_import_example.adesh imports mymath.h and calls its functions)
Write-Host "`n===== c_import_example.adesh ====="
& $Adesh run c_import_example.adesh -L . -l mymath
if ($LASTEXITCODE -ne 0) { throw "c_import_example.adesh failed with exit code $LASTEXITCODE" }
Write-Host "[mymath] Done."
