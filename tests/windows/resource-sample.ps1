#requires -Version 5.1
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$script:resourceSamplePath = Join-Path -Path $PSScriptRoot -ChildPath '..\..\scripts\resource-sample.ps1'
. $script:resourceSamplePath

$self = Get-Process -Id $PID
$identity = [pscustomobject]@{
    Id         = $PID
    StartTicks = $self.StartTime.ToUniversalTime().Ticks.ToString()
    Group      = 'harness'
    Label      = 'resource-sampler-test'
}

function New-BadAssertionException {
    param([string]$Message)
    throw "Assertion failed: $Message"
}

function Test-AssertTrue {
    param([bool]$Condition, [string]$Message)
    if (-not $Condition) { New-BadAssertionException -Message $Message }
}

$previous = @{}
$logical = [Environment]::ProcessorCount

# ----- First sample -----
$rows = Read-ResourceSample -Identities @($identity) -Previous $previous -LogicalProcessors $logical

Test-AssertTrue ($rows.Count -eq 1) -Message 'rows.Count must be 1'
$row = $rows[0]
Test-AssertTrue ($row.status -eq 'ok') -Message "row.status must be 'ok' but was '$($row.status)'"
Test-AssertTrue ($null -eq $row.cpu_machine_pct) -Message 'cpu_machine_pct must be null on first sample'
Test-AssertTrue ($row.private_working_set_bytes -ge 0) -Message 'private_working_set_bytes must be >= 0'
Test-AssertTrue ($row.private_bytes -gt 0) -Message 'private_bytes must be > 0'
Test-AssertTrue ($row.working_set_bytes -gt 0) -Message 'working_set_bytes must be > 0'
Test-AssertTrue ($row.started_ticks -eq $identity.StartTicks) -Message 'started_ticks must equal identity.StartTicks exactly'

# ----- Second sample after delay: CPU must be measurable -----
Start-Sleep -Milliseconds 100
$rows2 = Read-ResourceSample -Identities @($identity) -Previous $previous -LogicalProcessors $logical

Test-AssertTrue ($rows2.Count -eq 1) -Message 'second sample rows.Count must be 1'
$row2 = $rows2[0]
Test-AssertTrue ($null -ne $row2.cpu_machine_pct) -Message 'cpu_machine_pct must not be null on second sample'
Test-AssertTrue ($row2.cpu_machine_pct -ge 0) -Message 'cpu_machine_pct must be >= 0 on second sample'

# ----- Wrong identity with same PID but mismatched StartTicks must yield identity_changed -----
$badIdentity = [pscustomobject]@{
    Id         = $PID
    StartTicks = '1'
    Group      = 'harness'
    Label      = 'resource-sampler-test'
}

$rowsBad = Read-ResourceSample -Identities @($badIdentity) -Previous @{} -LogicalProcessors $logical
Test-AssertTrue ($rowsBad.Count -eq 1) -Message 'bad identity sample rows.Count must be 1'
$rowBad = $rowsBad[0]
Test-AssertTrue ($rowBad.status -eq 'identity_changed') -Message "wrong-identity row.status must be 'identity_changed' but was '$($rowBad.status)'"
Test-AssertTrue (-not ($rowBad.PSObject.Properties.Name -contains 'private_bytes')) -Message 'identity_changed row must not expose private_bytes property'

# ----- Non-existent PID must yield unavailable -----
$missingIdentity = [pscustomobject]@{
    Id         = 2147483647
    StartTicks = '1'
    Group      = 'harness'
    Label      = 'resource-sampler-test'
}

$rowsMissing = Read-ResourceSample -Identities @($missingIdentity) -Previous @{} -LogicalProcessors $logical
Test-AssertTrue ($rowsMissing.Count -eq 1) -Message 'missing-pid sample rows.Count must be 1'
$rowMissing = $rowsMissing[0]
Test-AssertTrue ($rowMissing.status -eq 'unavailable') -Message "missing-pid row.status must be 'unavailable' but was '$($rowMissing.status)'"

Write-Output 'PASS resource identity, missing PID and distinct memory counters'