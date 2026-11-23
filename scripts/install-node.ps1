# Installs a Protectorate node on Windows from a GitHub release. It downloads
# protectorate-node.exe for this machine, checks it against SHA256SUMS, stages it in
# <home>\releases\<version>\, writes current.txt (and updates releases\current if symlinks are
# available), runs `protectorate-node join <ptjoin1:...> --home <home>`, waits for hub
# approval via `protectorate-node authorize`, and installs the Windows service.
#
# Fetch this script at a commit and compare its sha256 before running from an elevated PowerShell:
#   curl.exe -q -fsSLO https://raw.githubusercontent.com/hotschmoe/protectorate/<commit>/scripts/install-node.ps1
#   Get-FileHash -Algorithm SHA256 .\install-node.ps1
#   powershell -ExecutionPolicy Bypass -File .\install-node.ps1 -JoinToken ptjoin1:... -DryRun
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$JoinToken,
    [string]$Name = '',
    [string]$Version = '',
    [ValidateSet('stable', 'dev')][string]$Channel = 'stable',
    [string]$From = '',
    [string]$Repo = 'hotschmoe/protectorate',
    [string]$StateDir = (Join-Path $env:ProgramData 'protectorate-node'),
    [string]$ServiceName = 'ProtectorateNode',
    [int]$WaitSecs = 60,
    [switch]$DryRun
)

$ErrorActionPreference = 'Stop'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
$ProgressPreference = 'SilentlyContinue'

function Die([string]$msg) { [Console]::Error.WriteLine("install-node: $msg"); exit 1 }
function Say([string]$msg) { Write-Output $msg }

function TagKey([string]$tag) {
    if ($tag -notmatch '^v(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z.-]+))?$') { return $null }
    $key = '{0:D10}.{1:D10}.{2:D10}' -f [long]$Matches[1], [long]$Matches[2], [long]$Matches[3]
    if (-not $Matches[4]) { return "$key~" }
    $ids = $Matches[4].Split('.') | ForEach-Object { if ($_ -match '^\d+$') { '0' + $_.PadLeft(20, '0') } else { '1' + $_ } }
    return "$key-" + ($ids -join '.')
}

