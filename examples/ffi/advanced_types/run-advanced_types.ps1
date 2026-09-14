#!/usr/bin/env pwsh
# Builds the type-mapping stub library with gcc, then runs the advanced_types
# reference program via the adesh CLI.

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
Write-Host "[advanced_types] Using $Adesh"

# 1) Build the stub library that implements the extern declarations
Write-Host "[advanced_types] Building advanced_types.dll (types_stub.c) with gcc..."
gcc -shared -o advanced_types.dll types_stub.c
if ($LASTEXITCODE -ne 0) { throw "gcc failed to build advanced_types.dll" }

# 2) Run the reference program
Write-Host "`n===== advanced_types.adesh ====="
& $Adesh run advanced_types.adesh -L . -l advanced_types
if ($LASTEXITCODE -ne 0) { throw "advanced_types.adesh failed with exit code $LASTEXITCODE" }
Write-Host "[advanced_types] Done."
