[CmdletBinding()]
param(
  [string]$TargetDir = "$env:TEMP\remotecodex-cargo-test-agent"
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if ($env:OS -ne 'Windows_NT') { throw 'This fixture requires Windows desktop.' }

$repo = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$gstRoot = Join-Path $env:LOCALAPPDATA 'Programs\gstreamer\1.0\msvc_x86_64'
$gstBin = Join-Path $gstRoot 'bin'
$gstPkg = Join-Path $gstRoot 'lib\pkgconfig'
if (-not (Test-Path (Join-Path $gstBin 'gst-launch-1.0.exe'))) {
  throw "GStreamer SDK not found at $gstRoot"
}

$oldPath = $env:PATH
$oldPkgConfig = $env:PKG_CONFIG
$oldPkgConfigPath = $env:PKG_CONFIG_PATH
$runner = $null
$record = Join-Path $env:TEMP ("remotecodex-owned-window-" + [guid]::NewGuid().ToString('N') + '.txt')
try {
  Push-Location $repo
  try {
    & cargo build --offline --quiet -p rc-platform-windows --example owned-window-runner --target-dir $TargetDir
    if ($LASTEXITCODE -ne 0) { throw "owned window runner build failed with exit code $LASTEXITCODE" }
  } finally {
    Pop-Location
  }
  $runnerExe = (Resolve-Path (Join-Path $TargetDir 'debug\examples\owned-window-runner.exe')).Path
  $runner = Start-Process -FilePath $runnerExe -ArgumentList @($record) -WindowStyle Hidden -PassThru
  $runnerId = $runner.Id
  $runnerStart = $runner.StartTime
  $deadline = [DateTime]::UtcNow.AddSeconds(10)
  do {
    Start-Sleep -Milliseconds 100
    $runner.Refresh()
    if ($runner.HasExited) { throw 'Owned native window runner exited before publishing its handle.' }
  } while (-not (Test-Path -LiteralPath $record) -and [DateTime]::UtcNow -lt $deadline)
  if (-not (Test-Path -LiteralPath $record)) { throw 'Owned native window runner did not publish a handle.' }
  $fields = Get-Content -LiteralPath $record
  if ($fields.Count -lt 2 -or [uint32]$fields[0] -ne [uint32]$runnerId) {
    throw 'Owned native window runner identity record did not match its process.'
  }
  $windowHandle = [uint64]$fields[1]

  $env:PATH = "$gstBin;$oldPath"
  $env:PKG_CONFIG = Join-Path $gstBin 'pkg-config.exe'
  $env:PKG_CONFIG_PATH = $gstPkg
  Push-Location $repo
  try {
    & cargo run --offline --quiet -p rc-media --features native-media --example native-pipeline-fixture --target-dir $TargetDir -- $windowHandle
    if ($LASTEXITCODE -ne 0) { throw "native pipeline fixture failed with exit code $LASTEXITCODE" }
  } finally {
    Pop-Location
  }
} finally {
  $env:PATH = $oldPath
  $env:PKG_CONFIG = $oldPkgConfig
  $env:PKG_CONFIG_PATH = $oldPkgConfigPath
  if ($runner) {
    $runner.Refresh()
    if (-not $runner.HasExited -and $runner.Id -eq $runnerId -and $runner.StartTime -eq $runnerStart) {
      Stop-Process -InputObject $runner -Force
    }
  }
  if (Test-Path -LiteralPath $record) { Remove-Item -LiteralPath $record -Force }
}
