import 'dart:io' as io;

import 'package:connectrpc/connect.dart';
import 'package:connectrpc/io.dart' as rpcio;
import 'package:connectrpc/protobuf.dart';
import 'package:connectrpc/protocol/connect.dart' as protocol;

import 'package:protectorate/gen/protectorate/v1/hub.connect.client.dart';
import 'package:protectorate/gen/protectorate/v1/hub.pb.dart';
import 'package:protectorate/hubs/connection.dart';

Future<void> main(List<String> args) async {
  final baseUrl = args[0];
  final token = args[1];
  final http = io.HttpClient()..connectionTimeout = const Duration(seconds: 8);
  try {
    HubServiceClient client(String url, String bearer) => HubServiceClient(
      protocol.Transport(
        baseUrl: url,
        codec: const JsonCodec(),
        httpClient: hubHttpClient(url, client: rpcio.createHttpClient(http)),
        interceptors: [
          <I extends Object, O extends Object>(AnyFn<I, O> next) =>
              (Request<I, O> request) {
                request.headers.set('authorization', ['Bearer $bearer']);
                return next(request);
              },
        ],
      ),
    );
    void check(bool condition, String message) {
      if (!condition) throw StateError(message);
    }

    await probeHub(client(baseUrl, token), baseUrl);
    try {
      await probeHub(client(baseUrl, 'invalid-token'), baseUrl);
      throw StateError('accepted an invalid token');
    } on HubProbeException catch (error) {
      check(
        '$error'.contains('rejected the token'),
        'missing auth guidance: $error',
      );
    }

    final socket = await io.ServerSocket.bind(
      io.InternetAddress.loopbackIPv4,
      0,
    );
    final refusedUrl = 'http://127.0.0.1:${socket.port}';
    await socket.close();
    try {
      await client(refusedUrl, token).getStatus(GetStatusRequest());
      throw StateError('accepted a closed port');
    } on ConnectException catch (error) {
      check(
        error.code == Code.unavailable,
        'wrong connection error code: $error',
      );
      check(
        error.message.contains(refusedUrl) &&
            !'$error'.contains('SocketException') &&
            !'$error'.contains('errno') &&
            !'$error'.contains('\n'),
        'raw or misleading connection error: $error',
      );
    }
    try {
      await probeHub(client(refusedUrl, token), refusedUrl);
      throw StateError('probe accepted a closed port');
    } on HubProbeException catch (error) {
      check(
        '$error'.contains(Uri.parse(refusedUrl).authority),
        'missing destination: $error',
      );
      check(
        !'$error'.contains('default'),
        'explicit port got missing-port guidance: $error',
      );
    }
    const missingPortUrl = 'http://127.0.0.2';
    try {
      await probeHub(client(missingPortUrl, token), missingPortUrl);
      throw StateError('probe accepted a missing hub port');
    } on HubProbeException catch (error) {
      check(
        '$error'.contains('127.0.0.2:80') && '$error'.contains(':7470'),
        'missing effective/default port: $error',
      );
    }

    final stalled = await io.HttpServer.bind(
      io.InternetAddress.loopbackIPv4,
      0,
    );
    final stalledUrl = 'http://127.0.0.1:${stalled.port}';
    final subscription = stalled.listen((request) {});
    try {
      await probeHub(
        client(stalledUrl, token),
        stalledUrl,
        timeout: const Duration(milliseconds: 250),
      );
      throw StateError('probe accepted a stalled server');
    } on HubProbeException catch (error) {
      check(
        '$error'.contains(Uri.parse(stalledUrl).authority),
        'timeout lost destination: $error',
      );
    } finally {
      await stalled.close(force: true);
      await subscription.cancel();
    }
    io.stdout.writeln(
      'Hub probe, token refusal, destination errors and timeout passed.',
    );
  } finally {
    http.close(force: true);
  }
}
