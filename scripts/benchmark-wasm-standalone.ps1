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
    $peak = 0L
    while (!$process.WaitForExit(1)) {
        if ($clock.Elapsed.TotalSeconds -gt 120) {
            $process.Kill($true)
            throw 'Compiler exceeded two-minute per-case timeout'
        }
        try {
            $process.Refresh()
            $peak = [math]::Max($peak, $process.PeakWorkingSet64)
        } catch [InvalidOperationException] {
            # The process may exit between WaitForExit and Refresh.
        }
    }
    $clock.Stop()
    $out = $stdout.GetAwaiter().GetResult()
    $err = $stderr.GetAwaiter().GetResult()
    if ($process.ExitCode -ne 0) { throw "Compilation failed: $out`n$err" }
    try { $peak = [math]::Max($peak, $process.PeakWorkingSet64) } catch [InvalidOperationException] {}
    if ($peak -eq 0) { throw 'Peak working set could not be measured' }
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
$loopInput = Join-Path $work 'loop.adesh'
$loopOutput = Join-Path $work 'loop.wasm'
[IO.File]::WriteAllText($loopInput, @'
export fn sum(n: i64): i64 {
    let total: i64 = 0;
    while (n > 0) {
        total = total + n;
        n = n - 1;
    }
    return total;
}
'@)
$loopCompile = Measure-Compiler $loopInput $loopOutput $false
$runner = Join-Path $work 'run.cjs'
[IO.File]::WriteAllText($runner, @'
const fs = require('fs'), assert = require('assert');
const start = process.hrtime.bigint();
const bytes = fs.readFileSync(process.argv[2]);
const m = new WebAssembly.Module(bytes);
const e = new WebAssembly.Instance(m, {}).exports;
const call = process.hrtime.bigint();
assert.equal(e.sum(1000000n), 500000500000n);
const end = process.hrtime.bigint();
console.log(JSON.stringify({
    instantiate_ms: Number(call-start)/1e6,
    first_call_ms: Number(end-call)/1e6,
    rss_after_bytes: process.memoryUsage().rss,
    output_bytes: bytes.length
}));
'@)
$execution = @()
for ($trial = 1; $trial -le $Trials; $trial++) {
    $raw = & node $runner $loopOutput
    if ($LASTEXITCODE -ne 0) { throw "Node execution failed: $raw" }
    $execution += ($raw | ConvertFrom-Json)
}
$report = [pscustomobject]@{
    compiler = $Compiler
    note = 'Fresh compiler processes including gates and linking; Rust build time excluded. Windows process-reported peak working set sampled with 1ms waits, not allocator-only memory. No warm-up trials discarded. Loop execution is the first call in each fresh Node process.'
    directory = $work
    results = $results
    loop_compile = $loopCompile
    loop_execution = $execution
}
$json = $report | ConvertTo-Json -Depth 5
[IO.File]::WriteAllText((Join-Path $work 'results.json'), $json)
$json
