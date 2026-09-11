param(
    [Parameter(Mandatory)]
    [string]$Exe,

    [Parameter(Mandatory)]
    [string]$Profile,

    [Parameter(Mandatory)]
    [string]$Output,

    [Parameter(Mandatory)]
    [ValidateRange(1024,65535)]
    [int]$Port
)

$ErrorActionPreference = 'Stop'

if (-not (Test-Path -LiteralPath $Exe -PathType Leaf)) {
    throw "Exe not found: $Exe"
}

$exeItem = Get-Item -LiteralPath $Exe
$baseName = $exeItem.Name
if ($baseName -ne 'remotecodex-desktop.exe') {
    throw "Exe basename must be 'remotecodex-desktop.exe', got '$baseName.exe'"
}

$profileFull = (Resolve-Path -LiteralPath $Profile).ProviderPath
if (-not (Test-Path -LiteralPath $profileFull -PathType Container)) {
    throw "Profile directory not found: $profileFull"
}

if (Test-Path -LiteralPath $Output) {
    throw "Output already exists: $Output"
}

$occupied = @(Get-NetTCPConnection -LocalPort $Port -State Listen -ErrorAction SilentlyContinue)
if ($occupied.Count -gt 0) {
    throw "Port $Port is already occupied"
}

$env:WEBVIEW2_USER_DATA_FOLDER = $profileFull
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=$Port"

try {
    $proc = Start-Process -FilePath $exeItem.FullName -PassThru -WindowStyle Hidden -WorkingDirectory $exeItem.DirectoryName
}
finally {
    Remove-Item Env:WEBVIEW2_USER_DATA_FOLDER -ErrorAction SilentlyContinue
    Remove-Item Env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS -ErrorAction SilentlyContinue
}

$obj = [ordered]@{
    Id         = [int]$proc.Id
    StartTicks = [string]$proc.StartTime.ToUniversalTime().Ticks
    Exe        = $exeItem.FullName
    Profile    = $profileFull
    Port       = $Port
}

$json = $obj | ConvertTo-Json -Compress

$utf8NoBom = New-Object System.Text.UTF8Encoding($false)
$fs = [System.IO.File]::Open($Output, [System.IO.FileMode]::CreateNew, [System.IO.FileAccess]::Write, [System.IO.FileShare]::None)
try {
    $bytes = $utf8NoBom.GetBytes($json)
    $fs.Write($bytes, 0, $bytes.Length)
    $fs.Flush()
}
finally {
    $fs.Dispose()
}

# Do not wait, do not kill.
