$ErrorActionPreference = 'Stop'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
$packageRoot = 'https://pkgs.tailscale.com/stable/'
$publisher = 'Tailscale Inc.'
$source = $null
$code = $null
$arch = $null
$msiPath = $null
$workDir = $null
$proc = $null
$installer_pid = $null
$installer_start_ticks = $null
function Select-StableMsi {
    param([string]$Html,[string]$Architecture)
    $selected = $null
    foreach ($match in [regex]::Matches($Html, 'href\s*=\s*["'']([^"'']+)["'']', [Text.RegularExpressions.RegexOptions]::IgnoreCase)) {
        $name = $match.Groups[1].Value
        $parts = [regex]::Match($name, '^tailscale-setup-(\d+)\.(\d+)\.(\d+)-(amd64|arm64|x86)\.msi$')
        if (-not $parts.Success -or $parts.Groups[4].Value -ne $Architecture) { continue }
        $candidate = [pscustomobject]@{
            Name = $name
            Version = [version]("{0}.{1}.{2}" -f $parts.Groups[1].Value,$parts.Groups[2].Value,$parts.Groups[3].Value)
        }
        if ($null -eq $selected -or $candidate.Version -gt $selected.Version) { $selected = $candidate }
    }
    return $selected
}
function Test-Installed {
    $paths = @()
    foreach ($e in 'ProgramW6432','ProgramFiles','ProgramFiles(x86)') {
        $v = [Environment]::GetEnvironmentVariable($e)
        if ($v) { $paths += (Join-Path $v 'Tailscale\tailscale.exe') }
    }
    $candidates = @()
    $pe = [Environment]::GetEnvironmentVariable('PATH','Machine')
    if ($pe) { $pe.Split(';') | ForEach-Object { if ($_ -and [System.IO.Path]::IsPathRooted($_)) { $candidates += $_ } } }
    $pu = [Environment]::GetEnvironmentVariable('PATH','User')
    if ($pu) { $pu.Split(';') | ForEach-Object { if ($_ -and [System.IO.Path]::IsPathRooted($_)) { $candidates += $_ } } }
    foreach ($p in $candidates) {
        try {
            $leaf = [System.IO.Path]::Combine($p, 'tailscale.exe')
            if (Test-Path -LiteralPath $leaf -PathType Leaf) { $paths += $leaf }
        } catch {}
    }
    foreach ($p in $paths) {
        try {
            if (Test-Path -LiteralPath $p -PathType Leaf) { return $true }
        } catch {}
    }
    return $false
}
try {
    $pa = $env:PROCESSOR_ARCHITEW6432
    if ([string]::IsNullOrEmpty($pa)) { $pa = $env:PROCESSOR_ARCHITECTURE }
    switch ($pa) {
        'AMD64' { $arch = 'amd64' }
        'ARM64' { $arch = 'arm64' }
        'X86'   { $arch = 'x86' }
        default { $code = 'unsupported_platform'; return }
    }
    if (Test-Installed) { $code = 'already_installed'; return }
    try {
        $index = (Invoke-WebRequest -Uri $packageRoot -UseBasicParsing -TimeoutSec 120 -MaximumRedirection 0).Content
    } catch {
        $code = 'download_failed'; return
    }
    $selected = Select-StableMsi -Html $index -Architecture $arch
    if ($null -eq $selected) { $code = 'download_failed'; return }
    $version = $selected.Version.ToString()
    $source = $packageRoot + $selected.Name
    try {
        $expected = (Invoke-WebRequest -Uri ($source + '.sha256') -UseBasicParsing -TimeoutSec 120 -MaximumRedirection 0).Content.Trim()
    } catch {
        $code = 'download_failed'; return
    }
    if ($expected -notmatch '^[a-fA-F0-9]{64}$') { $code = 'verification_failed'; return }
    $guid = [Guid]::NewGuid().ToString()
    $workDir = Join-Path ([System.IO.Path]::GetTempPath()) $guid
    $null = New-Item -ItemType Directory -Path $workDir -Force
    $msiPath = Join-Path $workDir 'setup.msi'
    try {
        Invoke-WebRequest -Uri $source -OutFile $msiPath -UseBasicParsing -TimeoutSec 120 -MaximumRedirection 0
    } catch {
        $code = 'download_failed'; return
    }
    $actual = (Get-FileHash -Path $msiPath -Algorithm SHA256).Hash.ToLower()
    if ($actual -ne $expected.ToLower()) { $code = 'verification_failed'; return }
    $sig = Get-AuthenticodeSignature -FilePath $msiPath
    if ($sig.Status -ne 'Valid') { $code = 'verification_failed'; return }
    $sn = $sig.SignerCertificate.GetNameInfo([System.Security.Cryptography.X509Certificates.X509NameType]::SimpleName, $false)
    if ($sn -ne $publisher) { $code = 'verification_failed'; return }
    if (Test-Installed) { $code = 'already_installed'; return }
    $msiExec = Join-Path $env:SystemRoot 'System32\msiexec.exe'
    try {
        $proc = Start-Process -FilePath $msiExec -ArgumentList @('/i', "`"$msiPath`"", 'TS_INSTALLUPDATES="always"', '/norestart') -PassThru -WindowStyle Normal
    } catch [System.ComponentModel.Win32Exception] {
        if ($_.Exception.NativeErrorCode -eq 1223) { $code = 'approval_denied' } else { $code = 'install_failed' }
        return
    } catch {
        $code = 'install_failed'; return
    }
    $deadline = (Get-Date).AddMinutes(15)
    while (-not $proc.HasExited) {
        Start-Sleep -Milliseconds 500
        if ((Get-Date) -gt $deadline) { $installer_pid = [uint32]$proc.Id; $installer_start_ticks = $proc.StartTime.ToFileTimeUtc().ToString(); $code = 'timeout'; return }
    }
    switch ($proc.ExitCode) {
        0    { $code = 'installed' }
        3010 { $code = 'reboot_required' }
        1641 { $code = 'reboot_required' }
        1602 { $code = 'cancelled' }
        1223 { $code = 'approval_denied' }
        1618 { $code = 'busy' }
        default { $code = 'install_failed' }
    }
}
catch {
    if (-not $code) { $code = 'install_failed' }
}
finally {
    if (-not $code) { $code = 'install_failed' }
    if (($proc -eq $null) -or $proc.HasExited) {
        if ($msiPath -and (Test-Path -LiteralPath $msiPath)) {
            try { Remove-Item -LiteralPath $msiPath -Force -ErrorAction SilentlyContinue } catch {}
        }
        if ($workDir -and (Test-Path -LiteralPath $workDir)) {
            try { Remove-Item -LiteralPath $workDir -Force -ErrorAction SilentlyContinue } catch {}
        }
    }
    Write-Output ((@{code=$code;version=$version;source=$source;installer_pid=$installer_pid;installer_start_ticks=$installer_start_ticks}) | ConvertTo-Json -Compress)
}
