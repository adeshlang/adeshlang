#!/usr/bin/env pwsh
# Builds wingui.dll with gcc (links user32/gdi32), then runs the GUI demo programs.

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
Write-Host "[wingui] Using $Adesh"

# 1) Build the C library with gcc
Write-Host "[wingui] Building wingui.dll with gcc..."
gcc -shared -o wingui.dll wingui.c -luser32 -lgdi32
if ($LASTEXITCODE -ne 0) { throw "gcc failed to build wingui.dll" }

# 2) Run the GUI programs (each opens a window; close it to continue)
$Programs = @('demo_window.adesh', 'test_window_simple.adesh', 'test_window_u64.adesh')
foreach ($prog in $Programs) {
    Write-Host "`n===== $prog ====="
    & $Adesh run $prog -L . -l wingui
    if ($LASTEXITCODE -ne 0) { throw "$prog failed with exit code $LASTEXITCODE" }
}
Write-Host "[wingui] Done."
