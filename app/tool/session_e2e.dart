import 'dart:io' as io;

import 'package:connectrpc/connect.dart';
import 'package:connectrpc/io.dart' as rpcio;
import 'package:connectrpc/protobuf.dart';
import 'package:connectrpc/protocol/connect.dart' as protocol;

import 'package:protectorate/gen/protectorate/v1/session.connect.client.dart';
import 'package:protectorate/gen/protectorate/v1/session.pb.dart';
import 'package:protectorate/hubs/connection.dart';

Future<void> main(List<String> args) async {
  if (args.length != 2) throw ArgumentError('expected hub URL and owner token');
  final http = io.HttpClient()..connectionTimeout = const Duration(seconds: 8);
  final transport = protocol.Transport(
    baseUrl: args[0],
    codec: const JsonCodec(),
    httpClient: hubHttpClient(args[0], client: rpcio.createHttpClient(http)),
    interceptors: [
      <I extends Object, O extends Object>(AnyFn<I, O> next) =>
          (Request<I, O> request) {
            request.headers.set('authorization', ['Bearer ${args[1]}']);
            return next(request);
          },
    ],
  );
  final sessions = SessionServiceClient(transport);
  const name = 'flutter-session';
  void check(bool condition, String message) {
    if (!condition) throw StateError(message);
  }

  Future<Session> exited() async {
    final deadline = DateTime.now().add(const Duration(seconds: 10));
    while (DateTime.now().isBefore(deadline)) {
      final listed = await sessions.listSessions(ListSessionsRequest());
      for (final session in listed.sessions) {
        if (session.name == name &&
            session.state == SessionState.SESSION_STATE_EXITED) {
          return session;
        }
      }
      await Future<void>.delayed(const Duration(milliseconds: 50));
    }
    throw StateError('session did not exit');
  }

  try {
    check(
      (await sessions.listSessions(ListSessionsRequest())).sessions.isEmpty,
      'fresh hub already had a raw session',
    );
    final created = await sessions.createSession(
      CreateSessionRequest(
        name: name,
        command: ['/bin/sh', '-c', 'printf "flutter-session-ok\\n"; exit 7'],
      ),
    );
    check(created.session.name == name, 'create lost the session name');
    var ended = await exited();
    check(ended.exitCode == 7, 'session exit code was lost');
    check(ended.hasExitedAt(), 'session exit timestamp was lost');
    var pane = await sessions.capturePane(
      CapturePaneRequest(name: name, lines: 1),
    );
    check(pane.text.contains('flutter-session-ok'), 'final pane was lost');

    final restarted = await sessions.respawnSession(
      RespawnSessionRequest(name: name),
    );
    check(restarted.session.name == name, 'respawn lost the session name');
    ended = await exited();
    check(ended.exitCode == 7, 'respawn did not run the saved command');
    pane = await sessions.capturePane(CapturePaneRequest(name: name, lines: 1));
    check(pane.text.contains('flutter-session-ok'), 'respawn pane was lost');

    await sessions.killSession(KillSessionRequest(name: name));
    check(
      (await sessions.listSessions(ListSessionsRequest())).sessions.isEmpty,
      'removed session remained in list',
    );
    io.stdout.writeln(
      'Dart session create, list, exit, capture, respawn and removal passed.',
    );
  } finally {
    http.close(force: true);
  }
}
