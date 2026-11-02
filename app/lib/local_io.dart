import 'dart:convert';
import 'dart:io';

import 'package:flutter/services.dart';

final Map<String, String> _settings = {};
File? _file;

/// Android has no files dir in dart:io, so MainActivity reports `filesDir` over a channel.
Future<void> initSettings() async {
  try {
    final dir = await _settingsDir();
    if (dir == null) return;
    _file = File('${dir.path}${Platform.pathSeparator}settings.json');
    final data = jsonDecode(await _file!.readAsString());
    if (data is Map) {
      for (final e in data.entries) {
        if (e.key is String && e.value is String) _settings[e.key as String] = e.value as String;
      }
    }
  } catch (_) {}
}

Future<Directory?> _settingsDir() async {
  final env = Platform.environment;
  String? base;
  if (Platform.isAndroid) {
    base = await const MethodChannel('protectorate/files').invokeMethod<String>('filesDir');
  } else if (Platform.isWindows) {
    base = env['APPDATA'];
  } else {
    final xdg = env['XDG_CONFIG_HOME'];
    final home = env['HOME'];
    base = xdg != null && xdg.isNotEmpty ? xdg : (home == null ? null : '$home/.config');
  }
  if (base == null || base.isEmpty) return null;
  if (Platform.isAndroid) return Directory(base);
  final dir = Directory('$base${Platform.pathSeparator}protectorate');
  await dir.create(recursive: true);
  // The file holds a bearer token. Android's filesDir is already private to the app.
  if (!Platform.isWindows && (await Process.run('chmod', ['700', dir.path])).exitCode != 0) {
    return null;
  }
  return dir;
}

String? loadSetting(String key) => _settings[key];

void saveSetting(String key, String? value) {
  if (value == null || value.isEmpty) {
    _settings.remove(key);
  } else {
    _settings[key] = value;
  }
  _flush();
}

void _flush() {
  final f = _file;
  if (f == null) return;
  try {
    final tmp = File('${f.path}.tmp');
    tmp.writeAsStringSync(jsonEncode(_settings), flush: true);
    tmp.renameSync(f.path);
  } catch (_) {}
}

String? pageOrigin() => null;

void dropQueryParam(String key) {}
