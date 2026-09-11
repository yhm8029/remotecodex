param([string]$Identity,[int]$Port,[string]$Output)
$ErrorActionPreference = 'Stop'

& (Join-Path $PSScriptRoot 'native-descendants.ps1') -Identity $Identity -Output $Output
$parsed = ConvertFrom-Json -InputObject ([IO.File]::ReadAllText($Output))
$records = @($parsed)
$listeners = @(Get-NetTCPConnection -LocalPort $Port -State Listen -ErrorAction Stop)
if ($listeners.Count -lt 1) { throw "No listener found on port $Port" }
$webviews = @($records | Where-Object { $_.Label -eq 'webview2' })
foreach ($listener in $listeners) {
    if ([string]$listener.LocalAddress -notin @('127.0.0.1','::1')) { throw "Non-loopback listener: $($listener.LocalAddress)" }
    $record = $webviews | Where-Object { [int]$_.Id -eq [int]$listener.OwningProcess } | Select-Object -First 1
    if (-not $record) { throw "Listener PID $($listener.OwningProcess) is not an owned WebView2 process" }
    $process = Get-Process -Id ([int]$record.Id) -ErrorAction Stop
    if ($process.StartTime.ToUniversalTime().Ticks.ToString() -ne [string]$record.StartTicks) { throw "StartTicks mismatch for PID $($record.Id)" }
    if ($process.Path -ne [string]$record.Exe) { throw "Path mismatch for PID $($record.Id)" }
}
$result = [ordered]@{ verified = $true; port = $Port; listenerIds = @($listeners | ForEach-Object { [int]$_.OwningProcess }) }
$result | ConvertTo-Json -Compress
