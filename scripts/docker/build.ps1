# PowerShell script to build AdeshLang Docker images
param (
    [string]$Tag = "latest"
)

$ErrorActionPreference = "Stop"
$RepoRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)

Push-Location $RepoRoot
try {
    Write-Host "==> Building AdeshLang Docker image (tag: adeshlang:$Tag)..." -ForegroundColor Cyan
    docker build -t "adeshlang:$Tag" -f Dockerfile .

    if ($Tag -eq "latest") {
        Write-Host "==> Building AdeshLang Slim Docker image (tag: adeshlang:slim)..." -ForegroundColor Cyan
        docker build -t "adeshlang:slim" -f Dockerfile.slim .
    }

    Write-Host "==> Docker build completed successfully!" -ForegroundColor Green
    docker images | Select-String "adeshlang"
} finally {
    Pop-Location
}
