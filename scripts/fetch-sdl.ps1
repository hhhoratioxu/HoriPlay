$ErrorActionPreference = 'Stop'
$taskRoot = Split-Path -Parent $PSScriptRoot
$taskCache = Join-Path $taskRoot '.cache'
$taskRuntime = Join-Path $taskRoot 'runtime'
New-Item -ItemType Directory -Force -Path $taskCache,$taskRuntime | Out-Null
$taskArchive = Join-Path $taskCache 'SDL3-3.4.18-win32-x64.zip'
Invoke-WebRequest -UseBasicParsing -Uri 'https://github.com/libsdl-org/SDL/releases/download/release-3.4.18/SDL3-3.4.18-win32-x64.zip' -OutFile $taskArchive
if ((Get-FileHash -LiteralPath $taskArchive -Algorithm SHA256).Hash.ToLowerInvariant() -ne '75c2c0fc74e7d1206aaedc22893a35887332f4f85f5c8b74fddc10d177985f2c') { throw 'SDL archive checksum mismatch' }
Expand-Archive -LiteralPath $taskArchive -DestinationPath (Join-Path $taskCache 'sdl3') -Force
$taskLibrary = Get-ChildItem -LiteralPath (Join-Path $taskCache 'sdl3') -Filter SDL3.dll -Recurse | Select-Object -First 1
if (-not $taskLibrary) { throw 'SDL3.dll missing from official archive' }
Copy-Item -LiteralPath $taskLibrary.FullName -Destination (Join-Path $taskRuntime 'SDL3.dll') -Force
Write-Output "Verified official SDL 3.4.18 runtime: $taskRuntime"

