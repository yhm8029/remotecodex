param(
    [string]$SdkRoot,
    [string]$VsWhere,
    [string]$RuntimeRoot,
    [string]$Dumpbin,
    [switch]$Offline
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Resolve-RequiredPath([string]$Candidate, [bool]$Directory) {
    if ([string]::IsNullOrWhiteSpace($Candidate) -or $Candidate -notmatch '^(?:[A-Za-z]:[\\/]|\\\\)') {
        throw "Path must be absolute"
    }
    $resolved = Resolve-Path -LiteralPath $Candidate -ErrorAction Stop
    $item = Get-Item -LiteralPath $resolved.Path -Force
    if ($Directory -and -not $item.PSIsContainer) { throw "Expected directory" }
    if (-not $Directory -and $item.PSIsContainer) { throw "Expected file" }
    if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw "Reparse points are not accepted" }
    $item.FullName
}
function Ensure-Directory([string]$Path) {
    if (Test-Path -LiteralPath $Path) {
        $item = Get-Item -LiteralPath $Path -Force
        if (-not $item.PSIsContainer) { throw "Expected directory" }
    } else { New-Item -ItemType Directory -Path $Path -Force | Out-Null }
}
function Invoke-Checked([string]$File, [string[]]$Arguments) {
    & $File @Arguments
    if ($LASTEXITCODE -ne 0) { throw "Command failed" }
}
function Write-Utf8NoBom([string]$Path, [string]$Text) {
    [IO.File]::WriteAllText($Path, $Text, (New-Object Text.UTF8Encoding($false)))
}
function Copy-WebTree([string]$Source, [string]$Destination) {
    if (Test-Path -LiteralPath $Destination) {
        $existing = @(Get-ChildItem -LiteralPath $Destination -Recurse -Force)
        if ($existing.Count -gt 0) { throw 'runtime/package/web contains stale generated files; clean it explicitly' }
    } else { New-Item -ItemType Directory -Path $Destination -Force | Out-Null }
    foreach ($item in (Get-ChildItem -LiteralPath $Source -Recurse -Force)) {
        if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw 'Web assets contain a reparse point' }
    }
    foreach ($child in (Get-ChildItem -LiteralPath $Source -Force)) {
        Copy-Item -LiteralPath $child.FullName -Destination $Destination -Recurse -Force
    }
}
function Find-VsWhere([string]$Requested) {
    if ($Requested) { return Resolve-RequiredPath $Requested $false }
    $candidates = @(
        (Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'),
        (Join-Path $env:ProgramFiles 'Microsoft Visual Studio\Installer\vswhere.exe')
    )
    foreach ($candidate in $candidates) { if (Test-Path -LiteralPath $candidate) { return Resolve-RequiredPath $candidate $false } }
    throw 'vswhere.exe was not found'
}
function Find-VsRuntime([string]$Where, [string]$RequestedRuntime, [string]$RequestedDumpbin) {
    if ($RequestedRuntime -and $RequestedDumpbin) {
        return @(
            (Resolve-RequiredPath $RequestedRuntime $true),
            (Resolve-RequiredPath $RequestedDumpbin $false)
        )
    }
    $vswhere = Find-VsWhere $Where
    $install = (& $vswhere -products '*' -latest -property installationPath | Select-Object -First 1).Trim()
    if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($install)) { throw 'Visual Studio installation was not found' }
    $install = Resolve-RequiredPath $install $true
    $msvc = Get-ChildItem -LiteralPath (Join-Path $install 'VC\Tools\MSVC') -Directory | Sort-Object Name -Descending | Select-Object -First 1
    if (-not $msvc) { throw 'MSVC toolchain was not found' }
    $dumpbinPath = Join-Path $msvc.FullName 'bin\Hostx64\x64\dumpbin.exe'
    $crt = Get-ChildItem -LiteralPath (Join-Path $install 'VC\Redist\MSVC') -Directory |
        Where-Object { $_.Name -notmatch 'OneCore' -and (Test-Path -LiteralPath (Join-Path $_.FullName 'x64\Microsoft.VC143.CRT')) } |
        Sort-Object Name -Descending | Select-Object -First 1
    if (-not $crt) { throw 'VC143 CRT runtime was not found' }
    @(
        (Resolve-RequiredPath (Join-Path $crt.FullName 'x64\Microsoft.VC143.CRT') $true),
        (Resolve-RequiredPath $dumpbinPath $false)
    )
}
function Invoke-CargoMetadata([string]$Manifest, [string]$Output, [string]$Features) {
    $arguments = @('metadata','--manifest-path',$Manifest,'--format-version','1','--locked','--filter-platform',$targetTriple)
    if ($Features) { $arguments += @('--features',$Features) }
    $json = @(& cargo.exe @arguments 2>$null)
    if ($LASTEXITCODE -ne 0 -or $json.Count -eq 0) { throw 'cargo metadata failed' }
    Write-Utf8NoBom $Output (($json -join "`n") + "`n")
}
function Copy-OutputInstaller([string]$Target, [string]$Label, [string]$Version, [string]$Artifacts) {
    $nsis = Join-Path $Target 'x86_64-pc-windows-msvc\release\bundle\nsis'
    $source = Get-ChildItem -LiteralPath $nsis -Filter '*.exe' -File | Sort-Object LastWriteTimeUtc -Descending | Select-Object -First 1
    if (-not $source) { throw "No NSIS installer was produced" }
    $destination = Join-Path $Artifacts ("RemoteCodex-{0}-{1}.exe" -f $Version, $Label)
    if (Test-Path -LiteralPath $destination) { throw "Output artifact already exists; clean it explicitly" }
    Copy-Item -LiteralPath $source.FullName -Destination $destination
    $hash = (Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash.ToLowerInvariant()
    [ordered]@{ kind = $Label; path = ('runtime/artifacts/' + [IO.Path]::GetFileName($destination)); bytes = (Get-Item $destination).Length; sha256 = $hash; unsigned = $true }
}

$repo = Resolve-RequiredPath (Split-Path -Parent $PSScriptRoot) $true
$runtime = Join-Path $repo 'runtime'
$package = Join-Path $runtime 'package'
$artifacts = Join-Path $runtime 'artifacts'
Ensure-Directory $runtime
Ensure-Directory $package
Ensure-Directory $artifacts
$rootPackage = Get-Content -Raw (Join-Path $repo 'package.json') | ConvertFrom-Json
$sdk = if ($SdkRoot) { Resolve-RequiredPath $SdkRoot $true } else {
    Resolve-RequiredPath (Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'Programs\gstreamer\1.0\msvc_x86_64') $true
}
$vsParts = Find-VsRuntime $VsWhere $RuntimeRoot $Dumpbin
$vcRuntime = $vsParts[0]
$dumpbinExe = $vsParts[1]
$mediaSdkRoot = Resolve-RequiredPath (Join-Path $sdk 'bin') $true
$node = Resolve-RequiredPath (Get-Command node).Source $false
$tauri = Resolve-RequiredPath (Join-Path $repo 'node_modules\.bin\tauri.cmd') $false
$targetTriple = 'x86_64-pc-windows-msvc'
$envNames = @('PATH','PKG_CONFIG','PKG_CONFIG_PATH','PKG_CONFIG_LIBDIR','RUSTFLAGS','CARGO_TARGET_DIR','GST_PLUGIN_PATH_1_0','GST_PLUGIN_SYSTEM_PATH_1_0','GST_PLUGIN_SCANNER','GSTREAMER_1_0_ROOT_MSVC_X86_64')
$savedEnv = @{}
foreach ($name in $envNames) { $savedEnv[$name] = [Environment]::GetEnvironmentVariable($name, 'Process') }
$oldLocation = Get-Location
$outputs = New-Object 'System.Collections.Generic.List[object]'
try {
    $env:PATH = (Join-Path $sdk 'bin') + ';' + $savedEnv['PATH']
    $env:PKG_CONFIG = Join-Path $sdk 'bin\pkg-config.exe'
    $env:PKG_CONFIG_PATH = Join-Path $sdk 'lib\pkgconfig'
    $env:PKG_CONFIG_LIBDIR = $env:PKG_CONFIG_PATH
    $env:GST_PLUGIN_PATH_1_0 = Join-Path $sdk 'lib\gstreamer-1.0'
    $env:GST_PLUGIN_SYSTEM_PATH_1_0 = ''
    $env:GST_PLUGIN_SCANNER = Join-Path $sdk 'libexec\gstreamer-1.0\gst-plugin-scanner.exe'
    $env:GSTREAMER_1_0_ROOT_MSVC_X86_64 = $sdk
    $env:RUSTFLAGS = '-C target-feature=+crt-static'

    Set-Location $repo
    Invoke-Checked 'npm.cmd' @('run','check:web')
    Invoke-Checked 'npm.cmd' @('run','build:web')
    Copy-WebTree (Join-Path $repo 'apps\web\dist') (Join-Path $package 'web')

    $env:CARGO_TARGET_DIR = Join-Path $runtime 'build-static'
    Invoke-Checked 'cargo.exe' @('build','--release','--locked','-p','rc-agent','--target',$targetTriple)
    Copy-Item -LiteralPath (Join-Path $env:CARGO_TARGET_DIR "$targetTriple\release\rc-agent.exe") -Destination (Join-Path $package 'rc-agent.exe') -Force

    $env:CARGO_TARGET_DIR = Join-Path $runtime 'build-media'
    Invoke-Checked 'cargo.exe' @('build','--release','--locked','-p','rc-media','--features','native-media','--target',$targetTriple)
    $mediaExe = Resolve-RequiredPath (Join-Path $env:CARGO_TARGET_DIR "$targetTriple\release\rc-media.exe") $false
    $stageMedia = Join-Path $repo 'scripts\stage-media-runtime.ps1'
    & $stageMedia -SdkRoot $sdk -RuntimeRoot $vcRuntime -Dumpbin $dumpbinExe -MediaExe $mediaExe
    if ($LASTEXITCODE -ne 0) { throw 'Media runtime staging failed' }

    Invoke-CargoMetadata (Join-Path $repo 'Cargo.toml') (Join-Path $runtime 'cargo-metadata.json') 'rc-media/native-media'
    Invoke-CargoMetadata (Join-Path $repo 'apps\desktop\src-tauri\Cargo.toml') (Join-Path $runtime 'desktop-metadata.json') ''
    Invoke-Checked $node @((Join-Path $repo 'scripts\stage-licenses.mjs'), $sdk)
    Invoke-Checked $node @((Join-Path $repo 'scripts\write-sbom.mjs'), (Join-Path $runtime 'package-inventory.json'), (Join-Path $package 'SBOM.json'))

    $bundleConfig = Resolve-RequiredPath (Join-Path $repo 'apps\desktop\src-tauri\tauri.bundle.conf.json') $false
    $buildConfigs = New-Object 'System.Collections.Generic.List[object]'
    [void]$buildConfigs.Add([ordered]@{ label = 'standard'; path = $bundleConfig })
    if ($Offline) {
        $offlineConfig = Join-Path $artifacts 'tauri.offline.generated.json'
        $overlay = Get-Content -Raw $bundleConfig | ConvertFrom-Json
        $overlay.bundle.windows.webviewInstallMode.type = 'offlineInstaller'
        Write-Utf8NoBom $offlineConfig ($overlay | ConvertTo-Json -Depth 30)
        [void]$buildConfigs.Add([ordered]@{ label = 'offline'; path = $offlineConfig })
    }
    foreach ($buildConfig in $buildConfigs) {
        $target = Join-Path $runtime ('build-desktop-' + $buildConfig.label)
        $env:CARGO_TARGET_DIR = $target
        Push-Location (Join-Path $repo 'apps\desktop\src-tauri')
        Invoke-Checked $tauri @('build','--target',$targetTriple,'--bundles','nsis','--ci','--config',$buildConfig.path)
        Pop-Location
        [void]$outputs.Add((Copy-OutputInstaller $target $buildConfig.label $rootPackage.version $artifacts))
    }
    $manifest = [ordered]@{ name = 'remotecodex'; version = $rootPackage.version; unsigned = $true; outputs = $outputs.ToArray() }
    Write-Utf8NoBom (Join-Path $artifacts 'package-results.json') ($manifest | ConvertTo-Json -Depth 10)
} finally {
    Set-Location $oldLocation
    foreach ($name in $envNames) {
        if ($null -eq $savedEnv[$name]) { Remove-Item -LiteralPath ("Env:" + $name) -ErrorAction SilentlyContinue }
        else { [Environment]::SetEnvironmentVariable($name, $savedEnv[$name], 'Process') }
    }
}
