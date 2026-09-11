[CmdletBinding()] param([switch]$AgentE2E,[string]$AgentPath='target/release/rc-agent.exe',[string]$Origin='http://127.0.0.1:3847')
$ErrorActionPreference='Stop'; Set-StrictMode -Version Latest
Set-Location (Split-Path $PSScriptRoot -Parent)
if ($env:OS -ne 'Windows_NT') { throw 'Windows required; no skip-as-pass.' }
& cargo test --workspace --locked
if ($LASTEXITCODE -ne 0) { throw 'Rust tests failed.' }
& cargo test -p rc-agent --test conpty_smoke --locked -- --ignored --nocapture
if ($LASTEXITCODE -ne 0) { throw 'Actual ConPTY test failed.' }
if ($AgentE2E) {
    # Requires the user to have already started the Agent. Creates/revokes its own
    # temporary device and two CMD sessions, not any existing user session.
    & npm test
    if ($LASTEXITCODE -ne 0) { throw 'Portable tests failed.' }
    & node tests/e2e/windows-agent.mjs --agent $AgentPath --origin $Origin
    if ($LASTEXITCODE -ne 0) { throw 'Real Agent E2E failed.' }
}
