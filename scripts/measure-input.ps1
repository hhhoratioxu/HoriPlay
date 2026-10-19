param(
    [Parameter(Mandatory)][string]$Executable,
    [Parameter(Mandatory)][string]$OutputDirectory,
    [int]$Samples = 600,
    [int]$Runs = 3
)
$ErrorActionPreference = 'Stop'
$executablePath = (Resolve-Path -LiteralPath $Executable).Path
$outputPath = [IO.Path]::GetFullPath($OutputDirectory)
[IO.Directory]::CreateDirectory($outputPath) | Out-Null
$records = @()
for ($run = 1; $run -le $Runs; $run++) {
    # Alternate execution order to reduce a consistent warm-cache/order advantage.
    $modes = if ($run % 2) { @('poll', 'event') } else { @('event', 'poll') }
    foreach ($mode in $modes) {
        $arguments = @('input-bench', '--samples', $Samples)
        if ($mode -eq 'poll') { $arguments += '--poll-8ms' }
        $json = & $executablePath @arguments
        if ($LASTEXITCODE -ne 0) { throw "Input benchmark failed ($mode, run $run)" }
        $result = ($json -join [Environment]::NewLine) | ConvertFrom-Json
        if ($result.attempted -ne $Samples -or $result.accepted -ne $Samples) { throw 'Incomplete input measurement' }
        $json | Set-Content -LiteralPath (Join-Path $outputPath "input-$mode-$run.json") -Encoding utf8
        $records += [ordered]@{ run = $run; mode = $mode; result = $result }
    }
}
[ordered]@{
    captured_at_utc = [DateTime]::UtcNow.ToString('o')
    executable_sha256 = (Get-FileHash -LiteralPath $executablePath -Algorithm SHA256).Hash.ToLowerInvariant()
    source = 'Same executable in both modes; comparison isolates the old 8 ms polling gate. It is not a full old-vs-new Engine benchmark.'
    runs = $records
} | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath (Join-Path $outputPath 'input-comparison.json') -Encoding utf8
$records | ForEach-Object { [pscustomobject]@{ Run=$_.run; Mode=$_.mode; Samples=$_.result.accepted; P50_ms=$_.result.sample_to_authorized_receiver.p50_ms; P95_ms=$_.result.sample_to_authorized_receiver.p95_ms; P99_ms=$_.result.sample_to_authorized_receiver.p99_ms } } | Format-Table -AutoSize
