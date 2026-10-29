import 'dart:async';
import 'dart:io' as io;

import 'package:connectrpc/connect.dart';
import 'package:connectrpc/io.dart' as rpcio;
import 'package:connectrpc/protobuf.dart';
import 'package:connectrpc/protocol/connect.dart' as protocol;

import 'package:protectorate/gen/protectorate/v1/hub.connect.client.dart';
import 'package:protectorate/gen/protectorate/v1/hub.pb.dart';
import 'package:protectorate/gen/protectorate/v1/device.connect.client.dart';
import 'package:protectorate/gen/protectorate/v1/device.pb.dart';
import 'package:protectorate/gen/protectorate/v1/events.connect.client.dart';
import 'package:protectorate/gen/protectorate/v1/events.pb.dart';
import 'package:protectorate/hubs/connection.dart';

Future<void> main(List<String> args) async {
  final baseUrl = args[0];
  final token = args[1];
  final http = io.HttpClient()..connectionTimeout = const Duration(seconds: 8);
  try {
    protocol.Transport transport(String url, String bearer) =>
        protocol.Transport(
          baseUrl: url,
          codec: const JsonCodec(),
          httpClient: hubHttpClient(url, client: rpcio.createHttpClient(http)),
          interceptors: [bearerInterceptor(bearer)],
        );
    HubServiceClient client(String url, String bearer) =>
        HubServiceClient(transport(url, bearer));
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

    final owner = DeviceServiceClient(transport(baseUrl, token));
    final added = await owner.addDevice(AddDeviceRequest(name: 'Dart desktop'));
    final first = added.token;
    check(
      RegExp(r'^pd1\.[A-Za-z0-9_-]{43}$').hasMatch(first),
      'device token has the wrong shape: ${first.split('.').first}',
    );
    check(
      added.device.id == tokenDeviceId(first),
      'hub derived another device id: ${added.device.id}',
    );
    check(
      added.device.name == 'Dart desktop' &&
          added.device.hasAddedAt() &&
          !added.device.hasLastUsedAt(),
      'new device record is wrong: ${added.device}',
    );
    await probeHub(client(baseUrl, first), baseUrl);

    final device = DeviceServiceClient(transport(baseUrl, first));
    final second = await device.addDevice(AddDeviceRequest(name: 'Dart phone'));
    check(second.token != first, 'the hub minted the same token twice');
    await device.renameDevice(
      RenameDeviceRequest(id: second.device.id, name: 'Dart tablet'),
    );
    final listed = await device.listDevices(ListDevicesRequest());
    final self = listed.devices.where((d) => d.id == added.device.id);
    check(
      self.length == 1 && self.single.hasLastUsedAt(),
      'the device that called is missing or unstamped: ${listed.devices}',
    );
    check(
      listed.devices.any(
        (d) => d.id == second.device.id && d.name == 'Dart tablet',
      ),
      'renamed device missing from Devices: ${listed.devices}',
    );
    try {
      await device.addDevice(AddDeviceRequest(name: 'envoy'));
      throw StateError('the hub added a device named envoy');
    } on ConnectException catch (error) {
      check(error.code == Code.invalidArgument, 'reserved name: $error');
    }

    // Revoking closes an open stream and the token stops working.
    final events = EventServiceClient(transport(baseUrl, second.token));
    final abort = CancelableSignal();
    final opened = Completer<void>();
    final ended = Completer<Object?>();
    final stream = events
        .subscribe(SubscribeRequest(), signal: abort)
        .listen(
          (_) {
            if (!opened.isCompleted) opened.complete();
          },
          onError: (Object error) {
            if (!ended.isCompleted) ended.complete(error);
          },
          onDone: () {
            if (!ended.isCompleted) ended.complete(null);
          },
        );
    try {
      // The hub sends a heartbeat as soon as a stream opens.
      await opened.future.timeout(const Duration(seconds: 5));
      await device.revokeDevice(RevokeDeviceRequest(id: second.device.id));
      final error = await ended.future.timeout(
        const Duration(seconds: 3),
        onTimeout: () =>
            throw StateError('revoking left the event stream open'),
      );
      check(
        error is ConnectException && error.code == Code.unauthenticated,
        'revoked stream ended without unauthenticated: $error',
      );
    } finally {
      abort.cancel();
      await stream.cancel();
    }
    await owner.revokeDevice(RevokeDeviceRequest(id: added.device.id));
    for (final (name, revoked) in [
      ('second', second.token),
      ('first', first),
    ]) {
      try {
        await client(baseUrl, revoked).getStatus(GetStatusRequest());
        throw StateError('the hub admitted the revoked $name device');
      } on ConnectException catch (error) {
        check(
          error.code == Code.unauthenticated,
          'revoked $name device: $error',
        );
      }
    }
    check(
      (await owner.listDevices(ListDevicesRequest())).devices.isEmpty,
      'revoked devices are still listed',
    );
    io.stdout.writeln(
      'Hub probe, token refusal, destination errors, timeout and Dart device add, bearer calls, rename, reserved names, revoke closing a stream and revoked tokens passed.',
    );
  } finally {
    http.close(force: true);
  }
}
