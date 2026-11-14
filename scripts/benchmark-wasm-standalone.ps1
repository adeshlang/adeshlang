param(
    [Parameter(Mandatory)][string]$Compiler,
    [Parameter(Mandatory)][string]$OutputDirectory,
    [ValidateRange(1, 20)][int]$Trials = 3
)
$ErrorActionPreference = 'Stop'
$Compiler = (Resolve-Path -LiteralPath $Compiler).Path
$work = Join-Path $OutputDirectory ("wasm-bench-" + [guid]::NewGuid().ToString('N'))
$null = New-Item -ItemType Directory -Path $work
$work = (Resolve-Path -LiteralPath $work).Path

function Measure-Compiler([string]$InputPath, [string]$OutputPath, [bool]$ObjectOnly) {
    $info = [Diagnostics.ProcessStartInfo]::new($Compiler)
    $info.UseShellExecute = $false
    $info.RedirectStandardOutput = $true
    $info.RedirectStandardError = $true
    foreach ($arg in @('build', $InputPath, '--target=wasm32-unknown-unknown',
        '--codegen=wasm-standalone', '--quiet', '-o', $OutputPath)) {
        $info.ArgumentList.Add($arg)
    }
    if ($ObjectOnly) { $info.ArgumentList.Add('--emit=obj') }
    $process = [Diagnostics.Process]::new()
    $process.StartInfo = $info
    $clock = [Diagnostics.Stopwatch]::StartNew()
    $null = $process.Start()
    $stdout = $process.StandardOutput.ReadToEndAsync()
    $stderr = $process.StandardError.ReadToEndAsync()
    if (!$process.WaitForExit(120000)) {
        $process.Kill($true)
        throw 'Compiler exceeded two-minute per-case timeout'
    }
    $clock.Stop()
    $out = $stdout.GetAwaiter().GetResult()
    $err = $stderr.GetAwaiter().GetResult()
    if ($process.ExitCode -ne 0) { throw "Compilation failed: $out`n$err" }
    $peak = $process.PeakWorkingSet64
    $process.Dispose()
    [pscustomobject]@{
        milliseconds = [math]::Round($clock.Elapsed.TotalMilliseconds, 2)
        peak_working_set_bytes = $peak
        output_bytes = (Get-Item -LiteralPath $OutputPath).Length
    }
}

$results = @()
foreach ($locals in @(1, 300, 3000)) {
    $source = [Text.StringBuilder]::new('export fn many(): i64 {')
    for ($i = 0; $i -lt $locals; $i++) { $null = $source.Append("let x${i}: i64 = $i;") }
    $null = $source.Append("return x$($locals - 1); }")
    $inputPath = Join-Path $work "locals-$locals.adesh"
    [IO.File]::WriteAllText($inputPath, $source.ToString())
    foreach ($objectOnly in @($false, $true)) {
        $outputPath = Join-Path $work ("locals-$locals." + $(if ($objectOnly) { 'o' } else { 'wasm' }))
        for ($trial = 1; $trial -le $Trials; $trial++) {
            $measurement = Measure-Compiler $inputPath $outputPath $objectOnly
            $results += [pscustomobject]@{
                locals = $locals; object_only = $objectOnly; trial = $trial
                milliseconds = $measurement.milliseconds
                peak_working_set_bytes = $measurement.peak_working_set_bytes
                output_bytes = $measurement.output_bytes
            }
        }
    }
}
$report = [pscustomobject]@{
    compiler = $Compiler
    note = 'Fresh compiler processes including gates and linking; Rust build time excluded. Windows peak working set, not allocator-only memory. No warm-up trials discarded.'
    directory = $work
    results = $results
}
$json = $report | ConvertTo-Json -Depth 5
[IO.File]::WriteAllText((Join-Path $work 'results.json'), $json)
$json
