[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [string]$Manifest,

    [Parameter(Mandatory)]
    [string]$Output,

    [ValidateRange(1,86400)]
    [int]$DurationSeconds = 600,

    [ValidateRange(250,10000)]
    [int]$IntervalMs = 1000,
    [switch]$IncludeGpu
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

. (Join-Path -Path $PSScriptRoot -ChildPath 'resource-sample.ps1')
if ($IncludeGpu) { . (Join-Path $PSScriptRoot 'gpu-sample.ps1') }

# --- Read & validate manifest ---
$rawJson = [System.IO.File]::ReadAllText((Resolve-Path -LiteralPath $Manifest).Path, [System.Text.Encoding]::UTF8)
$parsedManifest = ConvertFrom-Json -InputObject $rawJson
$manifestItems = @($parsedManifest)
if ($manifestItems.Count -lt 1 -or $manifestItems.Count -gt 256) {
    throw "Manifest must contain 1..256 identities (found $($manifestItems.Count))."
}

$allowedGroups = @('product','workload','transport','harness')
$ticksPattern  = '^[0-9]+$'
$identities    = New-Object System.Collections.Generic.List[object]
$seenIds       = [System.Collections.Generic.HashSet[int]]::new()

foreach ($item in $manifestItems) {
    if ($null -eq $item.Id -or $null -eq $item.StartTicks -or $null -eq $item.Group -or $null -eq $item.Label) {
        throw "Identity missing required field (Id/StartTicks/Group/Label)."
    }
    if ($item.Id -isnot [int] -or $item.Id -lt 1) {
        throw "Identity Id must be a positive int (got '$($item.Id)')."
    }
    if (-not ($seenIds.Add([int]$item.Id))) {
        throw "Duplicate identity Id detected: $($item.Id)."
    }
    $startTicksStr = [string]$item.StartTicks
    if ($startTicksStr -notmatch $ticksPattern) {
        throw "Identity Id $($item.Id) has invalid StartTicks '$startTicksStr' (expected digits only)."
    }
    if ($allowedGroups -notcontains [string]$item.Group) {
        throw "Identity Id $($item.Id) has invalid Group '$($item.Group)'."
    }
    $labelStr = [string]$item.Label
    if ([string]::IsNullOrWhiteSpace($labelStr) -or $labelStr.Length -gt 80) {
        throw "Identity Id $($item.Id) has invalid Label (must be non-empty and <=80 chars)."
    }

    $identities.Add([pscustomobject]@{
        Id         = [int]$item.Id
        StartTicks = $startTicksStr
        Group      = [string]$item.Group
        Label      = $labelStr
    })
}

# --- Output path validation ---
if ([string]::IsNullOrWhiteSpace($Output)) {
    throw "Output path is required."
}
$fullOutput = [System.IO.Path]::GetFullPath($Output)
if ([System.IO.File]::Exists($fullOutput)) {
    throw "Output file already exists: $fullOutput"
}
$parentDir = [System.IO.Path]::GetDirectoryName($fullOutput)
if (-not [string]::IsNullOrEmpty($parentDir)) {
    [void](New-Item -ItemType Directory -Path $parentDir -Force)
}

# --- Open writer (CreateNew, no overwrite) ---
$fileStream = [System.IO.File]::Open($fullOutput, [System.IO.FileMode]::CreateNew, [System.IO.FileAccess]::Write, [System.IO.FileShare]::Read)
try {
    $writer = [System.IO.StreamWriter]::new($fileStream, [System.Text.UTF8Encoding]::new($false))
    try {

        $startedUtc = [DateTime]::UtcNow.ToString('yyyy-MM-ddTHH:mm:ss.fffZ')
        $meta = [ordered]@{
            type                = 'metadata'
            started_utc         = $startedUtc
            duration_seconds    = $DurationSeconds
            interval_ms         = $IntervalMs
            logical_processors  = [Environment]::ProcessorCount
            identities          = $identities.ToArray()
            gpu_memory_requested = [bool]$IncludeGpu
            gpu_memory_scope = 'per-process CIM adapter sum; null means unavailable, not zero'
        }
        $writer.WriteLine((ConvertTo-Json -InputObject $meta -Depth 6 -Compress))

        $previous   = @{}
        $watch      = [System.Diagnostics.Stopwatch]::StartNew()
        $sampleIndex = 0

        while ($true) {
            $gpuMemory = @{}
            if ($IncludeGpu) { $gpuMemory = Read-GpuMemory -Identities $identities }
            $rows = Read-ResourceSample -Identities $identities -Previous $previous -LogicalProcessors ([Environment]::ProcessorCount)

            if ($IncludeGpu) { foreach ($row in $rows) { if ($row.status -eq 'ok') { $row | Add-Member -NotePropertyName gpu_memory -NotePropertyValue $gpuMemory[[int]$row.pid] } } }
            $sample = [ordered]@{
                type             = 'sample'
                index            = $sampleIndex
                elapsed_seconds  = [double]$watch.Elapsed.TotalSeconds
                processes        = $rows
            }
            $writer.WriteLine((ConvertTo-Json -InputObject $sample -Depth 6 -Compress))

            $sampleIndex++
            $writer.Flush()

            if ($watch.Elapsed.TotalSeconds -ge $DurationSeconds) { break }

            $remainingMs = $DurationSeconds * 1000 - [int]$watch.ElapsedMilliseconds
            $sleepMs = [Math]::Min($IntervalMs, $remainingMs)
            if ($sleepMs -lt 1) { $sleepMs = 1 }
            Start-Sleep -Milliseconds $sleepMs
        }
    }
    finally {
        $writer.Dispose()
    }
}
finally {
    $fileStream.Dispose()
}

Write-Output ("Wrote {0} sample(s) to {1}" -f $sampleIndex, $fullOutput)
