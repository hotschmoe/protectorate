import 'dart:io' as io;

import 'package:jsonrpc_spike/client.dart';

Future<void> main() async {
  final c = Client('http://127.0.0.1:7472');
  final sw = Stopwatch()..start();
  final sub = c.subscribe().listen(
        (e) => print('[${sw.elapsedMilliseconds}ms] event: ${e.type} ${e.sleeveId} ${e.message}'),
        onError: (Object e) => print('stream error: $e'),
        onDone: () => print('stream done'),
      );
  await Future<void>.delayed(const Duration(milliseconds: 300));
  final name = 'dart-${DateTime.now().millisecondsSinceEpoch % 100000}';
  final s = await c.spawn(name, harness: 'gemini');
  print('spawn: ${s.id} ${s.name} ${s.harness}');
  try {
    await c.spawn(name);
  } on RpcException catch (e) {
    print('error: code=${e.code} message=${e.message}');
  }
  print('list: ${(await c.list()).map((s) => s.name).toList()}');
  await Future<void>.delayed(const Duration(milliseconds: 2500));
  await sub.cancel();
  io.exit(0);
}
