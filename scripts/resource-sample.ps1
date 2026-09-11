function Read-ResourceSample {
    [CmdletBinding()]
    param(
        [object[]]$Identities,
        [hashtable]$Previous,
        [int]$LogicalProcessors
    )

    $rows = @()
    $stopwatchFrequency = [System.Diagnostics.Stopwatch]::Frequency

    $filterParts = foreach ($identity in $Identities) {
        'IDProcess = ' + [int]$identity.Id
    }
    $filter = ($filterParts -join ' OR ')

    $rawCim = @{}
    try {
        $instances = Get-CimInstance -ClassName Win32_PerfRawData_PerfProc_Process -Filter $filter -OperationTimeoutSec 5
        foreach ($inst in $instances) {
            $rawCim[[int]$inst.IDProcess] = $inst
        }
    } catch {
        $rawCim = @{}
    }

    foreach ($identity in $Identities) {
        $pidValue = [int]$identity.Id
        $groupValue = [string]$identity.Group
        $labelValue = [string]$identity.Label
        $startTicksValue = [string]$identity.StartTicks

        $proc = $null
        $processError = $false
        try {
            $proc = Get-Process -Id $pidValue -ErrorAction Stop
            $proc.Refresh()
        } catch {
            $processError = $true
        }

        if ($processError -or $null -eq $proc) {
            $rows += [pscustomobject]@{
                status = 'unavailable'
                pid = $pidValue
                group = $groupValue
                label = $labelValue
            }
            continue
        }

        $currentTicks = $proc.StartTime.ToUniversalTime().Ticks.ToString()
        if ($currentTicks -ne $startTicksValue) {
            $rows += [pscustomobject]@{
                status = 'identity_changed'
                pid = $pidValue
                group = $groupValue
                label = $labelValue
            }
            continue
        }

        $rawRow = $rawCim[[int]$pidValue]
        $workingSetPrivate = $null
        if ($null -ne $rawRow) {
            $workingSetPrivate = $rawRow.WorkingSetPrivate
        }

        if ($null -eq $rawRow -or $null -eq $workingSetPrivate) {
            $rows += [pscustomobject]@{
                status = 'unavailable'
                pid = $pidValue
                group = $groupValue
                label = $labelValue
            }
            continue
        }

        $now = [System.Diagnostics.Stopwatch]::GetTimestamp()
        $cpu = $proc.TotalProcessorTime.TotalSeconds
        $key = ('{0}/{1}' -f $pidValue, $startTicksValue)
        $prev = $Previous[$key]

        $cpuMachinePct = $null
        if ($null -ne $prev) {
            $deltaCpu = $cpu - [double]$prev.Cpu
            $deltaTicks = ($now - [long]$prev.Tick)
            if ($deltaTicks -gt 0) {
                $secondsElapsed = $deltaTicks / $stopwatchFrequency
                $cpuMachinePct = [math]::Max(0.0, 100.0 * $deltaCpu / $secondsElapsed / $LogicalProcessors)
            } else {
                $cpuMachinePct = 0.0
            }
        }

        $Previous[$key] = [pscustomobject]@{
            Tick = $now
            Cpu = $cpu
        }

        $rows += [pscustomobject]@{
            status = 'ok'
            pid = $pidValue
            group = $groupValue
            label = $labelValue
            started_ticks = $startTicksValue
            monotonic_tick = $now
            at_utc = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ss.fffZ')
            cpu_machine_pct = $cpuMachinePct
            private_working_set_bytes = [long]$workingSetPrivate
            private_bytes = [long]$proc.PrivateMemorySize64
            working_set_bytes = [long]$proc.WorkingSet64
            threads = $proc.Threads.Count
            handles = $proc.HandleCount
        }
    }

    return ,$rows
}
