#Requires -Version 5.1
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$Directory,

    [ValidateRange(2, 1920)]
    [int]$Width = 1280,

    [ValidateRange(2, 1080)]
    [int]$Height = 720,

    [ValidateRange(1, 30)]
    [int]$Fps = 15,

    [ValidateRange(1, 100000)]
    [int]$Bitrate = 2500,

    [ValidateRange(1, 600)]
    [int]$Seconds = 5,
    [string]$AgentManifest
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

if ($env:OS -ne 'Windows_NT') {
    throw 'Windows is required for the owned media smoke benchmark.'
}

$repoRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
$runnerExe = [IO.Path]::GetFullPath((Join-Path $repoRoot 'runtime\build-media\x86_64-pc-windows-msvc\release\examples\owned-window-benchmark.exe'))
$encoderExe = [IO.Path]::GetFullPath((Join-Path $repoRoot 'runtime\build-media\x86_64-pc-windows-msvc\release\examples\native-capture-benchmark.exe'))
$closeHelper = [IO.Path]::GetFullPath((Join-Path $repoRoot 'tests\perf\native-close.ps1'))

function Assert-File {
    param([Parameter(Mandatory = $true)][string]$Path)
    if (-not [IO.File]::Exists($Path)) {
        throw "Required file does not exist: $Path"
    }
}

function Write-ExclusiveJson {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)]$Value
    )

    $json = ConvertTo-Json -InputObject $Value -Depth 30 -Compress
    $bytes = [Text.Encoding]::UTF8.GetBytes($json + "`n")
    $stream = [IO.File]::Open($Path, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
    try {
        $stream.Write($bytes, 0, $bytes.Length)
        $stream.Flush()
    }
    finally {
        $stream.Dispose()
    }
}

function Get-ProcessIdentity {
    param(
        [Parameter(Mandatory = $true)][int]$Id,
        [Parameter(Mandatory = $true)][string]$StartTicks,
        [Parameter(Mandatory = $true)][string]$Exe
    )

    $process = Get-Process -Id $Id -ErrorAction Stop
    $process.Refresh()
    $actualTicks = $process.StartTime.ToUniversalTime().Ticks.ToString()
    $actualExe = [string]$process.MainModule.FileName
    if ($actualTicks -ne $StartTicks) {
        throw "Process start identity changed for PID ${Id}: expected $StartTicks, got $actualTicks."
    }
    if ($actualExe -ne $Exe) {
        throw "Process executable identity changed for PID ${Id}: expected '$Exe', got '$actualExe'."
    }
    return $process
}

