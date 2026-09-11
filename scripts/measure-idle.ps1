[CmdletBinding()] param([int[]]$ProcessIds, [int]$Samples = 600, [string]$Output = 'docs/test-results/windows-idle.csv')
$ErrorActionPreference = 'Stop'; Set-StrictMode -Version Latest
if (-not $ProcessIds -or $Samples -lt 10) { throw 'Pass exact Agent/UI/WebView2 PIDs and at least 10 samples.' }
$Output = [IO.Path]::GetFullPath($Output); [IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($Output)) | Out-Null
$logical = [Environment]::ProcessorCount
$previous = @{}; $rows = [Collections.Generic.List[object]]::new()
for ($i = 0; $i -lt $Samples; $i++) {
    $now = [Diagnostics.Stopwatch]::GetTimestamp()
    foreach ($id in $ProcessIds) {
        $p = Get-Process -Id $id -ErrorAction Stop
        $key = "$id/$($p.StartTime.ToUniversalTime().Ticks)"
        $cpu = $p.TotalProcessorTime.TotalSeconds
        $pct = $null
        if ($previous.ContainsKey($key)) {
            $last = $previous[$key]
            $elapsed = ($now - $last.Tick) / [double][Diagnostics.Stopwatch]::Frequency
            $pct = 100.0 * ($cpu - $last.Cpu) / ($elapsed * $logical)
        }
        $previous[$key] = @{ Tick=$now; Cpu=$cpu }
        $rows.Add([PSCustomObject]@{
            AtUtc=[DateTime]::UtcNow.ToString('o'); Pid=$id; Name=$p.ProcessName;
            StartTimeUtc=$p.StartTime.ToUniversalTime().ToString('o'); CpuMachinePct=$pct;
            PrivateBytes=$p.PrivateMemorySize64; WorkingSetBytes=$p.WorkingSet64;
            Threads=$p.Threads.Count; Handles=$p.HandleCount
        })
    }
    Start-Sleep -Milliseconds 1000
}
$rows | Export-Csv -NoTypeInformation -Encoding UTF8 -Path $Output
Write-Host "Raw samples written to $Output. PrivateBytes and WorkingSetBytes are NOT interchangeable."
Write-Host 'This script does not decide PASS; apply SPEC environment/warm-up/exclusions and aggregate all product PIDs.'
