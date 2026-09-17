#!/usr/bin/env pwsh
Write-Host "===================================================" -ForegroundColor Cyan
Write-Host "Building AdeshLang Mobile Bridge for Android (NDK)" -ForegroundColor Cyan
Write-Host "===================================================" -ForegroundColor Cyan

if (-not $env:ANDROID_NDK_HOME) {
    if (Test-Path "C:\Android\Sdk\ndk\27.0.11902837") {
        $env:ANDROID_NDK_HOME = "C:\Android\Sdk\ndk\27.0.11902837"
    } elseif (Test-Path "$env:LOCALAPPDATA\Android\Sdk\ndk") {
        $ndkDirs = Get-ChildItem "$env:LOCALAPPDATA\Android\Sdk\ndk" -Directory
        if ($ndkDirs.Count -gt 0) {
            $env:ANDROID_NDK_HOME = $ndkDirs[0].FullName
        }
    }
}

if (-not $env:ANDROID_NDK_HOME) {
    Write-Error "ANDROID_NDK_HOME is not set and could not be detected. Please set ANDROID_NDK_HOME."
    exit 1
}

Write-Host "Using NDK at: $env:ANDROID_NDK_HOME" -ForegroundColor Green

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$outputDir = Join-Path $scriptDir "flutter\android\app\src\main\jniLibs"
$manifestPath = Join-Path $scriptDir "bridge\Cargo.toml"

Write-Host "Target output directory: $outputDir" -ForegroundColor Green

cargo ndk -t arm64-v8a -t x86_64 -o "$outputDir" --manifest-path "$manifestPath" build --release

if ($LASTEXITCODE -ne 0) {
    Write-Error "cargo ndk build failed with code $LASTEXITCODE"
    exit $LASTEXITCODE
}

Write-Host "Copying AdeshLang standard library to Flutter assets..." -ForegroundColor Yellow
$stdAssets = Join-Path $scriptDir "flutter\assets\std"
New-Item -ItemType Directory -Force -Path $stdAssets | Out-Null
Copy-Item (Join-Path $scriptDir "..\src\stdlib\*.adesh") $stdAssets -Force

Write-Host "[SUCCESS] AdeshLang Mobile Bridge libraries built and standard library bound successfully!" -ForegroundColor Green
