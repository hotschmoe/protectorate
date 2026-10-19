import 'dart:io' as io;

import 'package:connect_spike/pb/protectorate/v1/event.connect.client.dart';
import 'package:connect_spike/pb/protectorate/v1/event.pb.dart';
import 'package:connect_spike/pb/protectorate/v1/sleeve.connect.client.dart';
import 'package:connect_spike/pb/protectorate/v1/sleeve.pb.dart';
import 'package:connectrpc/connect.dart';
import 'package:connectrpc/io.dart';
import 'package:connectrpc/protobuf.dart';
import 'package:connectrpc/protocol/connect.dart' as protocol;

Future<void> main(List<String> args) async {
  final binary = args.contains('--binary');
  final transport = protocol.Transport(
    baseUrl: 'http://127.0.0.1:7471',
    codec: binary ? const ProtoCodec() : const JsonCodec(),
    httpClient: createHttpClient(io.HttpClient()),
  );
  final sleeves = SleeveServiceClient(transport);
  final events = EventServiceClient(transport);
  final sw = Stopwatch()..start();

  final sub = events.subscribe(SubscribeRequest()).listen(
        (e) => print('[${sw.elapsedMilliseconds}ms] event: ${e.type} ${e.sleeveId} ${e.message}'),
        onError: (Object e) => print('stream error: $e'),
        onDone: () => print('stream done'),
      );

  await Future<void>.delayed(const Duration(milliseconds: 300));
  final name = 'dart-${DateTime.now().millisecondsSinceEpoch % 100000}';
  final r = await sleeves.spawn(SpawnRequest(name: name, harness: 'gemini'));
  print('spawn: ${r.sleeve.id} ${r.sleeve.name} ${r.sleeve.harness}');
  try {
    await sleeves.spawn(SpawnRequest(name: name));
  } on ConnectException catch (e) {
    print('error: code=${e.code} message=${e.message}');
  }
  final l = await sleeves.list(ListRequest());
  print('list: ${l.sleeves.map((s) => s.name).toList()}');

  await Future<void>.delayed(const Duration(milliseconds: 2500));
  await sub.cancel();
  io.exit(0);
}
