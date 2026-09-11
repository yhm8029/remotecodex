[CmdletBinding()] param([string]$Config = 'config/agent.local.toml')
$ErrorActionPreference = 'Stop'; Set-StrictMode -Version Latest
Set-Location (Split-Path $PSScriptRoot -Parent)
if (-not (Test-Path $Config)) { throw 'Copy config/agent.example.toml to agent.local.toml and review it first.' }
if (-not (Test-Path 'apps/web/dist/index.html')) { throw 'Run npm run build:web first.' }
# Foreground is intentional in this source alpha: visible local emergency stop, no covert persistence.
# Closing the Tauri window is independent. Closing THIS console/Ctrl+C explicitly stops the host.
& cargo run --release --locked -p rc-agent -- --config $Config run
if ($LASTEXITCODE -ne 0) { throw "Agent exited with $LASTEXITCODE" }
