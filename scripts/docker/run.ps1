# PowerShell convenience runner for executing Adesh commands in Docker
param (
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$Arguments
)

$ErrorActionPreference = "Stop"
$Image = if ($env:DOCKER_ADESH_IMAGE) { $env:DOCKER_ADESH_IMAGE } else { "adeshlang:latest" }
$Workspace = (Get-Location).Path

$dockerArgs = @("run", "--rm", "-it", "-v", "${Workspace}:/workspace", "-w", "/workspace", $Image) + $Arguments
docker @dockerArgs
