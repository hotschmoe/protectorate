param(
    [string]$Version = "",
    [ValidateSet("amd64", "arm64")][string]$Architecture = ""
)

$ErrorActionPreference = "Stop"
if ($Version) {
    $env:PROTECTORATE_BUILD_VERSION = $Version
}
$hostTriple = (rustc -vV | Select-String '^host: (.+)$').Matches.Groups[1].Value
if (-not $Architecture) {
    $Architecture = switch ($hostTriple) {
        "x86_64-pc-windows-msvc" { "amd64" }
        "aarch64-pc-windows-msvc" { "arm64" }
        default { throw "Unsupported native Rust Windows host: $hostTriple" }
    }
}
$target = if ($Architecture -eq "arm64") { "aarch64-pc-windows-msvc" } else { "x86_64-pc-windows-msvc" }
if ($hostTriple -cne $target) {
    throw "Native Windows $Architecture runner required (Rust host: $hostTriple)"
}

cargo build --locked --release --target $target -p protectorate-node-windows -p protectorate-cli -p protectorate-mcpbridge --bins
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
cargo test --locked --target $target -p protectorate-node-windows -p protectorate-node -p protectorate-cli -p protectorate-mcpbridge
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

New-Item -ItemType Directory -Force -Path dist | Out-Null
$release = Join-Path (Join-Path "target" $target) "release"
Copy-Item -LiteralPath (Join-Path $release "protectorate-rs-windows.exe") -Destination "dist/protectorate-rs-windows-$Architecture.exe"
Copy-Item -LiteralPath (Join-Path $release "protectorate.exe") -Destination "dist/protectorate-windows-$Architecture.exe"
Copy-Item -LiteralPath (Join-Path $release "protectorate-mcpbridge.exe") -Destination "dist/protectorate-mcpbridge-windows-$Architecture.exe"

$reported = & "dist/protectorate-rs-windows-$Architecture.exe" version --json | ConvertFrom-Json
if ($LASTEXITCODE -ne 0 -or -not $reported.version) {
    throw "Rust Windows node version check failed"
}
if ($Version -and $reported.version -ne $Version) {
    throw "Rust Windows node reports $($reported.version), expected $Version"
}
Write-Output "Rust Windows $Architecture node/CLI/MCP bridge built at version $($reported.version)"
