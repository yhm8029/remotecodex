[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$Identity
)

$ErrorActionPreference = 'Stop'

Add-Type -Namespace RemoteCodex -Name Win32 -MemberDefinition @"
[System.Runtime.InteropServices.DllImport("user32.dll", SetLastError = true)]
public static extern bool PostMessageW(System.IntPtr hWnd, uint Msg, System.IntPtr wParam, System.IntPtr lParam);
"@

$payload = Get-Content -LiteralPath $Identity -Raw -Encoding UTF8 | ConvertFrom-Json

$expectedId        = [int]$payload.Id
$expectedStartTick = [string]$payload.StartTicks
$expectedPath      = [string]$payload.Exe

$proc = Get-Process -Id $expectedId -ErrorAction Stop
$actualStartTicks = $proc.StartTime.ToUniversalTime().Ticks.ToString()
$actualPath       = $proc.MainModule.FileName

if ($actualStartTicks -ne $expectedStartTick) {
    throw "StartTime mismatch for PID $expectedId. Expected '$expectedStartTick', got '$actualStartTicks'."
}
if ($actualPath -ne $expectedPath) {
    throw "Path mismatch for PID $expectedId. Expected '$expectedPath', got '$actualPath'."
}

$proc.Refresh()
if ($proc.MainWindowHandle -eq [System.IntPtr]::Zero) {
    throw "MainWindowHandle is zero for PID $expectedId; cannot post WM_CLOSE."
}

$recheck = Get-Process -Id $expectedId -ErrorAction Stop
$recheckStartTicks = $recheck.StartTime.ToUniversalTime().Ticks.ToString()
$recheckPath       = $recheck.MainModule.FileName

$handleForClose = [System.IntPtr]::Zero
if ($recheckStartTicks -eq $expectedStartTick -and
    $recheckPath -eq $expectedPath -and
    $recheck.MainWindowHandle -ne [System.IntPtr]::Zero) {

    $handleForClose = $recheck.MainWindowHandle
    $WM_CLOSE = [uint32]0x0010
    $posted = [RemoteCodex.Win32]::PostMessageW(
        $handleForClose,
        $WM_CLOSE,
        [System.IntPtr]::Zero,
        [System.IntPtr]::Zero
    )

    if (-not $posted) {
        $err = [System.Runtime.InteropServices.Marshal]::GetLastWin32Error()
        throw "PostMessageW(WM_CLOSE) failed for PID $expectedId (handle $handleForClose). Win32 error: $err"
    }
}
else {
    throw 'Process identity or window handle changed; close cancelled'
}

if (-not $recheck.WaitForExit(10000)) {
    throw "PID $expectedId did not exit within 10000ms after WM_CLOSE. Process left running."
}

$result = [ordered]@{
    closed     = $true
    Id         = $expectedId
    StartTicks = $expectedStartTick
}
$result | ConvertTo-Json -Compress
