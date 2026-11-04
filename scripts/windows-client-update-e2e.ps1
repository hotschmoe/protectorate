# Run on Windows with Dart installed. Uses only disposable executables and leaves artifacts.
param(
  [string]$Source = "$PSScriptRoot\..\app\lib\client_update_io.dart",
  [string]$Dart = 'dart.exe',
  [string]$Work = (Join-Path $env:TEMP ("Protectorate update e2e " + [guid]::NewGuid()))
)
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$originalTemp = $env:TEMP
$originalTmp = $env:TMP
try {
  New-Item -ItemType Directory -Force "$Work\installed", "$Work\payload", "$Work\temp" | Out-Null
  $env:TEMP = "$Work\temp"
  $env:TMP = "$Work\temp"
  $sourceText = [IO.File]::ReadAllText((Resolve-Path $Source))
  $start = $sourceText.IndexOf('String _ps(')
  if ($start -lt 0) { throw 'Windows updater function not found' }
  $old = @"
import 'dart:async';
import 'dart:convert';
import 'dart:io';
$($sourceText.Substring($start))
Future<void> main(List<String> args) async {
  await _installWindows(args.single);
  exit(0);
}
"@
  $new = @"
import 'dart:io';
void main() {
  File(Platform.resolvedExecutable + '.success').writeAsStringSync('updated and restarted');
}
"@
  [IO.File]::WriteAllText("$Work\old.dart", $old)
  [IO.File]::WriteAllText("$Work\new.dart", $new)
  & $Dart compile exe "$Work\old.dart" -o "$Work\installed\update-e2e.exe"
  if ($LASTEXITCODE) { throw 'Compiling the updater harness failed' }
  & $Dart compile exe "$Work\new.dart" -o "$Work\payload\update-e2e.exe"
  if ($LASTEXITCODE) { throw 'Compiling the replacement app failed' }
  Compress-Archive -LiteralPath "$Work\payload\update-e2e.exe" -DestinationPath "$Work\update.zip"
  & "$Work\installed\update-e2e.exe" "$Work\update.zip"
  if ($LASTEXITCODE) { throw 'The updater harness failed' }
  $deadline = (Get-Date).AddSeconds(30)
  while (!(Test-Path "$Work\installed\update-e2e.exe.success") -and (Get-Date) -lt $deadline) {
    Start-Sleep -Milliseconds 200
  }
  if (!(Test-Path "$Work\installed\update-e2e.exe.success")) {
    throw "The replacement app did not restart; artifacts: $Work"
  }
  if ((Get-FileHash "$Work\payload\update-e2e.exe").Hash -ne
      (Get-FileHash "$Work\installed\update-e2e.exe").Hash) {
    throw 'The installed executable differs from the replacement'
  }
  "PASS: updater startup, parent exit, extraction, copy, restart and hash; artifacts: $Work"
} finally {
  $env:TEMP = $originalTemp
  $env:TMP = $originalTmp
}
