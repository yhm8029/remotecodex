[CmdletBinding()] param([switch]$NativeMedia, [switch]$Desktop)
$ErrorActionPreference = 'Stop'; Set-StrictMode -Version Latest
Set-Location (Split-Path $PSScriptRoot -Parent)
function Run([string]$Command,[string[]]$Arguments) { & $Command @Arguments; if ($LASTEXITCODE -ne 0) { throw "$Command failed ($LASTEXITCODE)" } }
if ($env:OS -ne 'Windows_NT') { throw 'Run this script on the company Windows development PC.' }
if (-not (Test-Path Cargo.lock) -or -not (Test-Path package-lock.json)) { throw 'Run scripts/bootstrap.ps1 to resolve real dependencies first.' }
Run 'cargo' @('fmt','--all')
Run 'cargo' @('check','--workspace','--all-targets','--locked')
Run 'cargo' @('test','--workspace','--locked')
Run 'npm' @('test')
Run 'npm' @('run','check:web')
Run 'npm' @('run','build:web')
Run 'cargo' @('build','--release','--locked','-p','rc-agent','-p','rc-media')
if ($NativeMedia) {
    # Deliberately opt-in: SDK DLLs are NOT copied/bundled by this script.
    if (-not (Get-Command 'gst-inspect-1.0.exe' -ErrorAction SilentlyContinue)) { throw 'Configure the reviewed x64 MSVC GStreamer 1.24+ runtime + development SDK and PATH first. See docs/compatibility/MEDIA.md.' }
    Run 'cargo' @('check','--locked','-p','rc-media','--features','native-media','--all-targets')
    Run 'cargo' @('test','--locked','-p','rc-media','--features','native-media')
    Run 'cargo' @('build','--release','--locked','-p','rc-media','--features','native-media')
    & '.\target\release\rc-media.exe' --probe
    if ($LASTEXITCODE -ne 0) { throw 'Media probe process failed.' }
}
if ($Desktop) {
    Push-Location 'apps/desktop/src-tauri'
    try { Run 'cargo' @('fmt','--all'); Run 'cargo' @('check','--locked'); Run 'cargo' @('build','--release','--locked') } finally { Pop-Location }
}
Write-Host 'Built source binaries only. No installer, signing, firewall/VPN changes, or automatic background installation.'
Write-Host 'A build/probe pass is NOT proof of real two-PC video, input, or performance. Run the remaining release gates.'
