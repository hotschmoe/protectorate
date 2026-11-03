import 'dart:async';
import 'dart:convert';
import 'dart:ffi' show Abi;
import 'dart:io';

import 'package:flutter/services.dart';

import 'api/api.g.dart';

const _channel = MethodChannel('protectorate/files');

const bool reloadsFromHub = false;

/// The running ABI picks the Windows build, so an x64 app emulated on arm64 stays x64.
String? clientAsset() {
  if (Platform.isAndroid) return 'protectorate-android.apk';
  if (!Platform.isWindows) return null;
  return switch (Abi.current()) {
    Abi.windowsX64 => 'protectorate-desktop-windows-amd64.zip',
    Abi.windowsArm64 => 'protectorate-desktop-windows-arm64.zip',
    _ => null,
  };
}

void reloadPage() {}

String _failure(int status, String body) {
  try {
    return 'HTTP $status: ${ErrorBody.fromJson(jsonDecode(body)).error.message}';
  } on Object {
    return 'HTTP $status';
  }
}

/// Into the cache directory (Android) or %TEMP%, through a `.part` file renamed once the whole
/// Content-Length arrived.
Future<String> downloadClient(Uri url, String? token, void Function(int got, int total) progress) async {
  final base = Platform.isAndroid
      ? await _channel.invokeMethod<String>('cacheDir')
      : Directory.systemTemp.path;
  if (base == null) throw StateError('no cache directory');
  final file = File('$base${Platform.pathSeparator}${url.pathSegments.last}');
  final part = File('${file.path}.part');
  final client = HttpClient()..connectionTimeout = const Duration(seconds: 10);
  try {
    final req = await client.getUrl(url);
    req.headers.set(HttpHeaders.acceptEncodingHeader, 'identity');
    if (token != null) req.headers.set(HttpHeaders.authorizationHeader, 'Bearer $token');
    final resp = await req.close();
    if (resp.statusCode != 200) {
      throw StateError(_failure(resp.statusCode, await resp.transform(utf8.decoder).join()));
    }
    final total = resp.contentLength;
    if (total < 0) throw StateError('the hub sent no Content-Length');
    var got = 0;
    final sink = part.openWrite();
    try {
      await for (final chunk in resp) {
        sink.add(chunk);
        got += chunk.length;
        progress(got, total);
      }
    } finally {
      await sink.close();
    }
    if (got != total) throw StateError('the download stopped at $got of $total bytes');
    return (await part.rename(file.path)).path;
  } finally {
    client.close(force: true);
  }
}

Future<String> installClient(String path) async {
  if (Platform.isAndroid) {
    final r = await _channel.invokeMethod<String>('install', {'path': path});
    return r == 'allow'
        ? 'Allow installs from Protectorate, then press Install again'
        : 'Confirm the update in the Android dialog';
  }
  if (Platform.isWindows) {
    await _installWindows(path);
    exit(0);
  }
  throw UnsupportedError('no installer for ${Platform.operatingSystem}');
}

String _ps(String s) => "'${s.replaceAll("'", "''")}'";

/// A detached PowerShell script waits for this process to exit, unpacks the zip and copies it
/// over the running exe's directory, elevated (one UAC prompt) only when that directory is not
/// writable, then starts the app again unelevated. A failure is logged to
/// %TEMP%\protectorate-update.log and the app starts as it was.
Future<void> _installWindows(String zip) async {
  final tmp = Directory.systemTemp.path;
  final exe = Platform.resolvedExecutable;
  final dir = File(exe).parent.path;
  final stage = '$tmp\\protectorate-update';
  final copy = 'robocopy ${_ps(stage)} ${_ps(dir)} /E /IS /R:10 /W:1 /NFL /NDL /NJH /NJS /NP | Out-Null; '
      'exit [int](\$LASTEXITCODE -ge 8)';
  final encoded = base64Encode([
    for (final u in copy.codeUnits) ...[u & 0xff, u >> 8],
  ]);
  final script = File('$tmp\\protectorate-update.ps1');
  // The BOM makes Windows PowerShell 5.1 read the script as UTF-8, not the ANSI code page.
  await script.writeAsString('''\uFEFF
\$ErrorActionPreference = 'Stop'
\$dest = ${_ps(dir)}
\$stage = ${_ps(stage)}
\$copy = @('-NoProfile', '-NonInteractive', '-EncodedCommand', '$encoded')
try {
  Wait-Process -Id $pid -Timeout 60 -ErrorAction SilentlyContinue
  if (Test-Path -LiteralPath \$stage) { Remove-Item -LiteralPath \$stage -Recurse -Force }
  Expand-Archive -LiteralPath ${_ps(zip)} -DestinationPath \$stage
  \$probe = Join-Path \$dest '.protectorate-update-probe'
  \$writable = \$true
  try { [IO.File]::WriteAllText(\$probe, ''); Remove-Item -LiteralPath \$probe } catch { \$writable = \$false }
  if (\$writable) {
    & powershell.exe @copy
    \$code = \$LASTEXITCODE
  } else {
    \$code = (Start-Process powershell.exe -Verb RunAs -Wait -PassThru -WindowStyle Hidden -ArgumentList \$copy).ExitCode
  }
  if (\$code -ne 0) { throw "copying into \$dest failed (exit \$code)" }
  Remove-Item -LiteralPath \$stage -Recurse -Force -ErrorAction SilentlyContinue
} catch {
  \$_ | Out-String | Out-File -LiteralPath ${_ps('$tmp\\protectorate-update.log')} -Encoding utf8
}
Start-Process -FilePath ${_ps(exe)}
''');
  await Process.start(
    'powershell.exe',
    ['-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-WindowStyle', 'Hidden', '-File', script.path],
    mode: ProcessStartMode.detached,
  );
}
