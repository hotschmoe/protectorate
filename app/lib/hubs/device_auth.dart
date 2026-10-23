import 'dart:async';

import 'package:connectrpc/connect.dart';

import '../gen/protectorate/v1/device.connect.client.dart';
import '../gen/protectorate/v1/device.pb.dart';

class DeviceAuthException implements Exception {
  const DeviceAuthException(this.status);

  final String status;

  @override
  String toString() => switch (status) {
    'denied' => 'Access was denied. Start again if this was a mistake.',
    'expired' => 'This code expired. Try again for a new code.',
    'consumed' => 'This code has already been used. Connect again.',
    _ => 'The hub returned an unexpected device authorization response.',
  };
}

/// Registers `publicKey` with the hub and waits for a trusted device to
/// approve it. Returns the approved device.
Future<DeviceInfo> authorizeDevice(
  DeviceServiceClient client, {
  required String name,
  required String platform,
  required List<int> publicKey,
  required AbortSignal signal,
  required void Function(StartDeviceAuthResponse) onStarted,
}) async {
  final request = await client.startDeviceAuth(
    StartDeviceAuthRequest(
      name: name,
      platform: platform,
      publicKey: publicKey,
    ),
    signal: TimeoutSignal(const Duration(seconds: 8), parent: signal),
  );
  onStarted(request);
  var interval = request.interval.clamp(1, 3600);
  final expires = request.expiresAt.toDateTime();
  while (true) {
    final remaining = expires.difference(DateTime.now());
    if (remaining <= Duration.zero) {
      throw const DeviceAuthException('expired');
    }
    final delay = Duration(seconds: interval);
    await Future.any<void>([
      Future<void>.delayed(delay < remaining ? delay : remaining),
      signal.future.then<void>((error) => throw error),
    ]);
    if (!DateTime.now().isBefore(expires)) {
      throw const DeviceAuthException('expired');
    }
    final PollDeviceAuthResponse response;
    try {
      response = await client.pollDeviceAuth(
        PollDeviceAuthRequest(deviceCode: request.deviceCode),
        signal: TimeoutSignal(const Duration(seconds: 8), parent: signal),
      );
    } on ConnectException catch (error) {
      if (error.code != Code.resourceExhausted) rethrow;
      interval = interval < 60 ? 60 : interval;
      continue;
    }
    switch (response.status) {
      case 'pending':
        interval = response.interval > interval ? response.interval : interval;
      case 'slow_down':
        interval = response.interval > interval
            ? response.interval
            : interval + 5;
      case 'approved':
        if (!response.hasDevice() || response.device.id.isEmpty) {
          throw const DeviceAuthException('invalid');
        }
        return response.device;
      default:
        throw DeviceAuthException(response.status);
    }
  }
}
