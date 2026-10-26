$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$installer = Join-Path (Split-Path $PSScriptRoot -Parent) "install-node-windows.ps1"
$bundle = Join-Path ([IO.Path]::GetTempPath()) ("protectorate-installer-test-" + [Guid]::NewGuid().ToString("N"))
$version = "v9.8.7"
$server = "protectorate-rs-windows-amd64.exe"
$cli = "protectorate-windows-amd64.exe"
$bridge = "protectorate-mcpbridge-windows-amd64.exe"
$state = Join-Path $env:LOCALAPPDATA "Protectorate-Node"
$install = Join-Path $env:LOCALAPPDATA "Programs\Protectorate-Node"
$stateBefore = Test-Path -LiteralPath $state
$installBefore = Test-Path -LiteralPath $install
$taskBefore = [bool](Get-ScheduledTask -TaskName "Protectorate-Node" -ErrorAction SilentlyContinue)

function Pin-Manifest {
    return (Get-FileHash -LiteralPath (Join-Path $bundle "SHA256SUMS") -Algorithm SHA256).Hash
}

function Expect-Failure([string]$Label, [string]$Pin, [string]$Architecture = "amd64") {
    $failed = $false
    try {
        & $installer -Version $version -ManifestSha256 $Pin -BundleDir $bundle -Architecture $Architecture -VerifyOnly | Out-Null
    } catch {
        $failed = $true
    }
    if (-not $failed) { throw "$Label was accepted" }
}

