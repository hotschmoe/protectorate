import 'dart:io';

Future<void> main(List<String> arguments) async {
  if (arguments.any(
    (arg) => arg == '--wasm' || arg.startsWith('--output') || arg == '-o',
  )) {
    stderr.writeln('Build web: WASM and custom output paths are unsupported.');
    exitCode = 1;
    return;
  }
  final app = File.fromUri(Platform.script).parent.parent;
  final result = await Process.start(
    'flutter',
    [
      'build',
      'web',
      '--no-web-resources-cdn',
      '--no-wasm-dry-run',
      ...arguments,
    ],
    workingDirectory: app.path,
    mode: ProcessStartMode.inheritStdio,
  );
  exitCode = await result.exitCode;
  if (exitCode != 0) return;

  final engine = Directory('${app.path}/build/web/canvaskit');
  const retained = {'canvaskit.js', 'canvaskit.wasm'};
  for (final name in retained) {
    if (!File('${engine.path}/$name').existsSync()) {
      stderr.writeln('Build web: missing CanvasKit asset $name.');
      exitCode = 1;
      return;
    }
  }
  for (final entry in engine.listSync()) {
    final name = entry.uri.pathSegments.where((part) => part.isNotEmpty).last;
    if (!retained.contains(name)) entry.deleteSync(recursive: true);
  }
}
