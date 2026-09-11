param([string]$Identity,[string]$Output)
$ErrorActionPreference = 'Stop'
$root = ConvertFrom-Json -InputObject ([IO.File]::ReadAllText($Identity))
$rootProc = Get-Process -Id $root.Id -ErrorAction Stop
$rootTicks = $rootProc.StartTime.ToUniversalTime().Ticks.ToString()
if ($rootTicks -ne $root.StartTicks) { throw "Root StartTime mismatch: $rootTicks vs $($root.StartTicks)" }
if ($rootProc.Path -ne $root.Exe) { throw "Root Path mismatch: $($rootProc.Path) vs $($root.Exe)" }
$wmi = Get-CimInstance -ClassName Win32_Process
$records = [System.Collections.Generic.List[object]]::new()
$queue = [System.Collections.Generic.Queue[object]]::new()
$rootRecord = [pscustomobject]@{
    Id = [int]$rootProc.Id
    StartTicks = $rootTicks
    Exe = $rootProc.Path
    Group = "product"
    Label = "tauri-ui"
}
$records.Add($rootRecord)
$queue.Enqueue($rootRecord)
while ($queue.Count -gt 0) {
    $current = $queue.Dequeue()
    $parentProc = Get-Process -Id $current.Id -ErrorAction Stop
    $parentTicks = $parentProc.StartTime.ToUniversalTime().Ticks.ToString()
    if ($parentTicks -ne $current.StartTicks) { throw "Parent StartTime mismatch for PID $($current.Id)" }
    $children = $wmi | Where-Object { $_.ParentProcessId -eq $current.Id -and $_.Name -eq 'msedgewebview2.exe' }
    foreach ($child in $children) {
        $childProc = Get-Process -Id $child.ProcessId -ErrorAction Stop
        if ($childProc.Path -ne $child.ExecutablePath) { throw "Child Path mismatch for PID $($child.ProcessId)" }
        $childStart = $childProc.StartTime.ToUniversalTime()
        $parentStart = $parentProc.StartTime.ToUniversalTime()
        if ($childStart -lt $parentStart) { throw "Child start precedes parent for PID $($child.ProcessId)" }
        $childRecord = [pscustomobject]@{
            Id = [int]$childProc.Id
            StartTicks = $childStart.Ticks.ToString()
            Exe = $childProc.Path
            Group = "product"
            Label = "webview2"
        }
        $records.Add($childRecord)
        $queue.Enqueue($childRecord)
    }
}
foreach ($rec in $records) {
    $verify = Get-Process -Id $rec.Id -ErrorAction Stop
    if ($verify.StartTime.ToUniversalTime().Ticks.ToString() -ne $rec.StartTicks) { throw "Final verify StartTime mismatch for PID $($rec.Id)" }
    if ($verify.Path -ne $rec.Exe) { throw "Final verify Path mismatch for PID $($rec.Id)" }
}
if ($records.Count -lt 2) { throw "Records count $($records.Count) less than 2" }
$json = ConvertTo-Json -InputObject $records.ToArray() -Depth 5
$stream = $null
$writer = $null
try {
    $stream = [IO.File]::Open($Output,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write)
    $writer = New-Object System.IO.StreamWriter($stream, [System.Text.UTF8Encoding]::new($false))
    $writer.Write($json)
} finally {
    if ($writer) { $writer.Dispose() }
    elseif ($stream) { $stream.Dispose() }
}
