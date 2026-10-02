<#
.SYNOPSIS
    Runs the AdeshLang example test suites under examples/*/tests/.

.DESCRIPTION
    For every examples/<folder>/tests/main.adesh this script runs:
      * adesh run --test <file>   (native test harness discovery)
      * adesh run <file>          (standalone analytics test runner)
    and reports a pass/fail summary table with a non-zero exit code if
    any suite failed.

.PARAMETER Name
    Only run folders whose name matches this wildcard (default "*").

.PARAMETER Mode
    "test" = only `adesh run --test`, "run" = only standalone runner,
    "both" = run both (default).

.PARAMETER IncludeModules
    Also run `adesh check` + `adesh run` on every tests/*.test.adesh module
    standalone (catches type/ownership errors that only surface when a module
    is the entry file).

.EXAMPLE
    powershell -File scripts/run_example_tests.ps1
    powershell -File scripts/run_example_tests.ps1 -Name loops
    powershell -File scripts/run_example_tests.ps1 -Name "*" -Mode test -IncludeModules
#>
param(
    [string]$Name = "*",
    [ValidateSet("test", "run", "both")]
    [string]$Mode = "both",
    [switch]$IncludeModules
)

$repoRoot = Split-Path -Parent $PSScriptRoot
$examplesDir = Join-Path $repoRoot "examples"
$workDir = Join-Path ([System.IO.Path]::GetTempPath()) "adesh_example_tests"
New-Item -ItemType Directory -Path $workDir -Force | Out-Null

$mains = Get-ChildItem -Path $examplesDir -Directory |
    Where-Object { $_.Name -like $Name } |
    ForEach-Object { Join-Path $_.FullName "tests\main.adesh" } |
    Where-Object { Test-Path $_ } |
    Sort-Object

if (-not $mains) {
    Write-Host "No examples/<folder>/tests/main.adesh found matching '$Name'."
    exit 1
}

function Invoke-Aadesh {
    param([string[]]$Arguments, [string]$LogName)
    $log = Join-Path $workDir $LogName
    $argString = ($Arguments | ForEach-Object { if ($_ -match '[\s"]') { '"' + $_ + '"' } else { $_ } }) -join ' '
    cmd /c "adesh $argString > `"$log`" 2>&1" | Out-Null
    $code = $LASTEXITCODE
    $text = ""
    if (Test-Path $log) { $text = Get-Content $log -Raw }
    return @{ Code = $code; Text = $text }
}

$results = @()

foreach ($main in $mains) {
    $folder = Split-Path -Parent (Split-Path -Parent $main)
    $folderName = Split-Path -Leaf $folder
    $rel = $main.Substring($repoRoot.Length + 1)

    if ($IncludeModules) {
        $modules = Get-ChildItem -Path (Split-Path $main) -Filter "*.test.adesh" -File | Sort-Object
        foreach ($mod in $modules) {
            $modRel = $mod.FullName.Substring($repoRoot.Length + 1)
            $chk = Invoke-Aadesh -Arguments @("check", $mod.FullName) -LogName "check_$folderName`_$($mod.BaseName).txt"
            $run = Invoke-Aadesh -Arguments @("run", $mod.FullName) -LogName "run_$folderName`_$($mod.BaseName).txt"
            $ok = ($chk.Code -eq 0 -and $run.Code -eq 0)
            $results += [pscustomobject]@{
                Folder = $folderName
                Target = $modRel
                Mode   = "module"
                Status = if ($ok) { "PASS" } else { "FAIL" }
                Code   = "$($chk.Code)/$($run.Code)"
            }
            if (-not $ok) {
                Write-Host "--- $modRel (check=$($chk.Code) run=$($run.Code)) ---"
                $tail = (@($chk.Text -split "`n") + @($run.Text -split "`n")) | Select-Object -Last 15
                Write-Host ($tail -join "`n")
            }
        }
    }

    $modes = @()
    if ($Mode -eq "test" -or $Mode -eq "both") { $modes += "test" }
    if ($Mode -eq "run" -or $Mode -eq "both") { $modes += "run" }

    foreach ($m in $modes) {
        if ($m -eq "test") {
            $res = Invoke-Aadesh -Arguments @("run", "--test", $main) -LogName "test_$folderName.txt"
        } else {
            $res = Invoke-Aadesh -Arguments @("run", $main) -LogName "run_$folderName.txt"
        }
        $ok = ($res.Code -eq 0)
        $results += [pscustomobject]@{
            Folder = $folderName
            Target = $rel
            Mode   = $m
            Status = if ($ok) { "PASS" } else { "FAIL" }
            Code   = "$($res.Code)"
        }
        if (-not $ok) {
            Write-Host "--- FAILED: $rel [$m] ---"
            Write-Host ((@($res.Text -split "`n") | Select-Object -Last 20) -join "`n")
        }
    }
}

$pass = @($results | Where-Object { $_.Status -eq "PASS" }).Count
$fail = @($results | Where-Object { $_.Status -eq "FAIL" }).Count

Write-Host ""
Write-Host "=============================================================="
Write-Host ("  Example test suites: {0} passed, {1} failed (of {2} runs)" -f $pass, $fail, $results.Count)
Write-Host "=============================================================="
$results | Format-Table -AutoSize Folder, Mode, Status, Code, Target | Out-String | Write-Host

if ($fail -gt 0) { exit 1 }
exit 0
