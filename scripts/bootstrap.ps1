[CmdletBinding()] param()
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
Set-Location (Split-Path $PSScriptRoot -Parent)
function Invoke-Native([string]$Command, [string[]]$Arguments) {
    & $Command @Arguments
    if ($LASTEXITCODE -ne 0) { throw "$Command failed: $LASTEXITCODE" }
}
foreach ($Name in @('node','npm','cargo','rustc')) {
    if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) { throw "Install the documented prerequisite first: $Name" }
}
$NodeVersionOutput = & node --version 2>&1
if ($LASTEXITCODE -ne 0) { throw 'Unable to determine the installed Node.js version.' }
$NodeVersionText = ($NodeVersionOutput | Out-String).Trim()
if ($NodeVersionText -notmatch '^v(\d+)(?:\.\d+){0,2}(?:[-+].*)?$') { throw 'Unable to parse the installed Node.js version.' }
if ([int]$Matches[1] -lt 22) { throw 'Node.js 22+ is required.' }
# This is the FIRST resolution. The authoring sandbox had no network/Rust compiler;
# no invented lockfiles are supplied. Inspect the generated files before release.
Invoke-Native 'npm' @('install','--ignore-scripts')
Invoke-Native 'cargo' @('generate-lockfile')
Push-Location 'apps/desktop/src-tauri'
try { Invoke-Native 'cargo' @('generate-lockfile') } finally { Pop-Location }
Write-Host 'Dependency locks generated on THIS PC. Review and commit all lockfiles.'
Write-Host 'Next: ./scripts/check.ps1. No Windows installer is built by this script.'
