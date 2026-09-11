param(
    [Parameter(Mandatory)][string]$SdkRoot,
    [Parameter(Mandatory)][string]$RuntimeRoot,
    [Parameter(Mandatory)][string]$Dumpbin,
    [Parameter(Mandatory)][string[]]$Roots
)
$ErrorActionPreference = 'Stop'
function Resolve-Absolute([string]$Path, [ValidateSet('Container','Leaf')] [string]$Type) {
    if ($Path -notmatch '^(?:[A-Za-z]:[\\/]|\\\\)' -or -not (Test-Path -LiteralPath $Path -PathType $Type)) { throw "Invalid path: $Path" }
    (Resolve-Path -LiteralPath $Path).Path
}
$sdk = Resolve-Absolute $SdkRoot Container
$runtime = Resolve-Absolute $RuntimeRoot Container
$dump = Resolve-Absolute $Dumpbin Leaf
$rootsAbs = @($Roots | ForEach-Object { Resolve-Absolute $_ Leaf })
$bin = Resolve-Absolute (Join-Path $sdk 'bin') Container
$dlls = [Collections.Generic.Dictionary[string,string]]::new([StringComparer]::OrdinalIgnoreCase)
foreach ($dir in @($bin, $runtime)) {
    foreach ($file in Get-ChildItem -LiteralPath $dir -Filter *.dll -File) {
        if ($dlls.ContainsKey($file.Name)) { throw "Duplicate DLL basename: $($file.Name)" }
        $dlls[$file.Name] = $file.FullName
    }
}
$queue = [Collections.Generic.Queue[string]]::new(); $rootsAbs | ForEach-Object { $queue.Enqueue($_) }
$seen = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
$os = '^(api-ms-win-|ext-ms-win-).+[.]dll$'; $crt = '^(vcruntime|msvcp|concrt).+[.]dll$'
while ($queue.Count) {
    $path = $queue.Dequeue()
    if (-not $seen.Add($path)) { continue }
    if ($seen.Count -gt 512) { throw 'Dependency closure exceeds 512 files' }
    $lines = & $dump /NOLOGO /DEPENDENTS $path 2>&1
    if ($LASTEXITCODE -ne 0) { throw "dumpbin failed: $path" }
    foreach ($line in ($lines | Out-String).Split("`n")) {
        $name = $line.Trim()
        if ($name -notmatch '^[A-Za-z0-9_.-]+[.]dll$') { continue }
        if ($dlls.ContainsKey($name)) { $queue.Enqueue($dlls[$name]); continue }
        if ($name -match $os) { continue }
        if ($name -match $crt) { throw "Missing CRT dependency: $name" }
        if (-not (Test-Path -LiteralPath (Join-Path (Join-Path $env:WINDIR 'System32') $name) -PathType Leaf)) { throw "Unresolved dependency: $name" }
    }
}
$seen | Sort-Object
