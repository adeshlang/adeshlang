# AdeshLang Windows Web Installer
# Downloads and installs the latest AdeshLang distribution on Windows PowerShell
param(
    [string]$Version = "",
    [string]$InstallDir = "$env:LOCALAPPDATA\AdeshLang",
    [switch]$Force
)

$ErrorActionPreference = "Stop"

Write-Host "==========================================" -ForegroundColor Cyan
Write-Host "   AdeshLang Windows Installer            " -ForegroundColor Cyan
Write-Host "==========================================" -ForegroundColor Cyan

# 1. Determine architecture
$Arch = if ([System.Environment]::Is64BitOperatingSystem) {
    if ($env:PROCESSOR_ARCHITECTURE -eq "ARM64") { "aarch64" } else { "x86_64" }
} else {
    throw "AdeshLang requires a 64-bit Windows operating system."
}

# 2. Determine version
if (-not $Version) {
    $Version = "0.3.0"
}

$Repo = "adeshlang/adeshlang"
$ZipName = "AdeshLang-$Version-$Arch-windows-portable.zip"
$DownloadUrl = "https://github.com/$Repo/releases/download/v$Version/$ZipName"
$TempDir = Join-Path $env:TEMP ([System.IO.Path]::GetRandomFileName())
$ZipPath = Join-Path $TempDir $ZipName

New-Item -ItemType Directory -Path $TempDir -Force | Out-Null

try {
    Write-Host "==> Downloading AdeshLang $Version ($Arch) from GitHub..." -ForegroundColor Yellow
    [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
    Invoke-WebRequest -Uri $DownloadUrl -OutFile $ZipPath -UseBasicParsing

    Write-Host "==> Extracting payload to $InstallDir..." -ForegroundColor Yellow
    if (Test-Path $InstallDir) {
        if ($Force) {
            Remove-Item $InstallDir -Recurse -Force
        }
    }
    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    Expand-Archive -Path $ZipPath -DestinationPath $InstallDir -Force

    # Ensure bin directory exists
    $BinDir = Join-Path $InstallDir "bin"
    if (-not (Test-Path $BinDir)) {
        # Check if contents were extracted inside a subfolder
        $SubFolder = Get-ChildItem -Path $InstallDir -Directory | Select-Object -First 1
        if ($SubFolder -and (Test-Path (Join-Path $SubFolder.FullName "bin"))) {
            $BinDir = Join-Path $SubFolder.FullName "bin"
            $InstallDir = $SubFolder.FullName
        }
    }

    Write-Host "==> Configuring environment variables..." -ForegroundColor Yellow
    [System.Environment]::SetEnvironmentVariable("ADESH_HOME", $InstallDir, [System.EnvironmentVariableTarget]::User)
    [System.Environment]::SetEnvironmentVariable("ADESH_STD", (Join-Path $InstallDir "std"), [System.EnvironmentVariableTarget]::User)

    $UserPath = [System.Environment]::GetEnvironmentVariable("PATH", [System.EnvironmentVariableTarget]::User)
    if ($UserPath -notlike "*$BinDir*") {
        $NewPath = if ($UserPath) { "$UserPath;$BinDir" } else { $BinDir }
        [System.Environment]::SetEnvironmentVariable("PATH", $NewPath, [System.EnvironmentVariableTarget]::User)
        Write-Host "    Added $BinDir to user PATH." -ForegroundColor Green
    }

    # Update current session PATH and ADESH_HOME
    $env:ADESH_HOME = $InstallDir
    $env:ADESH_STD = Join-Path $InstallDir "std"
    $env:PATH = "$BinDir;$env:PATH"

    Write-Host "==> Verifying AdeshLang installation..." -ForegroundColor Yellow
    $AdeshExe = Join-Path $BinDir "adesh.exe"
    if (Test-Path $AdeshExe) {
        & $AdeshExe --version
        & $AdeshExe toolchain check
    }

    Write-Host "`n==========================================" -ForegroundColor Green
    Write-Host " AdeshLang $Version successfully installed!" -ForegroundColor Green
    Write-Host " Location: $InstallDir" -ForegroundColor Green
    Write-Host " Run 'adesh help' or 'adesh doctor' to begin." -ForegroundColor Green
    Write-Host "==========================================" -ForegroundColor Green
}
finally {
    if (Test-Path $TempDir) {
        Remove-Item $TempDir -Recurse -Force -ErrorAction SilentlyContinue
    }
}