function ResolveTag([string]$channel) {
    $list = Invoke-RestMethod -UseBasicParsing -Headers @{ Accept = 'application/vnd.github+json' } `
        -Uri "https://api.github.com/repos/$Repo/releases?per_page=100"
    $best = $null; $bestKey = $null
    foreach ($r in $list) {
        if ($r.draft) { continue }
        $tag = [string]$r.tag_name
        $k = TagKey $tag
        if ($null -eq $k) { continue }
        if ($channel -eq 'stable' -and $tag.Contains('-')) { continue }
        if ($null -eq $bestKey -or [string]::CompareOrdinal($k, $bestKey) -gt 0) { $best = $tag; $bestKey = $k }
    }
    return $best
}

function Bytes([string]$name) {
    if ($From) {
        $p = Join-Path $From $name
        if (-not (Test-Path -LiteralPath $p -PathType Leaf)) { Die "$From has no $name" }
        return , [IO.File]::ReadAllBytes($p)
    }
    try { $r = Invoke-WebRequest -UseBasicParsing -Uri "$($script:Base)/$name" }
    catch { Die "downloading $name failed: $($_.Exception.Message)" }
    return , $r.RawContentStream.ToArray()
}

function Sha([byte[]]$bytes) {
    $h = [Security.Cryptography.SHA256]::Create()
    return (($h.ComputeHash($bytes) | ForEach-Object { $_.ToString('x2') }) -join '')
}

function Want([string]$name) {
    $hits = @($script:Sums | Where-Object { $_ -match '^([0-9a-f]{64}) [ *](.+)$' -and $Matches[2] -eq $name } |
        ForEach-Object { $_.Substring(0, 64) })
    if ($hits.Count -ne 1) { Die "SHA256SUMS of $script:Tag has no single valid entry for $name" }
    return $hits[0]
}

function CheckFile([string]$name, [string]$path) {
    $got = (Get-FileHash -Algorithm SHA256 -LiteralPath $path).Hash.ToLowerInvariant()
    if ($got -ne (Want $name)) { Die "$name does not match SHA256SUMS (got $got)" }
}

if ($JoinToken -notmatch '^ptjoin1:[A-Za-z0-9_-]+$') { Die 'JoinToken must look like ptjoin1:...' }
if ($Repo -notmatch '^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$') { Die 'Repo must be OWNER/NAME' }
if ($ServiceName -notmatch '^[A-Za-z0-9._-]+$') { Die 'ServiceName contains unsupported characters' }
$cpu = if ($env:PROCESSOR_ARCHITEW6432) { $env:PROCESSOR_ARCHITEW6432 } else { $env:PROCESSOR_ARCHITECTURE }
$arch = switch ($cpu) { 'AMD64' { 'amd64' } 'ARM64' { 'arm64' } default { Die "no node build for $cpu" } }
$admin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole(
    [Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $admin -and -not $DryRun) { Die 'run from an elevated PowerShell' }

if ($From) {
    if (-not (Test-Path -LiteralPath $From -PathType Container)) { Die "-From $From is not a directory" }
    $script:Base = $From
} else {
    $script:Tag = if ($Version) { $Version } else { ResolveTag $Channel }
    if (-not $script:Tag) { Die "$Repo has no $Channel release" }
    $script:Base = "https://github.com/$Repo/releases/download/$($script:Tag)"
}
$sumsBytes = Bytes 'SHA256SUMS'
$script:Sums = [Text.Encoding]::UTF8.GetString($sumsBytes) -split "`r?`n" | Where-Object { $_ }
$versionBytes = Bytes 'VERSION'
$said = [Text.Encoding]::UTF8.GetString($versionBytes).Trim()
if (-not $script:Tag) { $script:Tag = $said }
$Tag = $script:Tag
if ((Sha $versionBytes) -ne (Want 'VERSION')) { Die 'VERSION does not match SHA256SUMS' }
if ($said -ne $Tag) { Die "the release's VERSION says $said, not $Tag" }
if ($Tag -notmatch '^[A-Za-z0-9][A-Za-z0-9._+-]*$' -or $Tag -in 'current', 'previous') { Die "bad version $Tag" }

$asset = "protectorate-node-windows-$arch.exe"
Say "Protectorate node $Tag for windows-$arch from $($script:Base)"
if ($DryRun) {
    if ((Sha (Bytes $asset)) -ne (Want $asset)) { Die "$asset does not match SHA256SUMS" }
    Say "  verified $asset against SHA256SUMS"
}

$rel = Join-Path $StateDir 'releases'
$curMarker = Join-Path $rel 'current.txt'
$cur = Join-Path $rel 'current'
$joined = Test-Path -LiteralPath (Join-Path $StateDir 'config.json')
$service = Test-Path -LiteralPath (Join-Path $StateDir 'install-manifest.json')
$haveCurrent = (Test-Path -LiteralPath $curMarker) -or (Test-Path -LiteralPath $cur)
$join = @('join', $JoinToken, '--home', $StateDir)
if ($Name) { $join += @('--name', $Name) }
$installService = @('install-service', '--home', $StateDir, '--service-name', $ServiceName)
Say "Found: state $StateDir (joined: $joined, service installed: $service, current present: $haveCurrent)"
Say 'Plan:'
if (Test-Path -LiteralPath (Join-Path $rel $Tag)) { Say "  reuse    $rel\$Tag (checked against SHA256SUMS)" }
else { Say "  stage    $rel\$Tag\protectorate-node.exe" }
if ($haveCurrent) { Say "  stop     $curMarker exists: already installed; update from the hub" }
else { Say "  mark     $curMarker -> $Tag" }
if ($joined) { Say "  skip     join: $StateDir\config.json exists" }
else { Say "  run      $rel\$Tag\protectorate-node.exe $($join -join ' ')" }
if ($service) { Say "  skip     install-service: $StateDir\install-manifest.json exists" }
else { Say "  run      $rel\$Tag\protectorate-node.exe $($installService -join ' ')" }
if ($DryRun) { Say 'Dry run: nothing was written.'; exit 0 }
if ($haveCurrent) { Die "$StateDir is installed already; update from the hub" }

New-Item -ItemType Directory -Force -Path $rel | Out-Null
$vdir = Join-Path $rel $Tag
$exePath = Join-Path $vdir 'protectorate-node.exe'
if (Test-Path -LiteralPath $vdir) {
    CheckFile $asset $exePath
} else {
    $staging = Join-Path $rel ".staging-$Tag-$PID"
    New-Item -ItemType Directory -Path $staging | Out-Null
    try {
        $dest = Join-Path $staging 'protectorate-node.exe'
        if ($From) { Copy-Item -LiteralPath (Join-Path $From $asset) -Destination $dest }
        else {
            & curl.exe -q --fail --silent --show-error --location --proto '=https' --proto-redir '=https' --retry 3 `
                -o $dest "$($script:Base)/$asset"
            if ($LASTEXITCODE -ne 0) { Die "downloading $asset failed" }
        }
        CheckFile $asset $dest
        [IO.File]::WriteAllBytes((Join-Path $staging 'SHA256SUMS'), $sumsBytes)
        [IO.File]::WriteAllBytes((Join-Path $staging 'VERSION'), $versionBytes)
        $reports = @(& $dest --version)
        if ($LASTEXITCODE -ne 0) { Die 'the staged node binary does not run' }
        if ($reports[0] -ne "protectorate-node $Tag" -and -not $reports[0].StartsWith("protectorate-node $Tag ")) {
            Die "the staged node reports '$($reports[0])', not $Tag"
        }
        Rename-Item -LiteralPath $staging -NewName $Tag
    } catch {
        Remove-Item -LiteralPath $staging -Recurse -Force -ErrorAction SilentlyContinue
        throw
    }
}

