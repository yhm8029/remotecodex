function Read-GpuMemory {
    [CmdletBinding()]
    param(
        [object[]]$Identities
    )

    $result = @{}
    if (-not $Identities -or $Identities.Count -eq 0) { return $result }

    $allowed = @{}
    foreach ($id in $Identities) {
        if ($null -ne $id -and $id.PSObject.Properties.Match('Id').Count -gt 0) {
            $val = $id.Id
            if ($null -ne $val) { $allowed[[int]$val] = $true }
        }
    }
    if ($allowed.Count -eq 0) { return $result }

    try {
        $rows = Get-CimInstance -ClassName Win32_PerfFormattedData_GPUPerformanceCounters_GPUProcessMemory -OperationTimeoutSec 5 -ErrorAction Stop
    } catch {
        return $result
    }

    foreach ($row in $rows) {
        $name = [string]$row.Name
        if ($name -notmatch '^pid_(\d+)_luid_') { continue }
        $processIdValue = [int]$Matches[1]
        if (-not $allowed.ContainsKey($processIdValue)) { continue }

        if ($null -eq $row.DedicatedUsage -or $null -eq $row.SharedUsage) { continue }
        $dedicated = ([uint64]0)
        $shared = ([uint64]0)
        if ($null -ne $row.DedicatedUsage) { $dedicated = [uint64]$row.DedicatedUsage }
        if ($null -ne $row.SharedUsage)    { $shared = [uint64]$row.SharedUsage }

        if (-not $result.ContainsKey($processIdValue)) {
            $result[$processIdValue] = @{ dedicated_bytes = ([uint64]0); shared_bytes = ([uint64]0); adapters = 0 }
        }
        $entry = $result[$processIdValue]
        $entry.dedicated_bytes += $dedicated
        $entry.shared_bytes += $shared
        $entry.adapters += 1
    }

    return $result
}
