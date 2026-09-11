param(
    [Parameter(Mandatory = $true)][string]$SdkRoot,
    [Parameter(Mandatory = $true)][string]$RuntimeRoot,
    [Parameter(Mandatory = $true)][string]$Dumpbin,
    [Parameter(Mandatory = $true)][string]$MediaExe
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Resolve-RequiredPath([string]$Candidate, [bool]$Directory) {
    if ($Candidate -notmatch '^(?:[A-Za-z]:[\\/]|\\\\)') {
        throw "Path must be absolute: $Candidate"
    }
    $resolved = Resolve-Path -LiteralPath $Candidate -ErrorAction Stop
    $item = Get-Item -LiteralPath $resolved.Path -Force
    if ($Directory -and -not $item.PSIsContainer) { throw "Expected directory: $Candidate" }
    if (-not $Directory -and $item.PSIsContainer) { throw "Expected file: $Candidate" }
    $item.FullName
}

$repoRoot = Resolve-RequiredPath -Candidate (Split-Path -Parent $PSScriptRoot) -Directory $true
$sdk = Resolve-RequiredPath -Candidate $SdkRoot -Directory $true
$runtime = Resolve-RequiredPath -Candidate $RuntimeRoot -Directory $true
$dumpbinExe = Resolve-RequiredPath -Candidate $Dumpbin -Directory $false
$mediaExePath = Resolve-RequiredPath -Candidate $MediaExe -Directory $false
$destinationRoot = Join-Path $repoRoot 'runtime\package\media-runtime'
$binDestination = Join-Path $destinationRoot 'bin'
$pluginDestination = Join-Path $destinationRoot 'lib\gstreamer-1.0'
$scannerDestination = Join-Path $destinationRoot 'libexec\gstreamer-1.0'

$pluginNames = @(
    'gstd3d11.dll', 'gstmediafoundation.dll', 'gstopenh264.dll',
    'gstvideoconvertscale.dll', 'gstapp.dll', 'gstvideoparsersbad.dll',
    'gstrtp.dll', 'gstwebrtc.dll', 'gstnice.dll', 'gstcoreelements.dll',
    'gstrtpmanager.dll', 'gstdtls.dll', 'gstsrtp.dll', 'gstsctp.dll', 'gstudp.dll'
)
$pluginSources = New-Object 'System.Collections.Generic.List[string]'
foreach ($name in $pluginNames) {
    $source = Resolve-RequiredPath -Candidate (Join-Path $sdk "lib\gstreamer-1.0\$name") -Directory $false
    if ([System.IO.Path]::GetExtension($source) -ine '.dll') { throw "Plugin source is not a DLL: $source" }
    [void]$pluginSources.Add($source)
}
$scanner = Resolve-RequiredPath -Candidate (Join-Path $sdk 'libexec\gstreamer-1.0\gst-plugin-scanner.exe') -Directory $false
if ([System.IO.Path]::GetExtension($scanner) -ine '.exe') { throw "Scanner is not an executable: $scanner" }

$closureScript = Resolve-RequiredPath -Candidate (Join-Path $PSScriptRoot 'native-runtime-closure.ps1') -Directory $false
$roots = @($mediaExePath, $scanner) + $pluginSources.ToArray()
$closure = @(& $closureScript -SdkRoot $sdk -RuntimeRoot $runtime -Dumpbin $dumpbinExe -Roots $roots)
if ($closure.Count -eq 0) { throw 'Dependency closure was empty' }

$allowedExecutables = New-Object 'System.Collections.Generic.HashSet[string]'([System.StringComparer]::OrdinalIgnoreCase)
[void]$allowedExecutables.Add([System.IO.Path]::GetFileName($mediaExePath))
[void]$allowedExecutables.Add('gst-plugin-scanner.exe')
$pluginSourceSet = New-Object 'System.Collections.Generic.HashSet[string]'([System.StringComparer]::OrdinalIgnoreCase)
foreach ($source in $pluginSources) { [void]$pluginSourceSet.Add($source) }
$dependencySources = New-Object 'System.Collections.Generic.List[string]'
foreach ($entry in $closure) {
    $source = Resolve-RequiredPath -Candidate ([string]$entry) -Directory $false
    $extension = [System.IO.Path]::GetExtension($source)
    if ($extension -ieq '.exe') {
        if (-not $allowedExecutables.Contains([System.IO.Path]::GetFileName($source))) {
            throw "Unexpected executable in closure: $source"
        }
    } elseif ($extension -ieq '.dll') {
        if (-not $pluginSourceSet.Contains($source)) { [void]$dependencySources.Add($source) }
    } else {
        throw "Unsupported closure file: $source"
    }
}

$directories = @($binDestination, $pluginDestination, $scannerDestination)
foreach ($directory in $directories) {
    if (Test-Path -LiteralPath $directory) {
        if (-not (Get-Item -LiteralPath $directory).PSIsContainer) { throw "Destination is not a directory: $directory" }
    }
}
$expectedFiles = New-Object 'System.Collections.Generic.HashSet[string]'([System.StringComparer]::OrdinalIgnoreCase)
foreach ($source in $pluginSources) { [void]$expectedFiles.Add((Join-Path $pluginDestination ([System.IO.Path]::GetFileName($source)))) }
[void]$expectedFiles.Add((Join-Path $scannerDestination 'gst-plugin-scanner.exe'))
[void]$expectedFiles.Add((Join-Path $binDestination 'rc-media.exe'))
foreach ($source in $dependencySources) { [void]$expectedFiles.Add((Join-Path $binDestination ([System.IO.Path]::GetFileName($source)))) }

$actualFiles = New-Object 'System.Collections.Generic.HashSet[string]'([System.StringComparer]::OrdinalIgnoreCase)
if (Test-Path -LiteralPath $destinationRoot) {
    foreach ($file in (Get-ChildItem -LiteralPath $destinationRoot -Recurse -File -Force)) { [void]$actualFiles.Add($file.FullName) }
}
foreach ($file in $actualFiles) {
    if (-not $expectedFiles.Contains($file)) { throw "Unexpected existing staged file: $file" }
}

foreach ($directory in $directories) {
    if (-not (Test-Path -LiteralPath $directory)) { New-Item -ItemType Directory -Path $directory -Force | Out-Null }
}
foreach ($source in $pluginSources) { Copy-Item -LiteralPath $source -Destination (Join-Path $pluginDestination ([System.IO.Path]::GetFileName($source))) -Force }
Copy-Item -LiteralPath $scanner -Destination (Join-Path $scannerDestination 'gst-plugin-scanner.exe') -Force
Copy-Item -LiteralPath $mediaExePath -Destination (Join-Path $binDestination 'rc-media.exe') -Force
foreach ($source in $dependencySources) { Copy-Item -LiteralPath $source -Destination (Join-Path $binDestination ([System.IO.Path]::GetFileName($source))) -Force }

$fileCount = 0
$totalBytes = [int64]0
foreach ($file in (Get-ChildItem -LiteralPath $destinationRoot -Recurse -File -Force)) {
    if (-not $expectedFiles.Contains($file.FullName)) { throw "Unexpected staged file: $($file.FullName)" }
    $fileCount++
    $totalBytes += $file.Length
}
if ($fileCount -ne $expectedFiles.Count) { throw 'Staged file set does not match the expected closure' }
([ordered]@{ files = $fileCount; bytes = $totalBytes } | ConvertTo-Json -Compress)
