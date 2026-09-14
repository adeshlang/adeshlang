#!/usr/bin/env pwsh
# Runs the libc_strlen demo: calls C's strlen() from the platform runtime
# via the adesh CLI (the C runtime is loaded automatically).

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
Write-Host "[libc_strlen] Using $Adesh"

Write-Host "`n===== libc_strlen.adesh ====="
& $Adesh run libc_strlen.adesh
if ($LASTEXITCODE -ne 0) { throw "libc_strlen.adesh failed with exit code $LASTEXITCODE" }
Write-Host "[libc_strlen] Done."
