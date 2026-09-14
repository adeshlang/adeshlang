#!/usr/bin/env pwsh
# Runs the math_functions demo: sqrt/pow/sin/cos from the platform libm
# via the adesh CLI (the C math runtime is loaded automatically).

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
Write-Host "[math_functions] Using $Adesh"

Write-Host "`n===== math_functions.adesh ====="
& $Adesh run math_functions.adesh
if ($LASTEXITCODE -ne 0) { throw "math_functions.adesh failed with exit code $LASTEXITCODE" }
Write-Host "[math_functions] Done."
