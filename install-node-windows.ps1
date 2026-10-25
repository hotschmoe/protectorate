param(
    [Parameter(Mandatory = $true)][string]$Version,
    [Parameter(Mandatory = $true)][string]$ManifestSha256,
    [string]$BundleDir = "",
    [string]$Hub = "",
    [string]$Name = "",
    [string]$CaCert = "",
    [string]$Architecture = "",
    [switch]$VerifyOnly,
    [switch]$AllowPlainJobs
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

function Require-RegularFile([string]$Path, [long]$Maximum) {
    $item = Get-Item -LiteralPath $Path -Force -ErrorAction Stop
    if ($item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -or $item.Length -le 0 -or $item.Length -gt $Maximum) {
        throw "Expected a nonempty regular file under $Maximum bytes: $Path"
    }
    return $item
}

function Verified-Hash([string]$Path, [string]$Expected) {
    $actual = (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash
    if ($actual -ine $Expected) { throw "SHA256 mismatch: $Path" }
}

function Require-AbsolutePath([string]$Path, [string]$Label) {
    if ($Path -cnotmatch '^(?:[A-Za-z]:\\|\\\\[^\\]+\\[^\\]+\\)') {
        throw "$Label must be an absolute local or UNC path"
    }
}

function Restrict-Directory([string]$Path) {
    $user = [Security.Principal.WindowsIdentity]::GetCurrent().User
    $system = [Security.Principal.SecurityIdentifier]::new("S-1-5-18")
    $acl = [Security.AccessControl.DirectorySecurity]::new()
    $acl.SetOwner($user)
    $acl.SetAccessRuleProtection($true, $false)
    $inherit = [Security.AccessControl.InheritanceFlags]::ContainerInherit -bor [Security.AccessControl.InheritanceFlags]::ObjectInherit
    foreach ($sid in @($user, $system)) {
        $rule = [Security.AccessControl.FileSystemAccessRule]::new($sid, [Security.AccessControl.FileSystemRights]::FullControl, $inherit, [Security.AccessControl.PropagationFlags]::None, [Security.AccessControl.AccessControlType]::Allow)
        $acl.AddAccessRule($rule)
    }
    Set-Acl -LiteralPath $Path -AclObject $acl
}

function Manifest-Hash([string]$Manifest, [string]$Asset) {
    $text = [IO.File]::ReadAllText($Manifest)
    $found = @()
    foreach ($line in ($text -split "`n")) {
        $match = [regex]::Match($line.TrimEnd("`r"), '^([0-9a-fA-F]{64})  \*?([^\\/]+)$')
        if ($match.Success -and $match.Groups[2].Value -ceq $Asset) {
            $found += $match.Groups[1].Value
        }
    }
    if ($found.Count -ne 1) { throw "Expected exactly one SHA256SUMS entry for $Asset" }
    return $found[0]
}

if ($Version -cnotmatch '^v[A-Za-z0-9][A-Za-z0-9._+-]{0,79}$') { throw "Invalid release version" }
if ($ManifestSha256 -notmatch '^[0-9a-fA-F]{64}$') { throw "ManifestSha256 must be 64 hex digits from a trusted source" }
if (-not $BundleDir -and (-not $Hub -or -not $CaCert)) { throw "Downloads come from the hub: pass Hub and CaCert, or BundleDir" }

$nativeArchitecture = switch ([Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()) {
    "X64" { "amd64" }
    "Arm64" { "arm64" }
    default { throw "Unsupported Windows OS architecture" }
}
if ($Architecture) {
    if (-not $VerifyOnly) { throw "Architecture override is only available with VerifyOnly" }
    if ($Architecture -cnotin @("amd64", "arm64")) { throw "Architecture must be amd64 or arm64" }
} else {
    $Architecture = $nativeArchitecture
}
$assets = @("protectorate-rs-windows-$Architecture.exe", "protectorate-windows-$Architecture.exe", "protectorate-mcpbridge-windows-$Architecture.exe")
$taskName = "Protectorate-Node"
$stateHome = Join-Path $env:LOCALAPPDATA "Protectorate-Node"
$installDir = Join-Path $env:LOCALAPPDATA "Programs\Protectorate-Node"
$stage = Join-Path ([IO.Path]::GetTempPath()) ("protectorate-node-install-" + [Guid]::NewGuid().ToString("N"))
$createdState = $false
$createdInstall = $false
$taskDescription = "Protectorate Rust node managed by install-node-windows.ps1"
$conhost = ""
$taskArguments = ""

if (-not $VerifyOnly) {
    if (-not $Hub -or -not $Name -or -not $CaCert -or -not $env:PROTECTORATE_JOIN_CODE) { throw "Hub, Name, CaCert and PROTECTORATE_JOIN_CODE are required for installation" }
    $hubUri = $null
    if (-not [Uri]::TryCreate($Hub, [UriKind]::Absolute, [ref]$hubUri) -or $hubUri.Scheme -ne "https") {
        throw "Hub must be an HTTPS URL"
    }
    if ($Name -cnotmatch '^[a-z0-9][a-z0-9-]{0,62}$') { throw "Name must be a lowercase DNS label" }
    Require-AbsolutePath $CaCert "CaCert"
    $null = Require-RegularFile $CaCert 61440
    if (Test-Path -LiteralPath $stateHome -PathType Any) { throw "Rust node state already exists: $stateHome" }
    if (Test-Path -LiteralPath $installDir -PathType Any) { throw "Rust node install directory already exists: $installDir" }
    if (Get-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue) { throw "Scheduled task already exists: $taskName" }
}

try {
    New-Item -ItemType Directory -Path $stage -ErrorAction Stop | Out-Null
    if ($BundleDir) {
        Require-AbsolutePath $BundleDir "BundleDir"
        $manifest = Join-Path $BundleDir "SHA256SUMS"
        $versionFile = Join-Path $BundleDir "VERSION"
        $null = Require-RegularFile $manifest 1048576
        $null = Require-RegularFile $versionFile 1024
        if ([IO.File]::ReadAllText($versionFile).Trim() -cne $Version) { throw "Bundle VERSION does not match $Version" }
        Copy-Item -LiteralPath $manifest -Destination (Join-Path $stage "SHA256SUMS")
        foreach ($asset in $assets) { Copy-Item -LiteralPath (Join-Path $BundleDir $asset) -Destination (Join-Path $stage $asset) }
    } else {
        # Every download is pinned to the hub CA, like the invite line.
        $hubCa = [Security.Cryptography.X509Certificates.X509Certificate2]::new($CaCert)
        [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
        [Net.ServicePointManager]::ServerCertificateValidationCallback = {
            param($s, $c, $x, $e)
            $k = [Security.Cryptography.X509Certificates.X509Chain]::new()
            $k.ChainPolicy.RevocationMode = "NoCheck"
            $k.ChainPolicy.VerificationFlags = "AllowUnknownCertificateAuthority"
            [void]$k.ChainPolicy.ExtraStore.Add($hubCa)
            (($e -band 3) -eq 0) -and $k.Build([Security.Cryptography.X509Certificates.X509Certificate2]$c) -and ($k.ChainElements[$k.ChainElements.Count - 1].Certificate.Thumbprint -eq $hubCa.Thumbprint)
        }.GetNewClosure()
        $base = $Hub.TrimEnd("/") + "/dl"
        Invoke-WebRequest -Uri "$base/SHA256SUMS" -OutFile (Join-Path $stage "SHA256SUMS") -MaximumRedirection 0 -UseBasicParsing
        foreach ($asset in $assets) {
            Invoke-WebRequest -Uri "$base/$asset" -OutFile (Join-Path $stage $asset) -MaximumRedirection 0 -UseBasicParsing
        }
    }
    $manifest = Join-Path $stage "SHA256SUMS"
    $null = Require-RegularFile $manifest 1048576
    Verified-Hash $manifest $ManifestSha256
    foreach ($asset in $assets) {
        $path = Join-Path $stage $asset
        $null = Require-RegularFile $path 536870912
        Verified-Hash $path (Manifest-Hash $manifest $asset)
    }
    if ($VerifyOnly) {
        Write-Output "Verified Windows Rust backend, CLI, and MCP bridge assets for $Version"
        return
    }

    $server = Join-Path $stage $assets[0]
    $reported = & $server version --json | ConvertFrom-Json
    if ($LASTEXITCODE -ne 0 -or $reported.version -cne $Version) { throw "Server binary version does not match $Version" }

    New-Item -ItemType Directory -Path $stateHome -ErrorAction Stop | Out-Null
    $createdState = $true
    Restrict-Directory $stateHome
    New-Item -ItemType Directory -Path $installDir -ErrorAction Stop | Out-Null
    $createdInstall = $true
    Restrict-Directory $installDir
    Copy-Item -LiteralPath $server -Destination (Join-Path $installDir "protectorate-rs.exe")
    Copy-Item -LiteralPath $server -Destination (Join-Path $installDir "protectorate-rs-guardian.exe")
    Copy-Item -LiteralPath (Join-Path $stage $assets[1]) -Destination (Join-Path $installDir "protectorate.exe")
    Copy-Item -LiteralPath (Join-Path $stage $assets[2]) -Destination (Join-Path $installDir "protectorate-mcpbridge.exe")

    $installedServer = Join-Path $installDir "protectorate-rs.exe"
    $installedGuardian = Join-Path $installDir "protectorate-rs-guardian.exe"
    # The join code reaches the node binary through PROTECTORATE_JOIN_CODE.
    $join = @("node", "join", "--hub", $Hub, "--name", $Name, "--ca-cert", $CaCert, "--home", $stateHome)
    & $installedServer @join
    if ($LASTEXITCODE -ne 0) { throw "Joining the hub failed" }

    $user = [Security.Principal.WindowsIdentity]::GetCurrent().Name
    $conhost = Join-Path $env:SystemRoot "System32\conhost.exe"
    $taskArguments = '--headless "' + $installedGuardian + '" node serve --home "' + $stateHome + '"'
    if ($AllowPlainJobs) { $taskArguments += ' --allow-plain-jobs' }
    $action = New-ScheduledTaskAction -Execute $conhost -Argument $taskArguments
    $trigger = New-ScheduledTaskTrigger -AtLogOn -User $user
    $principal = New-ScheduledTaskPrincipal -UserId $user -LogonType Interactive -RunLevel Limited
    $settings = New-ScheduledTaskSettingsSet -ExecutionTimeLimit ([TimeSpan]::Zero) -RestartCount 3 -RestartInterval (New-TimeSpan -Minutes 1) -StartWhenAvailable
    Register-ScheduledTask -TaskName $taskName -Action $action -Trigger $trigger -Principal $principal -Settings $settings -Description $taskDescription -ErrorAction Stop | Out-Null
    Start-ScheduledTask -TaskName $taskName -ErrorAction Stop
    $running = $false
    for ($attempt = 0; $attempt -lt 10; $attempt++) {
        if ((Get-ScheduledTask -TaskName $taskName).State -eq "Running") { $running = $true; break }
        Start-Sleep -Seconds 1
    }
    if (-not $running) { throw "Node task did not remain running" }
    Write-Output "Installed $Version as $Name. Task: $taskName; state: $stateHome"
} catch {
    $installError = $_
    $safeToDelete = $true
    if ($createdState -or $createdInstall) {
        try {
            $task = Get-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue
            if ($task) {
                $managed = $task.Description -ceq $taskDescription -and
                    $task.Actions.Count -eq 1 -and
                    $task.Actions[0].Execute -ieq $conhost -and
                    $task.Actions[0].Arguments -ceq $taskArguments
                if (-not $managed) {
                    $safeToDelete = $false
                    Write-Warning "A task with $taskName exists but does not match this installer; preserving files and state"
                } else {
                    # The task action is conhost.exe --headless; stopping only
                    # that wrapper can leave its node child running. The Rust
                    # companion verifies task ownership and stops the child.
                    $installedServer = Join-Path $installDir "protectorate-rs.exe"
                    $null = Require-RegularFile $installedServer 536870912
                    & $installedServer node uninstall --home $stateHome
                    if ($LASTEXITCODE -ne 0) { throw "The failed node task or process did not stop" }
                }
            }
        } catch {
            $safeToDelete = $false
            Write-Warning "Could not prove $taskName stopped and was removed; preserving files and state: $_"
        }
    }
    if ($safeToDelete) {
        if ($createdInstall) { Remove-Item -LiteralPath $installDir -Recurse -Force -ErrorAction SilentlyContinue }
        if ($createdState) { Remove-Item -LiteralPath $stateHome -Recurse -Force -ErrorAction SilentlyContinue }
    }
    throw $installError
} finally {
    if (Test-Path -LiteralPath $stage) { Remove-Item -LiteralPath $stage -Recurse -Force -ErrorAction SilentlyContinue }
}
