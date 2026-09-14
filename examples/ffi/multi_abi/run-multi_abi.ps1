#!/usr/bin/env pwsh
# Runs the multi_abi demo: C printf/malloc/free plus Rust/System ABI
# declarations, via the adesh CLI. Verifies the gcc toolchain first.

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
Write-Host "[multi_abi] Using $Adesh"

# 1) Verify the gcc toolchain is available (demo links against the CRT)
Write-Host "[multi_abi] Verifying gcc..."
gcc --version | Select-Object -First 1
if ($LASTEXITCODE -ne 0) { throw "gcc not found on PATH" }

# 2) Run the demo (C runtime functions are loaded automatically)
Write-Host "`n===== multi_abi.adesh ====="
& $Adesh run multi_abi.adesh
if ($LASTEXITCODE -ne 0) { throw "multi_abi.adesh failed with exit code $LASTEXITCODE" }
Write-Host "[multi_abi] Done."