function Invoke-NativeEncoder {
    param(
        [Parameter(Mandatory = $true)][string]$Exe,
        [Parameter(Mandatory = $true)][string]$SdkRoot,
        [Parameter(Mandatory = $true)][UInt64]$Hwnd,
        [Parameter(Mandatory = $true)][int]$Width,
        [Parameter(Mandatory = $true)][int]$Height,
        [Parameter(Mandatory = $true)][int]$Fps,
        [Parameter(Mandatory = $true)][int]$Bitrate,
        [Parameter(Mandatory = $true)][int]$Seconds,
        [scriptblock]$OnStarted
    )

    $startInfo = New-Object System.Diagnostics.ProcessStartInfo
    $startInfo.FileName = $Exe
    $startInfo.Arguments = '{0} {1} {2} {3} {4} {5}' -f $Hwnd, $Width, $Height, $Fps, $Bitrate, $Seconds
    $startInfo.WorkingDirectory = [IO.Path]::GetDirectoryName($Exe)
    $startInfo.UseShellExecute = $false
    $startInfo.CreateNoWindow = $true
    $startInfo.RedirectStandardOutput = $true
    $startInfo.RedirectStandardError = $true

    $sdkBin = Join-Path $SdkRoot 'bin'
    $sdkPlugin = Join-Path $SdkRoot 'lib\gstreamer-1.0'
    $sdkScanner = Join-Path $SdkRoot 'libexec\gstreamer-1.0\gst-plugin-scanner.exe'
    if (-not [IO.Directory]::Exists($sdkBin)) {
        throw "GStreamer SDK bin directory does not exist: $sdkBin"
    }
    $oldPath = [string]$startInfo.EnvironmentVariables['PATH']
    $startInfo.EnvironmentVariables['PATH'] = "$sdkBin;$oldPath"
    $startInfo.EnvironmentVariables['GST_PLUGIN_PATH_1_0'] = $sdkPlugin
    $startInfo.EnvironmentVariables['GST_PLUGIN_SYSTEM_PATH_1_0'] = ''
    $startInfo.EnvironmentVariables['GST_PLUGIN_SCANNER'] = $sdkScanner
    $startInfo.EnvironmentVariables['GSTREAMER_1_0_ROOT_MSVC_X86_64'] = $SdkRoot

    $process = New-Object System.Diagnostics.Process
    $process.StartInfo = $startInfo
    if (-not $process.Start()) {
        throw "Failed to start native capture encoder: $Exe"
    }
    $stdoutTask = $process.StandardOutput.ReadToEndAsync()
    $stderrTask = $process.StandardError.ReadToEndAsync()
    $resourceFailure = $null
    if ($OnStarted) { try { & $OnStarted $process } catch { $resourceFailure = $_ } }
    $process.WaitForExit()
    $stdout = $stdoutTask.Result
    $stderr = $stderrTask.Result
    $exitCode = $process.ExitCode
    $process.Dispose()

    if ($resourceFailure) { throw $resourceFailure }
    if ($exitCode -ne 0) {
        throw "Native capture encoder exited with code ${exitCode}: $($stderr.Trim())"
    }
    if ([string]::IsNullOrWhiteSpace($stdout)) {
        throw 'Native capture encoder returned no JSON report.'
    }
    try {
        return ($stdout.Trim() | ConvertFrom-Json -ErrorAction Stop)
    }
    catch {
        throw "Native capture encoder returned invalid JSON: $($_.Exception.Message)"
    }
}

$sdkRoot = Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'Programs\gstreamer\1.0\msvc_x86_64'
$fullDirectory = [IO.Path]::GetFullPath($Directory)
$parentDirectory = [IO.Path]::GetDirectoryName($fullDirectory)
if ([string]::IsNullOrWhiteSpace($parentDirectory) -or -not [IO.Directory]::Exists($parentDirectory)) {
    throw "Parent directory does not exist: $parentDirectory"
}
if ([IO.Directory]::Exists($fullDirectory) -or [IO.File]::Exists($fullDirectory)) {
    throw "Benchmark directory already exists: $fullDirectory"
}
$null = New-Item -ItemType Directory -Path $fullDirectory -ErrorAction Stop

$resultPath = Join-Path $fullDirectory 'result.json'
$identityPath = Join-Path $fullDirectory 'runner-identity.json'
$recordPath = Join-Path $fullDirectory 'window-record.json'
$runnerErrorPath = Join-Path $fullDirectory 'runner.stderr.log'
$startedUtc = [DateTime]::UtcNow.ToString('o')
$runnerProcess = $null
$identity = $null
$record = $null
$report = $null
$runnerHash = $null
$encoderHash = $null
$failures = New-Object System.Collections.Generic.List[string]

