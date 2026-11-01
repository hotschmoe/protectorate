# Installs a Protectorate node on Windows from a GitHub release (SPEC 3.5, 6a). It downloads
# protectorate-node.exe for this machine (one file: it embeds member), checks it against the
# release's SHA256SUMS before using it and checks that it reports that release, stages it in
# <state>\releases\<version>\ (the layout pt-update keeps), makes releases\current point at that version, then runs
# `protectorate-node join SERVER_URL` (scan to join: it shows a QR code and waits for the owner to
# approve on the hub) and `protectorate-node install-service`, which registers and starts the
# Windows service protectorate-node. It prints the plan first; -DryRun checks the release and
# stops there, writing nothing. The state dir defaults to the binary's own,
# %ProgramData%\protectorate-node.
#
# Fetch this script at a commit, never a tag (SPEC 12a item 9), and compare its sha256 with the
# one in the release notes before running it, from an elevated PowerShell:
#   curl.exe -fsSLO https://raw.githubusercontent.com/hotschmoe/protectorate/<commit>/scripts/install-node.ps1
#   Get-FileHash -Algorithm SHA256 .\install-node.ps1
#   powershell -ExecutionPolicy Bypass -File .\install-node.ps1 -ServerUrl https://hub.example.lan -DryRun
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$ServerUrl,
    [string]$Name = '',
    [string]$Version = '',
    [ValidateSet('stable', 'dev')][string]$Channel = 'stable',
    [string]$From = '',
    [string]$Repo = 'hotschmoe/protectorate',
    [string]$StateDir = (Join-Path $env:ProgramData 'protectorate-node'),
    [switch]$DryRun
)

$ErrorActionPreference = 'Stop'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
$ProgressPreference = 'SilentlyContinue'

function Die([string]$msg) { [Console]::Error.WriteLine("install-node: $msg"); exit 1 }
function Say([string]$msg) { Write-Output $msg }

# A sort key for vX.Y.Z[-PRE] by semver precedence: numbers padded, a release after its
# pre-releases, numeric identifiers before alphanumeric ones.
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

# An asset's bytes in memory, so a dry run stores nothing. The leading comma keeps PowerShell
# from unrolling the array into the pipeline.
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

if ($ServerUrl -notmatch '^https?://[A-Za-z0-9.:\[\]-]+(/.*)?$') { Die 'ServerUrl must be http(s)://host[:port]' }
if ($Repo -notmatch '^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$') { Die 'Repo must be OWNER/NAME' }
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
$cur = Join-Path $rel 'current'
$joined = Test-Path -LiteralPath (Join-Path $StateDir 'node.json')
$service = Test-Path -LiteralPath (Join-Path $StateDir 'install-manifest.json')
$haveCurrent = Test-Path -LiteralPath $cur
$join = @('join', $ServerUrl, '--state-dir', $StateDir)
if ($Name) { $join += @('--name', $Name) }
$installService = @('install-service', '--state-dir', $StateDir)
Say "Found: state $StateDir (joined: $joined, service installed: $service, releases\current: $haveCurrent)"
Say 'Plan:'
if (Test-Path -LiteralPath (Join-Path $rel $Tag)) { Say "  reuse    $rel\$Tag (checked against SHA256SUMS)" }
else { Say "  stage    $rel\$Tag\protectorate-node.exe" }
if ($haveCurrent) { Say "  stop     $cur exists: already installed; update from the hub" }
else { Say "  link     $cur -> $Tag" }
if ($joined) { Say "  skip     join: $StateDir\node.json exists" }
else { Say "  run      $cur\protectorate-node.exe $($join -join ' ')" }
if ($service) { Say "  skip     install-service: $StateDir\install-manifest.json exists" }
else { Say "  run      $cur\protectorate-node.exe $($installService -join ' ')" }
if ($DryRun) { Say 'Dry run: nothing was written.'; exit 0 }
if ($haveCurrent) { Die "$cur exists; this node is installed already" }

New-Item -ItemType Directory -Force -Path $rel | Out-Null
$vdir = Join-Path $rel $Tag
if (Test-Path -LiteralPath $vdir) {
    CheckFile $asset (Join-Path $vdir 'protectorate-node.exe')
} else {
    $staging = Join-Path $rel ".staging-$Tag-$PID"
    New-Item -ItemType Directory -Path $staging | Out-Null
    try {
        $dest = Join-Path $staging 'protectorate-node.exe'
        if ($From) { Copy-Item -LiteralPath (Join-Path $From $asset) -Destination $dest }
        else {
            & curl.exe --fail --silent --show-error --location --proto '=https' --proto-redir '=https' --retry 3 `
                -o $dest "$($script:Base)/$asset"
            if ($LASTEXITCODE -ne 0) { Die "downloading $asset failed" }
        }
        CheckFile $asset $dest
        [IO.File]::WriteAllBytes((Join-Path $staging 'SHA256SUMS'), $sumsBytes)
        [IO.File]::WriteAllBytes((Join-Path $staging 'VERSION'), $versionBytes)
        $reports = @(& $dest --version)
        if ($LASTEXITCODE -ne 0) { Die 'the staged node binary does not run' }
        if ($reports[0] -ne "protectorate-node $Tag") { Die "the staged node reports '$($reports[0])', not $Tag" }
        if (-not ($reports -like 'member: embedded*')) { Die "the staged node does not embed member: $($reports -join ' ')" }
        Rename-Item -LiteralPath $staging -NewName $Tag
    } catch {
        # Only this run's own staging directory inside releases\.
        Remove-Item -LiteralPath $staging -Recurse -Force -ErrorAction SilentlyContinue
        throw
    }
}
# A relative directory symlink, as pt-update reads it.
& cmd.exe /c mklink /D "$cur" "$Tag" | Out-Null
if ($LASTEXITCODE -ne 0) { Die "creating the $cur link failed" }
Say "$cur -> $Tag"

if (-not $joined) {
    Say "Joining $ServerUrl`: approve this node on the hub when the QR code shows."
    & (Join-Path $cur 'protectorate-node.exe') @join
    if ($LASTEXITCODE -ne 0) { Die 'join failed' }
}
if (-not $service) {
    & (Join-Path $cur 'protectorate-node.exe') @installService
    if ($LASTEXITCODE -ne 0) { Die 'install-service failed' }
}
Say "Installed Protectorate node $Tag as the Windows service protectorate-node."
