# PowerShell 5.1, no Pester, no real downloads, processes, or filesystem effects.
param([string]$ScriptPath = (Join-Path $PSScriptRoot '..\..\apps\desktop\src-tauri\src\tailscale-install.ps1'))
$ErrorActionPreference = 'Stop'
$scriptBlock = [scriptblock]::Create((Get-Content -LiteralPath $ScriptPath -Raw))
$passCount = 0

function Fail([string]$Message) { throw $Message }

function Run-Case {
    param([hashtable]$P,[string]$Expected,[int]$ExpectedDownload,[int]$ExpectedLaunch,[string]$Label)
    $ctx = @{
        Exists = if ($P.ContainsKey('exists')) {$P.exists} else {$false}
        ExistsAfterDownload = if ($P.ContainsKey('exists_after_download')) {$P.exists_after_download} else {$false}
        HashOk = if ($P.ContainsKey('hash_ok')) {$P.hash_ok} else {$true}
        SigStatus = if ($P.ContainsKey('sig_status')) {$P.sig_status} else {'Valid'}
        SignerName = if ($P.ContainsKey('signer_name')) {$P.signer_name} else {'Tailscale Inc.'}
        ExitCode = if ($P.ContainsKey('exit_code')) {$P.exit_code} else {0}
        ExitAfterDelay = if ($P.ContainsKey('exit_after_delay')) {$P.exit_after_delay} else {$true}
        Win32Error = if ($P.ContainsKey('win32_error')) {$P.win32_error} else {$null}
        DownloadFailure = if ($P.ContainsKey('download_failure')) {$P.download_failure} else {$false}
        InvalidIndex = if ($P.ContainsKey('invalid_index')) {$P.invalid_index} else {$false}
        InvalidChecksum = if ($P.ContainsKey('invalid_checksum')) {$P.invalid_checksum} else {$false}
        ForeignOnlyIndex = if ($P.ContainsKey('foreign_only_index')) {$P.foreign_only_index} else {$false}
        WrongArchOnlyIndex = if ($P.ContainsKey('wrong_arch_only_index')) {$P.wrong_arch_only_index} else {$false}
    }
    $spy = @{InvokeWebRequest=0;StartProcess=0;NewItem=0;RemoveItem=0;GetFileHash=0;GetAuthenticodeSignature=0;TestPath=0;StartSleep=0;GetDate=0}
    $hashes = @{'amd64'='80eb007e39dfebe17299fa1a09c79a8e1d934f76e0246c0817ebe3af675b7ef6';'arm64'='b7dd1c03bf2e2c430f1fffc4e47ef92829c86d5190febbd9c025dcada5f410b6';'x86'='a8bda9fb254374bb13d46ebf02b6ffba4ed009a739580be511aa7afa8dddd42d'}
    $pa = $env:PROCESSOR_ARCHITEW6432; if ([string]::IsNullOrEmpty($pa)) {$pa=$env:PROCESSOR_ARCHITECTURE}
    $arch = switch ($pa) {'AMD64' {'amd64'} 'ARM64' {'arm64'} 'X86' {'x86'} default {'amd64'}}
    $state = @{DownloadCount=0;DateCalls=0;StartProcessArguments=$null;StartProcessPath=$null;RequestUris=@()}
    $fixedStart = [datetime]'2026-01-01T00:00:00Z'
    $index = @'
<a href="tailscale-setup-1.100.0-amd64.msi">old amd64</a>
<a href="tailscale-setup-1.102.3-amd64.msi">new amd64</a>
<a href="tailscale-setup-1.100.0-arm64.msi">old arm64</a>
<a href="tailscale-setup-1.102.3-arm64.msi">new arm64</a>
<a href="tailscale-setup-1.100.0-x86.msi">old x86</a>
<a href="tailscale-setup-1.102.3-x86.msi">new x86</a>
<a href="tailscale-setup-9.999.0-amd64.msi.exe">bad suffix</a>
<a href="https://evil.example/tailscale-setup-9.999.0-amd64.msi">bad host</a>
<a href="nested/tailscale-setup-9.999.0-amd64.msi">directory</a>
'@

    function Invoke-WebRequest {
        param([string]$Uri,[string]$OutFile,[switch]$UseBasicParsing,[int]$TimeoutSec,[int]$MaximumRedirection)
        $spy.InvokeWebRequest++; $state.DownloadCount++
        $state.RequestUris += $Uri
        if ($ctx.DownloadFailure) {throw 'mock download failure'}
        if ($Uri -eq 'https://pkgs.tailscale.com/stable/') {
            if ($ctx.InvalidIndex) { return [pscustomobject]@{Content='no valid package links'} }
            if ($ctx.ForeignOnlyIndex) { return [pscustomobject]@{Content='<a href="https://evil.example/tailscale-setup-1.102.3-amd64.msi">foreign</a>'} }
            if ($ctx.WrongArchOnlyIndex) {
                $wrong = if ($arch -eq 'amd64') {'arm64'} else {'amd64'}
                return [pscustomobject]@{Content="<a href=`"tailscale-setup-1.102.3-$wrong.msi`">wrong architecture</a>"}
            }
            return [pscustomobject]@{Content=$index}
        }
        if ($Uri -match '\.sha256$') {
            return [pscustomobject]@{Content=if($ctx.InvalidChecksum){"$($hashes[$arch]) tailscale-setup-1.102.3-$arch.msi"}else{$hashes[$arch]}}
        }
    }
    function Start-Process {
        param([string]$FilePath,[object[]]$ArgumentList,[switch]$PassThru,[string]$WindowStyle)
        $spy.StartProcess++
        $state.StartProcessArguments = @($ArgumentList)
        $state.StartProcessPath = $FilePath
        if ($null -ne $ctx.Win32Error) {throw [System.ComponentModel.Win32Exception]::new([int]$ctx.Win32Error)}
        [pscustomobject]@{HasExited=[bool]$ctx.ExitAfterDelay;ExitCode=[int]$ctx.ExitCode;Id=1;StartTime=$fixedStart}
    }
    function New-Item { param([string]$Path,[string]$ItemType,[switch]$Force); $spy.NewItem++ }
    function Remove-Item { param([string]$LiteralPath,[switch]$Force,[string]$ErrorAction); $spy.RemoveItem++ }
    function Get-FileHash { param([string]$LiteralPath,[string]$Path,[string]$Algorithm); $spy.GetFileHash++; [pscustomobject]@{Hash=if($ctx.HashOk){$hashes[$arch]}else{'0'*64}} }
    function Get-AuthenticodeSignature {
        param([string]$LiteralPath,[string]$FilePath)
        $spy.GetAuthenticodeSignature++
        $cert=[pscustomobject]@{Name=$ctx.SignerName}
        $cert | Add-Member -MemberType ScriptMethod -Name GetNameInfo -Value {param($Type,$UseChain);$this.Name} -Force
        [pscustomobject]@{Status=$ctx.SigStatus;SignerCertificate=$cert}
    }
    function Test-Path {
        param([string]$LiteralPath,[string]$Path,[string]$PathType)
        $spy.TestPath++
        $candidate = if ($null -ne $LiteralPath) {$LiteralPath} else {$Path}
        if ($candidate -match '(?i)Tailscale[\\/]tailscale\.exe$') {if($state.DownloadCount -gt 0 -and $ctx.ExistsAfterDownload){return $true};return [bool]$ctx.Exists}
        if ($candidate -match '(?i)(setup\.msi|[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})$') {return $true}
        $false
    }
    function Start-Sleep { param([int]$Milliseconds); $spy.StartSleep++ }
    function Get-Date { param([string]$Date); $spy.GetDate++; $state.DateCalls++; ([datetime]'2026-01-01T00:00:00').AddMinutes(($state.DateCalls-1)*16) }

    $stdout = @(& $scriptBlock 2>&1)
    if ($stdout.Count -ne 1 -or $stdout[0] -isnot [string] -or $stdout[0] -notmatch '^\{.*\}$') {Fail "$Label expected exactly one JSON output"}
    try {$result = $stdout[0] | ConvertFrom-Json} catch {Fail "$Label emitted invalid JSON"}
    $keys = @($result.PSObject.Properties.Name | Sort-Object)
    if (($keys -join ',') -ne 'code,installer_pid,installer_start_ticks,source,version') {Fail "$Label JSON fields mismatch"}
    if ($result.code -ne $Expected) {Fail "$Label code expected '$Expected' got '$($result.code)'"}
    if ($P.ContainsKey('expected_version') -and $result.version -ne $P.expected_version) {Fail "$Label version expected '$($P.expected_version)' got '$($result.version)'"}
    if ($P.ContainsKey('expected_source') -and $result.source -ne $P.expected_source) {Fail "$Label source expected '$($P.expected_source)' got '$($result.source)'"}
    if ($spy.InvokeWebRequest -ne $ExpectedDownload) {Fail "$Label download count mismatch"}
    if ($spy.StartProcess -ne $ExpectedLaunch) {Fail "$Label launch count mismatch"}
    if ($P.ContainsKey('expected_uris') -and (($state.RequestUris -join '|') -ne (@($P.expected_uris) -join '|'))) {Fail "$Label request URI order mismatch"}
    if ($P.no_launch -and $spy.StartProcess -ne 0) {Fail "$Label launched unexpectedly"}
    if ($P.no_cleanup -and $spy.RemoveItem -ne 0) {Fail "$Label cleaned up unexpectedly"}
    if ($P.ContainsKey('expect_update_policy')) {
        $expectedMsiexec = Join-Path $env:SystemRoot 'System32\msiexec.exe'
        if ($state.StartProcessPath -ne $expectedMsiexec) {Fail "$Label msiexec path mismatch"}
        $args = @($state.StartProcessArguments)
        $msiArgument = ([string]$args[1]).Trim('"')
        if ($args.Count -ne 4 -or $args[0] -ne '/i' -or $args[2] -ne 'TS_INSTALLUPDATES="always"' -or $args[3] -ne '/norestart' -or -not [IO.Path]::IsPathRooted($msiArgument)) {Fail "$Label MSI argument contract mismatch"}
    }
    if ($P.expect_timeout_identity) {
        $expectedTicks = $fixedStart.ToFileTimeUtc().ToString()
        if ([string]$result.installer_pid -ne '1' -or [string]$result.installer_start_ticks -ne $expectedTicks) {Fail "$Label timeout identity mismatch"}
    } elseif ($null -ne $result.installer_pid -or $null -ne $result.installer_start_ticks) {Fail "$Label unexpected installer identity"}
    $script:passCount++
}

Run-Case @{exists=$true} 'already_installed' 0 0 'existing'
Run-Case @{download_failure=$true} 'download_failed' 1 0 'downloadfailure'
Run-Case @{invalid_index=$true;no_launch=$true} 'download_failed' 1 0 'invalidindex'
Run-Case @{foreign_only_index=$true;no_launch=$true} 'download_failed' 1 0 'foreignindex'
Run-Case @{wrong_arch_only_index=$true;no_launch=$true} 'download_failed' 1 0 'wrongarchitecture'
Run-Case @{invalid_checksum=$true;no_launch=$true} 'verification_failed' 2 0 'invalidchecksum'
Run-Case @{hash_ok=$false} 'verification_failed' 3 0 'hashfail'
Run-Case @{sig_status='NotSigned'} 'verification_failed' 3 0 'badsignature'
Run-Case @{signer_name='Wrong Publisher'} 'verification_failed' 3 0 'wrongpublisher'
$processor = if ([string]::IsNullOrEmpty($env:PROCESSOR_ARCHITEW6432)) {$env:PROCESSOR_ARCHITECTURE} else {$env:PROCESSOR_ARCHITEW6432}
$expectedArch = switch ($processor) {'AMD64' {'amd64'} 'ARM64' {'arm64'} 'X86' {'x86'} default {'amd64'}}
$expectedSource = "https://pkgs.tailscale.com/stable/tailscale-setup-1.102.3-$expectedArch.msi"
Run-Case @{exit_code=0;expected_version='1.102.3';expected_source=$expectedSource;expected_uris=@('https://pkgs.tailscale.com/stable/',($expectedSource + '.sha256'),$expectedSource);expect_update_policy=$true} 'installed' 3 1 'msi0'
Run-Case @{exit_code=3010} 'reboot_required' 3 1 'msi3010'
Run-Case @{exit_code=1641} 'reboot_required' 3 1 'msi1641'
Run-Case @{exit_code=1602} 'cancelled' 3 1 'msi1602'
Run-Case @{win32_error=1223} 'approval_denied' 3 1 'denial'
Run-Case @{exit_code=1223} 'approval_denied' 3 1 'msi1223'
Run-Case @{exit_code=1618} 'busy' 3 1 'busy'
Run-Case @{exit_code=42} 'install_failed' 3 1 'otherfail'
Run-Case @{exists_after_download=$true;no_launch=$true} 'already_installed' 3 0 'discoveryafterdownload'
Run-Case @{exit_after_delay=$false;no_cleanup=$true;expect_timeout_identity=$true} 'timeout' 3 1 'timeout'
Write-Output "PASS $passCount"