[IO.File]::WriteAllText($curMarker, "$Tag`n")
if (Test-Path -LiteralPath $cur) { Remove-Item -LiteralPath $cur -Force -ErrorAction SilentlyContinue }
& cmd.exe /c mklink /D "$cur" "$Tag" 2>$null | Out-Null
Say "current -> $Tag"

if (-not $joined) {
    $jout = & $exePath @join
    if ($LASTEXITCODE -ne 0) { Die 'join failed' }
    $jout | ForEach-Object { Say $_ }
    $nName = ($jout | Where-Object { $_ -match '^\s*name:\s*(.+)$' } | ForEach-Object { $Matches[1].Trim() } | Select-Object -First 1)
    $nKey = ($jout | Where-Object { $_ -match '^\s*node_key:\s*(.+)$' } | ForEach-Object { $Matches[1].Trim() } | Select-Object -First 1)
    if ($nName -and $nKey) {
        Say "If not already pre-registered, run on the hub: protectorate node add $nName`:$nKey"
    }
    $elapsed = 0
    $authed = $false
    while ($elapsed -le $WaitSecs) {
        & $exePath authorize --home $StateDir *> $null
        if ($LASTEXITCODE -eq 0) { $authed = $true; break }
        if ($elapsed -eq 0) { Say "Waiting for hub authorization (up to ${WaitSecs}s)..." }
        if ($elapsed -ge $WaitSecs) { break }
        Start-Sleep -Seconds 3
        $elapsed += 3
    }
    if ($authed) { Say 'Authorized against hub.' }
    else { Say 'warning: node not authorized yet; service will connect once approved on the hub.' }
}
if (-not $service) {
    & $exePath @installService
    if ($LASTEXITCODE -ne 0) { Die 'install-service failed' }
}
Say "Installed Protectorate node $Tag as the Windows service $ServiceName."
