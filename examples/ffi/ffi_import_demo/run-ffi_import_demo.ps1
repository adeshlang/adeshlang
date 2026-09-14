#!/usr/bin/env pwsh
# Runs the ffi_import_demo: strlen/getenv/strcmp/atoi/llabs from the C
# runtime via the adesh CLI (the C runtime is loaded automatically).

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
Write-Host "[ffi_import_demo] Using $Adesh"

Write-Host "`n===== ffi_import_demo.adesh ====="
& $Adesh run ffi_import_demo.adesh
if ($LASTEXITCODE -ne 0) { throw "ffi_import_demo.adesh failed with exit code $LASTEXITCODE" }
Write-Host "[ffi_import_demo] Done."
