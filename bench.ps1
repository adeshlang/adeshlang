# Paths & Modes configuration
$examplesDir = "D:\Projects\AdeshLang\examples"
$modes = @("interpreter", "jit", "njit", "aot", "vm")

# Check if directory exists
if (-not (Test-Path -Path $examplesDir)) {
    Write-Error "Directory $examplesDir does not exist."
    exit 1
}

# Fetch all .adesh files (space added after -Path)
$files = Get-ChildItem -Path $examplesDir -Filter "*.adesh" -Recurse

if ($files.Count -eq 0) {
    Write-Host "No .adesh files found in $examplesDir" -ForegroundColor Yellow
    exit 0
}

Write-Host "Found $($files.Count) .adesh file(s). Starting execution and comparison...`n" -ForegroundColor Cyan

$results = @()

foreach ($file in $files) {
    Write-Host "========================================" -ForegroundColor Gray
    Write-Host " Testing File: $($file.Name)" -ForegroundColor Green
    Write-Host "========================================" -ForegroundColor Gray

    $modeOutputs = [ordered]@{} $modeTimes = [ordered]@{}

    foreach ($mode in $modes) {$sw = [System.Diagnostics.Stopwatch]::StartNew()
        
        try {
            $processOutput = adesh run "--$mode" $file.FullName 2>&1 | Out-String
        } catch {
            $processOutput = "ERROR: $_"
        }

        $sw.Stop()

        $trimmedOutput = $processOutput.Trim()$modeOutputs[$mode] =$trimmedOutput
        $modeTimes[$mode] = "$([math]::Round($sw.Elapsed.TotalMilliseconds, 2)) ms"

        Write-Host " Mode: " -NoNewline
        Write-Host "$mode " -ForegroundColor Yellow -NoNewline
        Write-Host "($($modeTimes[$mode]))" -ForegroundColor DarkGray
        Write-Host " Output:"
        Write-Host ($trimmedOutput -replace '(?m)^', '   ') -ForegroundColor DarkCyan
    }

    # Verify output consistency
    $firstOutput = $modeOutputs[$modes[0]]
    $outputsMatch =$true

    foreach ($mode in$modes) {
        if ($modeOutputs[$mode] -ne$firstOutput) {
            $outputsMatch =$false
            break
        }
    }

    $statusText = if ($outputsMatch) { "MATCH" } else { "MISMATCH" }
    
    $resultObj = [PSCustomObject]@{
        FileName    = $file.Name
        Consistency = $statusText
    }

    foreach ($mode in $modes) {$resultObj | Add-Member -MemberType NoteProperty -Name "Time ($mode)" -Value $modeTimes[$mode]
    }

    $results +=$resultObj

    if ($outputsMatch) {
        Write-Host "`n Output Check: ALL MODES MATCH" -ForegroundColor Green
    } else {
        Write-Host "`n Output Check: MISMATCH DETECTED" -ForegroundColor Red
    }
    Write-Host ""
}

Write-Host "========================================" -ForegroundColor Gray
Write-Host " BENCHMARK & CONSISTENCY SUMMARY" -ForegroundColor Header
Write-Host "========================================" -ForegroundColor Gray
$results | Format-Table -AutoSize