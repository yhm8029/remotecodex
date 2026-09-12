$ErrorActionPreference = 'Stop'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
$version = '1.102.4'
$source = $null
$code = $null
$arch = $null
$msiPath = $null
$workDir = $null
$proc = $null
$installer_pid = $null
$installer_start_ticks = $null
$hashes = @{
    'amd64' = '80eb007e39dfebe17299fa1a09c79a8e1d934f76e0246c0817ebe3af675b7ef6'
    'arm64' = 'b7dd1c03bf2e2c430f1fffc4e47ef92829c86d5190febbd9c025dcada5f410b6'
    'x86'   = 'a8bda9fb254374bb13d46ebf02b6ffba4ed009a739580be511aa7afa8dddd42d'
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
    $source = "https://pkgs.tailscale.com/stable/tailscale-setup-$version-$arch.msi"
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
    if ($actual -ne $hashes[$arch].ToLower()) { $code = 'verification_failed'; return }
    $sig = Get-AuthenticodeSignature -FilePath $msiPath
    if ($sig.Status -ne 'Valid') { $code = 'verification_failed'; return }
    $sn = $sig.SignerCertificate.GetNameInfo([System.Security.Cryptography.X509Certificates.X509NameType]::SimpleName, $false)
    if ($sn -ne 'Tailscale Inc.') { $code = 'verification_failed'; return }
    if (Test-Installed) { $code = 'already_installed'; return }
    $msiExec = Join-Path $env:SystemRoot 'System32\msiexec.exe'
    try {
        $proc = Start-Process -FilePath $msiExec -ArgumentList @('/i', "`"$msiPath`"", '/norestart') -PassThru -WindowStyle Normal
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
