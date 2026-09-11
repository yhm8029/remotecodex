#Requires -Version 5.1
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][int]$Id,
    [Parameter(Mandatory = $true)][string]$StartTicks,
    [Parameter(Mandatory = $true)][string]$Exe,
    [Parameter(Mandatory = $true)][string]$Output
)

$ErrorActionPreference = 'Stop'
$process = Get-Process -Id $Id -ErrorAction Stop
$process.Refresh()
$actualTicks = $process.StartTime.ToUniversalTime().Ticks.ToString()
$actualExe = [string]$process.MainModule.FileName
if ($actualTicks -ne $StartTicks) {
    throw "StartTicks mismatch for PID $Id. Expected '$StartTicks', got '$actualTicks'."
}
if ($actualExe -ne $Exe) {
    throw "Executable mismatch for PID $Id. Expected '$Exe', got '$actualExe'."
}

$json = ConvertTo-Json -InputObject ([ordered]@{
    Id = $Id
    StartTicks = $actualTicks
    Exe = $actualExe
    live = $true
}) -Compress
$bytes = [Text.Encoding]::UTF8.GetBytes($json + "`n")
$stream = [IO.File]::Open($Output, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
try {
    $stream.Write($bytes, 0, $bytes.Length)
    $stream.Flush()
}
finally {
    $stream.Dispose()
}
