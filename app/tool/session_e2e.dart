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

  // tmux drops pty output still in flight when a pane's process exits, so the
  // command stays alive until its output is on the pane and the script
  // releases it.
  final scratch = io.Directory.systemTemp.createTempSync(
    'protectorate-session-e2e-',
  );
  final release = io.File('${scratch.path}/release');

  Future<String> paneWith(String text) async {
    final deadline = DateTime.now().add(const Duration(seconds: 60));
    while (DateTime.now().isBefore(deadline)) {
      final pane = await sessions.capturePane(
        CapturePaneRequest(name: name, lines: 1),
      );
      if (pane.text.contains(text)) return pane.text;
      await Future<void>.delayed(const Duration(milliseconds: 50));
    }
    throw StateError('pane never showed $text');
  }

  Future<Session> exited() async {
    final deadline = DateTime.now().add(const Duration(seconds: 60));
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
        command: [
          '/bin/sh',
          '-c',
          r'printf "flutter-session-ok\n"; while [ ! -e "$1" ]; do sleep 0.05; done; exit 7',
          'sh',
          release.path,
        ],
      ),
    );
    check(created.session.name == name, 'create lost the session name');
    await paneWith('flutter-session-ok');
    release.createSync();
    var ended = await exited();
    check(ended.exitCode == 7, 'session exit code was lost');
    check(ended.hasExitedAt(), 'session exit timestamp was lost');
    var pane = await sessions.capturePane(
      CapturePaneRequest(name: name, lines: 1),
    );
    check(pane.text.contains('flutter-session-ok'), 'final pane was lost');

    release.deleteSync();
    final restarted = await sessions.respawnSession(
      RespawnSessionRequest(name: name),
    );
    check(restarted.session.name == name, 'respawn lost the session name');
    await paneWith('flutter-session-ok');
    release.createSync();
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
    scratch.deleteSync(recursive: true);
    http.close(force: true);
  }
}