try {
    Assert-File -Path $runnerExe
    Assert-File -Path $encoderExe
    Assert-File -Path $closeHelper
    $runnerHash = (Get-FileHash -LiteralPath $runnerExe -Algorithm SHA256).Hash
    $encoderHash = (Get-FileHash -LiteralPath $encoderExe -Algorithm SHA256).Hash

    $runnerArguments = '"{0}" {1} {2} {3}' -f $recordPath, $Width, $Height, $Fps
    $runnerProcess = Start-Process -FilePath $runnerExe `
        -ArgumentList $runnerArguments `
        -WindowStyle Hidden `
        -PassThru `
        -RedirectStandardError $runnerErrorPath

    $captureId = [int]$runnerProcess.Id
    $captureUtcTicks = $runnerProcess.StartTime.ToUniversalTime().Ticks.ToString()
    $captureExe = $runnerExe
    $identity = [ordered]@{
        Id = $captureId
        StartTicks = $captureUtcTicks
        Exe = $captureExe
    }
    Write-ExclusiveJson -Path $identityPath -Value $identity

    $recordDeadline = [DateTime]::UtcNow.AddSeconds(10)
    while ([DateTime]::UtcNow -lt $recordDeadline) {
        if ([IO.File]::Exists($recordPath)) {
            try {
                $stream = [IO.File]::Open($recordPath, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::ReadWrite)
                $reader = [IO.StreamReader]::new($stream, [Text.Encoding]::UTF8)
                try { $recordText = $reader.ReadToEnd() } finally { $reader.Dispose() }
                $candidate = ConvertFrom-Json -InputObject $recordText -ErrorAction Stop
                if ($null -ne $candidate.pid -and $null -ne $candidate.process_created -and
                    $null -ne $candidate.hwnd -and $null -ne $candidate.client_width -and
                    $null -ne $candidate.client_height -and $null -ne $candidate.fps) {
                    $record = $candidate
                    break
                }
            }
            catch {
                # The runner writes a complete record before entering its message loop.
                # Retry until the bounded deadline if the file is not complete yet.
            }
        }
        if ($runnerProcess.HasExited) {
            throw "Owned window runner exited before publishing its record (exit $($runnerProcess.ExitCode))."
        }
        Start-Sleep -Milliseconds 50
    }
    if ($null -eq $record) {
        throw 'Owned window runner did not publish a complete record within 10 seconds.'
    }

    $live = Get-ProcessIdentity -Id $captureId -StartTicks $captureUtcTicks -Exe $captureExe
    $expectedFileTime = $live.StartTime.ToUniversalTime().ToFileTimeUtc().ToString()
    if ([string]$record.pid -ne [string]$captureId) { throw "Record PID mismatch: $($record.pid) vs $captureId." }
    if ([string]$record.process_created -ne $expectedFileTime) { throw "Record process_created mismatch: $($record.process_created) vs $expectedFileTime." }
    if ([int]$record.client_width -ne $Width) { throw "Record client_width mismatch: $($record.client_width) vs $Width." }
    if ([int]$record.client_height -ne $Height) { throw "Record client_height mismatch: $($record.client_height) vs $Height." }
    if ([int]$record.fps -ne $Fps) { throw "Record fps mismatch: $($record.fps) vs $Fps." }

    if (-not ('RemoteCodex.MediaSmoke.Win32' -as [type])) {
        Add-Type -Namespace RemoteCodex.MediaSmoke -Name Win32 -MemberDefinition @"
[System.Runtime.InteropServices.StructLayout(System.Runtime.InteropServices.LayoutKind.Sequential)]
public struct RECT { public int left; public int top; public int right; public int bottom; }
[System.Runtime.InteropServices.DllImport("user32.dll", SetLastError=true)]
public static extern uint GetWindowThreadProcessId(System.IntPtr hWnd, out uint processId);
[System.Runtime.InteropServices.DllImport("user32.dll", SetLastError=true)]
public static extern bool GetClientRect(System.IntPtr hWnd, out RECT rect);
[System.Runtime.InteropServices.DllImport("user32.dll", SetLastError=true)]
public static extern bool IsWindow(System.IntPtr hWnd);
public static int GetClientWidth(System.IntPtr hWnd) {
    RECT rect;
    if (!GetClientRect(hWnd, out rect)) throw new System.ComponentModel.Win32Exception();
    return rect.right - rect.left;
}
public static int GetClientHeight(System.IntPtr hWnd) {
    RECT rect;
    if (!GetClientRect(hWnd, out rect)) throw new System.ComponentModel.Win32Exception();
    return rect.bottom - rect.top;
}
"@
    }

    $hwndValue = [UInt64]::Parse([string]$record.hwnd, [Globalization.CultureInfo]::InvariantCulture)
    $hwnd = [IntPtr]::new([int64]$hwndValue)
    $ownerPid = [uint32]0
    if (-not [RemoteCodex.MediaSmoke.Win32]::IsWindow($hwnd) -or
        [RemoteCodex.MediaSmoke.Win32]::GetWindowThreadProcessId($hwnd, [ref]$ownerPid) -eq 0 -or
        [int]$ownerPid -ne $captureId) {
        throw "Recorded HWND is not owned by the captured process: hwnd=$($record.hwnd), owner=$ownerPid, pid=$captureId."
    }
    $actualWidth = [RemoteCodex.MediaSmoke.Win32]::GetClientWidth($hwnd)
    $actualHeight = [RemoteCodex.MediaSmoke.Win32]::GetClientHeight($hwnd)
    if ($actualWidth -ne $Width -or $actualHeight -ne $Height) {
        throw "Actual HWND client dimensions are ${actualWidth}x${actualHeight}; expected ${Width}x${Height}."
    }

$measurementSeconds = $Seconds
$encoderSeconds = $Seconds
if ($AgentManifest) { $encoderSeconds += 10 }
    $resourceCallback = $null
    if ($AgentManifest) {
$resourceCallback = { param($encoderProcess)
	$manifestPath = Join-Path $fullDirectory 'process-manifest.json'

	$parsed = ConvertFrom-Json -InputObject (Get-Content -LiteralPath $AgentManifest -Raw -Encoding UTF8)
	$items = @($parsed)
	foreach ($item in $items) { if ($item.Group -ne 'product') { throw 'invalid group' } }
	$encoderEntry = [pscustomobject]@{ Id=[int]$encoderProcess.Id; StartTicks=$encoderProcess.StartTime.ToUniversalTime().Ticks.ToString();Group='product';Label='rc-media-benchmark' }
	$runnerEntry=[pscustomobject]@{Id=[int]$identity.Id;StartTicks=[string]$identity.StartTicks;Group='workload';Label='animated-window'}
	$manifestItems = $items + @($encoderEntry,$runnerEntry)
	Write-ExclusiveJson -Path $manifestPath -Value $manifestItems

	& (Join-Path $repoRoot 'scripts/measure-resources.ps1') -Manifest $manifestPath -Output (Join-Path $fullDirectory 'resources.ndjson') -DurationSeconds $measurementSeconds -IntervalMs 1000 -IncludeGpu | Out-Null
}
    }
    $report = Invoke-NativeEncoder -Exe $encoderExe -SdkRoot $sdkRoot -Hwnd $hwndValue `
        -Width $Width -Height $Height -Fps $Fps -Bitrate $Bitrate -Seconds $encoderSeconds -OnStarted $resourceCallback
}
catch {
    [void]$failures.Add($_.Exception.Message)
}
finally {
    if ($null -ne $runnerProcess -and $null -ne $identity) {
        try {
            $null = Get-ProcessIdentity -Id $identity.Id -StartTicks $identity.StartTicks -Exe $identity.Exe
            & $closeHelper -Identity $identityPath | Out-Null
        }
        catch {
            [void]$failures.Add("Owned runner cleanup failed: $($_.Exception.Message)")
        }
    }
}

$finishedUtc = [DateTime]::UtcNow.ToString('o')
$status = if ($failures.Count -eq 0) { 'MEASURED' } else { 'FAIL' }
$result = [ordered]@{
    status = $status
    started_utc = $startedUtc
    finished_utc = $finishedUtc
    coverage = 'owned-window native capture and encode smoke; WebRTC is not measured'
    parameters = [ordered]@{
        width = $Width
        height = $Height
        fps = $Fps
        bitrate = $Bitrate
        seconds = $Seconds
        encoder_seconds = if ($AgentManifest) { $Seconds + 10 } else { $Seconds }
    }
    executables = [ordered]@{
        runner = $runnerExe
        runner_sha256 = if ($null -ne $runnerHash) { $runnerHash } else { $null }
        encoder = $encoderExe
        encoder_sha256 = if ($null -ne $encoderHash) { $encoderHash } else { $null }
    }
    capture = if ($null -ne $identity) { $identity } else { $null }
    record = $record
    report = $report
    errors = $failures.ToArray()
}

try {
    Write-ExclusiveJson -Path $resultPath -Value $result
}
catch {
    throw "Could not write exclusive smoke result '$resultPath': $($_.Exception.Message)"
}

if ($failures.Count -gt 0) {
    throw ($failures -join ' | ')
}

Write-Output (ConvertTo-Json -InputObject $result -Depth 30 -Compress)
