param(
    [string]$Dist = "$(Join-Path (Resolve-Path "$PSScriptRoot\..\..\").Path 'dist')"
)

$ErrorActionPreference = "Stop"
Get-ChildItem -LiteralPath $Dist -File |
    Where-Object { $_.Extension -in ".zip", ".xz", ".pkg", ".deb", ".rpm", ".exe" } |
    Sort-Object Name |
    ForEach-Object {
        $hash = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
        "$hash  $($_.Name)"
    } | Set-Content -LiteralPath (Join-Path $Dist "SHA256SUMS") -Encoding ascii
