#!/usr/bin/env pwsh
# Builds the custom C library with gcc, then runs the custom_lib demo
# (add_numbers/print_message) via the adesh CLI.

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
Write-Host "[custom_lib] Using $Adesh"

# 1) Build the C library with gcc (customlib.c provides add_numbers/print_message)
Write-Host "[custom_lib] Building customlib.dll with gcc..."
gcc -shared -o customlib.dll customlib.c
if ($LASTEXITCODE -ne 0) { throw "gcc failed to build customlib.dll" }

# 2) Run the demo
Write-Host "`n===== custom_lib.adesh ====="
& $Adesh run custom_lib.adesh -L . -l customlib
if ($LASTEXITCODE -ne 0) { throw "custom_lib.adesh failed with exit code $LASTEXITCODE" }
Write-Host "[custom_lib] Done."