try {
    New-Item -ItemType Directory -Path $bundle | Out-Null
    [IO.File]::WriteAllText((Join-Path $bundle "VERSION"), "$version`n")
    [IO.File]::WriteAllBytes((Join-Path $bundle $server), [byte[]](77, 90, 1, 2))
    [IO.File]::WriteAllBytes((Join-Path $bundle $cli), [byte[]](77, 90, 3, 4))
    [IO.File]::WriteAllBytes((Join-Path $bundle $bridge), [byte[]](77, 90, 5, 6))
    $serverHash = (Get-FileHash -LiteralPath (Join-Path $bundle $server) -Algorithm SHA256).Hash.ToLowerInvariant()
    $cliHash = (Get-FileHash -LiteralPath (Join-Path $bundle $cli) -Algorithm SHA256).Hash.ToLowerInvariant()
    $bridgeHash = (Get-FileHash -LiteralPath (Join-Path $bundle $bridge) -Algorithm SHA256).Hash.ToLowerInvariant()
    $lines = "$serverHash  $server`n$cliHash  $cli`n$bridgeHash  $bridge`n"
    [IO.File]::WriteAllText((Join-Path $bundle "SHA256SUMS"), $lines, [Text.Encoding]::ASCII)
    $pin = Pin-Manifest
    & $installer -Version $version -ManifestSha256 $pin -BundleDir $bundle -Architecture amd64 -VerifyOnly | Out-Null
    if ((Get-ItemProperty 'HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\Environment').PROCESSOR_ARCHITECTURE -eq "AMD64") {
        & $installer -Version $version -ManifestSha256 $pin -BundleDir $bundle -VerifyOnly | Out-Null
    }

    [IO.File]::WriteAllBytes((Join-Path $bundle $cli), [byte[]](77, 90, 9, 9))
    Expect-Failure "Tampered CLI" $pin
    [IO.File]::WriteAllBytes((Join-Path $bundle $cli), [byte[]](77, 90, 3, 4))
    [IO.File]::WriteAllBytes((Join-Path $bundle $bridge), [byte[]](77, 90, 9, 9))
    Expect-Failure "Tampered MCP bridge" $pin
    [IO.File]::WriteAllBytes((Join-Path $bundle $bridge), [byte[]](77, 90, 5, 6))
    Remove-Item -LiteralPath (Join-Path $bundle $bridge)
    Expect-Failure "Missing MCP bridge" $pin
    [IO.File]::WriteAllBytes((Join-Path $bundle $bridge), [byte[]](77, 90, 5, 6))

    [IO.File]::AppendAllText((Join-Path $bundle "SHA256SUMS"), "$serverHash  $server`n")
    Expect-Failure "Duplicate asset entry" (Pin-Manifest)
    [IO.File]::WriteAllText((Join-Path $bundle "SHA256SUMS"), $lines, [Text.Encoding]::ASCII)
    Expect-Failure "Untrusted manifest" ("0" * 64)

    [IO.File]::WriteAllText((Join-Path $bundle "VERSION"), "v0.0.0`n")
    Expect-Failure "Wrong bundle version" $pin
    [IO.File]::WriteAllText((Join-Path $bundle "VERSION"), "$version`n")
    $binaryModeLines = "$serverHash  *$server`n$cliHash  *$cli`n$bridgeHash  *$bridge`n"
    [IO.File]::WriteAllText((Join-Path $bundle "SHA256SUMS"), $binaryModeLines, [Text.Encoding]::ASCII)
    & $installer -Version $version -ManifestSha256 (Pin-Manifest) -BundleDir $bundle -Architecture amd64 -VerifyOnly | Out-Null

    $armServer = "protectorate-rs-windows-arm64.exe"
    $armCli = "protectorate-windows-arm64.exe"
    $armBridge = "protectorate-mcpbridge-windows-arm64.exe"
    [IO.File]::WriteAllBytes((Join-Path $bundle $armServer), [byte[]](77, 90, 11, 12))
    [IO.File]::WriteAllBytes((Join-Path $bundle $armCli), [byte[]](77, 90, 13, 14))
    [IO.File]::WriteAllBytes((Join-Path $bundle $armBridge), [byte[]](77, 90, 15, 16))
    $armServerHash = (Get-FileHash -LiteralPath (Join-Path $bundle $armServer) -Algorithm SHA256).Hash.ToLowerInvariant()
    $armCliHash = (Get-FileHash -LiteralPath (Join-Path $bundle $armCli) -Algorithm SHA256).Hash.ToLowerInvariant()
    $armBridgeHash = (Get-FileHash -LiteralPath (Join-Path $bundle $armBridge) -Algorithm SHA256).Hash.ToLowerInvariant()
    $armLines = "$armServerHash  $armServer`n$armCliHash  $armCli`n$armBridgeHash  $armBridge`n"
    [IO.File]::WriteAllText((Join-Path $bundle "SHA256SUMS"), $armLines, [Text.Encoding]::ASCII)
    $armPin = Pin-Manifest
    & $installer -Version $version -ManifestSha256 $armPin -BundleDir $bundle -Architecture arm64 -VerifyOnly | Out-Null
    if ((Get-ItemProperty 'HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\Environment').PROCESSOR_ARCHITECTURE -eq "ARM64") {
        & $installer -Version $version -ManifestSha256 $armPin -BundleDir $bundle -VerifyOnly | Out-Null
    }
    Expect-Failure "Wrong architecture assets" $armPin "amd64"
    [IO.File]::WriteAllBytes((Join-Path $bundle $armBridge), [byte[]](77, 90, 9, 9))
    Expect-Failure "Tampered ARM64 MCP bridge" $armPin "arm64"
    [IO.File]::WriteAllBytes((Join-Path $bundle $armBridge), [byte[]](77, 90, 15, 16))

    if ((Test-Path -LiteralPath $state) -ne $stateBefore) { throw "VerifyOnly touched node state" }
    if ((Test-Path -LiteralPath $install) -ne $installBefore) { throw "VerifyOnly touched install directory" }
    if ([bool](Get-ScheduledTask -TaskName "Protectorate-Node" -ErrorAction SilentlyContinue) -ne $taskBefore) { throw "VerifyOnly changed the scheduled task" }
    Write-Output "Windows node installer manifest tests passed"
} finally {
    if (Test-Path -LiteralPath $bundle) { Remove-Item -LiteralPath $bundle -Recurse -Force }
}
