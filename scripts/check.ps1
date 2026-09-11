[CmdletBinding()] param([switch]$WindowsPtyTests,[switch]$NativeMedia)
$ErrorActionPreference = 'Stop'; Set-StrictMode -Version Latest
Set-Location (Split-Path $PSScriptRoot -Parent)
function Run([string]$Command, [string[]]$Arguments) { & $Command @Arguments; if ($LASTEXITCODE -ne 0) { throw "$Command failed ($LASTEXITCODE)" } }
if (-not (Test-Path 'Cargo.lock') -or -not (Test-Path 'package-lock.json')) { throw 'Run scripts/bootstrap.ps1 first; release checks require real lockfiles.' }
Run 'cargo' @('fmt','--all','--','--check')
Run 'cargo' @('check','--workspace','--all-targets','--locked')
Run 'cargo' @('test','--workspace','--locked')
Run 'npm' @('test')
Run 'npm' @('run','check:web')
Run 'npm' @('run','build:web')
Push-Location 'apps/desktop/src-tauri'
try { Run 'cargo' @('check','--locked') } finally { Pop-Location }
if ($WindowsPtyTests) { Run 'cargo' @('test','-p','rc-agent','--test','conpty_smoke','--locked','--','--ignored','--nocapture') }
if ($NativeMedia) { Run 'cargo' @('check','-p','rc-media','--features','native-media','--all-targets','--locked'); Run 'cargo' @('test','-p','rc-media','--features','native-media','--locked') }
Write-Host 'Only the commands that actually passed above are verified. Full SPEC gates are separate.'
