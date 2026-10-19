param(
    [Parameter(Mandatory)][string]$Executable,
    [Parameter(Mandatory)][string]$Label,
    [Parameter(Mandatory)][string]$OutputDirectory,
    [int]$Seconds = 12,
    [int]$Runs = 3
)
$ErrorActionPreference = 'Stop'
$executablePath = (Resolve-Path -LiteralPath $Executable).Path
$outputPath = [IO.Path]::GetFullPath($OutputDirectory)
[IO.Directory]::CreateDirectory($outputPath) | Out-Null
$records = @()
for ($run = 1; $run -le $Runs; $run++) {
    $stdoutPath = Join-Path $outputPath "$Label-$run-engine.json"
    $stderrPath = Join-Path $outputPath "$Label-$run-stderr.log"
    $timer = [Diagnostics.Stopwatch]::StartNew()
    $process = Start-Process -FilePath $executablePath -ArgumentList @('engine-test', '--seconds', $Seconds) -WindowStyle Hidden -RedirectStandardOutput $stdoutPath -RedirectStandardError $stderrPath -PassThru
    # Retain the process handle so final CPU times remain queryable after exit.
    $null = $process.Handle
    $peakWorkingSet = 0L
    while (-not $process.WaitForExit(100)) {
        $process.Refresh()
        $peakWorkingSet = [Math]::Max($peakWorkingSet, $process.PeakWorkingSet64)
    }
    $process.WaitForExit()
    $timer.Stop()
    if ($process.ExitCode -ne 0) { throw "Engine exited $($process.ExitCode): $(Get-Content -LiteralPath $stderrPath -Raw)" }
    $cpuMilliseconds = $process.TotalProcessorTime.TotalMilliseconds
    $snapshot = Get-Content -LiteralPath $stdoutPath -Raw | ConvertFrom-Json
    if ($snapshot.input_error) { throw $snapshot.input_error }
    $records += [ordered]@{
        run = $run
        wall_seconds = $timer.Elapsed.TotalSeconds
        cpu_ms = $cpuMilliseconds
        cpu_percent_of_one_core = 100 * $cpuMilliseconds / $timer.Elapsed.TotalMilliseconds
        peak_working_set_bytes = $peakWorkingSet
        physical_devices = @($snapshot.devices).Count
        poll_work = $snapshot.sample
    }
    $process.Dispose()
}
$evidence = [ordered]@{
    label = $Label
    captured_at_utc = [DateTime]::UtcNow.ToString('o')
    executable_sha256 = (Get-FileHash -LiteralPath $executablePath -Algorithm SHA256).Hash.ToLowerInvariant()
    seconds_per_run = $Seconds
    logical_processors = [Environment]::ProcessorCount
    runs = $records
    scope = 'Real native SDL engine process; no synthetic devices, no session, no virtual driver. CPU is percent of one logical core, includes startup/shutdown. Empty-device results do not measure active-controller or game latency.'
}
$evidence | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath (Join-Path $outputPath "$Label-engine-summary.json") -Encoding utf8
$evidence | ConvertTo-Json -Depth 12
