[CmdletBinding()] param([string]$Helper='target/release/rc-media.exe')
$ErrorActionPreference='Stop'; Set-StrictMode -Version Latest
Set-Location (Split-Path $PSScriptRoot -Parent)
if ($env:OS -ne 'Windows_NT') { throw 'Windows required; native tests were not run.' }
if (-not (Test-Path $Helper)) { throw 'Build native-media first with scripts/build-source.ps1 -NativeMedia.' }
& cargo check --locked -p rc-media --features native-media --all-targets
if ($LASTEXITCODE -ne 0) { throw 'Native media compile failed.' }
$raw = & $Helper --probe
if ($LASTEXITCODE -ne 0) { throw 'Native media probe failed.' }
$probe = ($raw -join "`n") | ConvertFrom-Json
if (-not $probe.available -or -not $probe.native_backend) { throw "Missing native media factories: $($probe.missing -join ', ')" }
$dir='docs/test-results/local-media'; New-Item -ItemType Directory -Force $dir | Out-Null
$probe | ConvertTo-Json -Depth 6 | Set-Content -Encoding UTF8 "$dir/factories.json"
Write-Host 'Factory discovery PASS only. WGC/encoder negotiation/WebRTC/SendInput/performance NOT certified.'
Write-Host 'Start scripts/gui-fixture.ps1 locally and follow docs/compatibility/MEDIA.md on the two real PCs.'
