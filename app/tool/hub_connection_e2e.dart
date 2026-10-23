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
import 'package:protectorate/hubs/connection.dart';
import 'package:protectorate/hubs/device_auth.dart';
import 'package:protectorate/hubs/device_key.dart';
import 'package:protectorate/hubs/software_key.dart';

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
          interceptors: [
            <I extends Object, O extends Object>(AnyFn<I, O> next) =>
                (Request<I, O> request) {
                  if (bearer.isNotEmpty) {
                    request.headers.set('authorization', ['Bearer $bearer']);
                  }
                  return next(request);
                },
          ],
        );
    // The app's own signing path: a device key and its proof interceptor.
    protocol.Transport signed(String url, DeviceKey key) => protocol.Transport(
      baseUrl: url,
      codec: const JsonCodec(),
      httpClient: hubHttpClient(url, client: rpcio.createHttpClient(http)),
      interceptors: [proofInterceptor(Future.value(key))],
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

    final publicDevice = DeviceServiceClient(transport(baseUrl, ''));
    final ownerDevice = DeviceServiceClient(transport(baseUrl, token));
    final key = SoftwareKey.generate();
    final started = Completer<StartDeviceAuthResponse>();
    final approvalSignal = CancelableSignal();
    var delivered = false;
    final authorization =
        authorizeDevice(
          publicDevice,
          name: 'Dart desktop',
          platform: 'linux',
          publicKey: key.publicKey,
          signal: approvalSignal,
          onStarted: started.complete,
        ).then((device) {
          delivered = true;
          return device;
        });
    final pending = await started.future;
    await Future<void>.delayed(Duration(seconds: pending.interval + 1));
    check(!delivered, 'Dart device connected without approval');
    try {
      await HubServiceClient(
        signed(baseUrl, key),
      ).getStatus(GetStatusRequest());
      throw StateError('an unapproved key signed a call');
    } on ConnectException catch (error) {
      check(error.code == Code.unauthenticated, 'unapproved key: $error');
    }
    await ownerDevice.approveDeviceAuth(
      ApproveDeviceAuthRequest(userCode: pending.userCode),
    );
    final device = await authorization;
    approvalSignal.cancel();
    check(
      device.id == deviceId(key.publicKey),
      'hub derived another device id: ${device.id}',
    );
    await probeHub(HubServiceClient(signed(baseUrl, key)), baseUrl);
    final replay = await publicDevice.pollDeviceAuth(
      PollDeviceAuthRequest(deviceCode: pending.deviceCode),
    );
    check(
      replay.status == 'consumed' && !replay.hasDevice(),
      'Dart device approval was delivered more than once',
    );
    // The same proof twice: the hub admits it once.
    final once = await deviceProof(
      key,
      'POST',
      '/protectorate.v1.HubService/GetStatus',
    );
    final replayed = HubServiceClient(transport(baseUrl, ''));
    final headers = Headers()..add('authorization', 'Proof $once');
    await replayed.getStatus(GetStatusRequest(), headers: headers);
    try {
      await replayed.getStatus(GetStatusRequest(), headers: headers);
      throw StateError('the hub admitted a replayed proof');
    } on ConnectException catch (error) {
      check(
        error.code == Code.unauthenticated &&
            error.message.contains('replayed'),
        'replayed proof: $error',
      );
    }
    final trustedDevice = DeviceServiceClient(signed(baseUrl, key));
    final listed = await trustedDevice.listDevices(ListDevicesRequest());
    check(
      listed.devices.any(
        (device) => device.name == 'Dart desktop' && device.platform == 'linux',
      ),
      'approved Dart device missing from Devices',
    );

    final deniedStarted = Completer<StartDeviceAuthResponse>();
    final deniedSignal = CancelableSignal();
    final denial =
        authorizeDevice(
          publicDevice,
          name: 'Dart denied device',
          platform: 'web',
          publicKey: SoftwareKey.generate().publicKey,
          signal: deniedSignal,
          onStarted: deniedStarted.complete,
        ).then<String>(
          (_) => throw StateError('Dart connected a denied device'),
          onError: (Object error) {
            check(
              error is DeviceAuthException && error.status == 'denied',
              'Dart lost denial reason: $error',
            );
            return '';
          },
        );
    final denied = await deniedStarted.future;
    await trustedDevice.denyDeviceAuth(
      DenyDeviceAuthRequest(userCode: denied.userCode),
    );
    await denial;
    deniedSignal.cancel();

    final canceledStarted = Completer<StartDeviceAuthResponse>();
    final canceledSignal = CancelableSignal();
    final cancellation =
        authorizeDevice(
          publicDevice,
          name: 'Dart canceled device',
          platform: 'web',
          publicKey: SoftwareKey.generate().publicKey,
          signal: canceledSignal,
          onStarted: canceledStarted.complete,
        ).then<String>(
          (_) => throw StateError('Dart connected after cancel'),
          onError: (Object error) {
            check(
              error is ConnectException && error.code == Code.canceled,
              'Dart lost cancellation: $error',
            );
            return '';
          },
        );
    final canceled = await canceledStarted.future;
    canceledSignal.cancel();
    await cancellation.timeout(const Duration(seconds: 1));
    await trustedDevice.denyDeviceAuth(
      DenyDeviceAuthRequest(userCode: canceled.userCode),
    );
    io.stdout.writeln(
      'Hub probe, token refusal, destination errors, timeout and Dart device approval, signed calls, replay refusal, single delivery, denial and cancellation passed.',
    );
  } finally {
    http.close(force: true);
  }
}
