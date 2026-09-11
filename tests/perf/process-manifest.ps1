#Requires -Version 5.1
[CmdletBinding()]
param(
    [Parameter(Mandatory)] [string] $AgentManifest,
    [Parameter(Mandatory)] [string] $Sessions,
    [Parameter(Mandatory)] [string] $Output
)
$ErrorActionPreference = 'Stop'

$agents = (Get-Content -LiteralPath $AgentManifest -Raw -Encoding UTF8) | ConvertFrom-Json
$sessionRecords = (Get-Content -LiteralPath $Sessions -Raw -Encoding UTF8) | ConvertFrom-Json

$bySessionId = @{}
foreach ($s in $sessionRecords) {
    if ($null -ne $s.session_id) { $bySessionId[[string]$s.session_id] = $s }
}

$results = New-Object System.Collections.Generic.List[object]
$seen = @{}

foreach ($a in $agents) {
    if (-not $a.Id) { throw "Agent missing Id" }
    $id = [string]$a.Id
    if ($seen.ContainsKey($id)) { throw "Duplicate Id: $id" }
    $seen[$id] = $true

    $expected = $a.StartTicks.ToString()
    $group = [string]$a.Group
    $label = [string]$a.Label

    if ($group -ne 'product') { throw 'Agent manifest must contain product roots only' }
    if ($group -eq 'product') {
        $proc = Get-Process -Id $id -ErrorAction Stop
        if ($null -eq $proc) { throw "No process $id" }
        $actual = $proc.StartTime.ToUniversalTime().Ticks.ToString()
        if ($actual -ne $expected) { throw "Ticks mismatch $id expected=$expected actual=$actual" }
        $results.Add([pscustomobject]@{
            Id = [int]$id
            StartTicks = $actual
            Group = 'product'
            Label = $label
        })
    }
}
foreach ($s in $sessionRecords) {
    if ($null -eq $s.pid -or [int]$s.pid -le 0) { throw 'Invalid or duplicate owned session identity' }
    if ([string]::IsNullOrWhiteSpace([string]$s.process_created) -or -not ([string]$s.process_created -match '^\d+$')) { throw 'Invalid or duplicate owned session identity' }
    if ([string]::IsNullOrWhiteSpace([string]$s.session_id)) { throw 'Invalid or duplicate owned session identity' }
    $pidKey = [string]$s.pid
    if ($seen.ContainsKey($pidKey)) { throw 'Invalid or duplicate owned session identity' }
    $proc = Get-Process -Id ([int]$s.pid) -ErrorAction Stop
    $startStr = $proc.StartTime.ToUniversalTime().ToFileTimeUtc().ToString()
    if ($startStr -ne [string]$s.process_created) { throw "PID $($s.pid) start time mismatch: process=$startStr manifest=$($s.process_created)" }
    $seen[$pidKey] = $true
    $results.Add([pscustomobject]@{
        Id         = [int]$s.pid
        StartTicks = $proc.StartTime.ToUniversalTime().Ticks.ToString()
        Group      = 'workload'
        Label      = 'owned-cmd-' + [string]$s.session_id
    })
}
$json = ConvertTo-Json -InputObject $results.ToArray() -Depth 5

$fs = [IO.File]::Open($Output, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
try {
    $sw = [IO.StreamWriter]::new($fs, [Text.UTF8Encoding]::new($false))
    try { $sw.Write($json) } finally { $sw.Dispose() }
}
finally { $fs.Dispose() }