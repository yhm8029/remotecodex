param([string]$Manifest)

$raw = ConvertFrom-Json -InputObject ([IO.File]::ReadAllText($Manifest))
$records = @($raw)

$ids = $records | ForEach-Object { [int]$_.Id }
$ticks = @{}
foreach ($r in $records) {
    $ticks[[int]$r.Id] = [string]$r.StartTicks
}

$sw = [System.Diagnostics.Stopwatch]::StartNew()
$elapsed = $sw.Elapsed
$limit = [TimeSpan]::FromSeconds(10)

$originalExited = $true
$stillPresent = New-Object System.Collections.Generic.List[int]

while ($true) {
    $originalExited = $true
    $stillPresent.Clear()
    $snapshot = Get-Process -Id $ids -ErrorAction SilentlyContinue
    $current = @{}
    if ($null -ne $snapshot) {
        foreach ($p in $snapshot) {
            $current[[int]$p.Id] = [string]$p.StartTime.ToUniversalTime().Ticks
        }
    }

    foreach ($id in $ids) {
        if (-not $current.ContainsKey($id)) {
            # absent -> exited
            continue
        }
        if ($current[$id] -ne $ticks[$id]) {
            # present but different start ticks -> original replaced
            continue
        }
        # original still present
        $originalExited = $false
        $stillPresent.Add($id) | Out-Null
    }

    if ($originalExited) {
        break
    }

    if ($sw.Elapsed -ge $limit) {
        break
    }

    Start-Sleep -Milliseconds 100
}

if (-not $originalExited) {
    $msg = ($stillPresent | ForEach-Object { $_ }) -join ','
    throw "Original processes still present: $msg"
}

$result = [ordered]@{
    all_original_processes_exited = $true
    checked = $records.Count
}
ConvertTo-Json -InputObject $result -Compress
