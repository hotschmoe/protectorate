import 'dart:async';
import 'dart:io';

import '../../../app/lib/api/api.g.dart';
import '../../../app/lib/real_hub.dart';

Future<void> waitFile(String path) async {
  final deadline = DateTime.now().add(const Duration(seconds: 60));
  while (!File(path).existsSync()) {
    if (DateTime.now().isAfter(deadline))
      throw StateError('timed out waiting for $path');
    await Future<void>.delayed(const Duration(milliseconds: 20));
  }
}

Future<void> main(List<String> args) async {
  final port = int.parse(args[0]);
  final token = args[1];
  final dir = args[2];
  final worker = args[3];
  final listener = await ServerSocket.bind(InternetAddress.loopbackIPv4, 0);
  final sockets = <Socket>[];
  final incoming = listener.listen((client) async {
    final upstream = await Socket.connect(InternetAddress.loopbackIPv4, port);
    sockets.addAll([client, upstream]);
    client.listen(
      upstream.add,
      onDone: upstream.destroy,
      onError: (_) => upstream.destroy(),
    );
    upstream.listen(
      client.add,
      onDone: client.destroy,
      onError: (_) => client.destroy(),
    );
  });
  final hub = RealHub('http://127.0.0.1:${listener.port}', token: token);
  final transcript = hub.transcripts(worker).listen((_) {});
  final initial = Completer<void>();
  final reconnected = Completer<void>();
  var watching = false;
  var frames = 0;
  final states = hub.states.listen((_) {
    if (watching) {
      frames++;
      if (hub.connected && !reconnected.isCompleted) reconnected.complete();
    } else if (hub.connected && !initial.isCompleted) {
      initial.complete();
    }
  });
  await initial.future.timeout(const Duration(seconds: 30));
  File('$dir/ready').writeAsStringSync('ready');
  await waitFile('$dir/drop');
  incoming.pause();
  for (final socket in sockets) {
    socket.destroy();
  }
  while (hub.connected) {
    await Future<void>.delayed(const Duration(milliseconds: 20));
  }
  await Future<void>.delayed(const Duration(milliseconds: 100));
  File('$dir/dropped').writeAsStringSync('dropped');
  await waitFile('$dir/resume');
  watching = true;
  incoming.resume();
  await reconnected.future.timeout(const Duration(seconds: 30));
  final catchupFrames = frames;
  final deadline = DateTime.now().add(const Duration(seconds: 30));
  while (!hub
      .transcript(worker)
      .entries
      .any(
        (e) =>
            e.entry is EntryAssistantText &&
        (e.entry as EntryAssistantText).text.startsWith('activity 2999'),
      )) {
    if (DateTime.now().isAfter(deadline))
      throw StateError('transcript did not catch up');
    await Future<void>.delayed(const Duration(milliseconds: 20));
  }
  File('$dir/result').writeAsStringSync('$catchupFrames');
  await transcript.cancel();
  hub.close();
  await states.cancel();
  await incoming.cancel();
  await listener.close();
  for (final socket in sockets) {
    socket.destroy();
  }
}
