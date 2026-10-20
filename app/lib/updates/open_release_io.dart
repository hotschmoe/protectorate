import 'dart:io';

import 'package:flutter/services.dart';

Future<void> openRelease(Uri uri) async {
  if (Platform.isAndroid) {
    await const MethodChannel(
      'protectorate/browser',
    ).invokeMethod<void>('open', uri.toString());
    return;
  }
  final command = Platform.isWindows
      ? 'rundll32.exe'
      : Platform.isMacOS
      ? 'open'
      : 'xdg-open';
  final result = await Process.run(command, [
    if (Platform.isWindows) 'url.dll,FileProtocolHandler',
    uri.toString(),
  ]);
  if (result.exitCode != 0) {
    throw ProcessException(command, [uri.toString()], '${result.stderr}');
  }
}
