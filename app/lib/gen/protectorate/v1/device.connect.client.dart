//
//  Generated code. Do not modify.
//  source: protectorate/v1/device.proto
//

import "package:connectrpc/connect.dart" as connect;
import "device.pb.dart" as protectoratev1device;
import "device.connect.spec.dart" as specs;

/// DeviceService connects new clients through approval by a trusted device.
extension type DeviceServiceClient (connect.Transport _transport) {
  /// StartDeviceAuth is unauthenticated and rate limited on the LAN.
  Future<protectoratev1device.StartDeviceAuthResponse> startDeviceAuth(
    protectoratev1device.StartDeviceAuthRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.DeviceService.startDeviceAuth,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  /// PollDeviceAuth delivers a fresh token only once after approval.
  Future<protectoratev1device.PollDeviceAuthResponse> pollDeviceAuth(
    protectoratev1device.PollDeviceAuthRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.DeviceService.pollDeviceAuth,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  Future<protectoratev1device.ListDeviceAuthResponse> listDeviceAuth(
    protectoratev1device.ListDeviceAuthRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.DeviceService.listDeviceAuth,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  Future<protectoratev1device.ApproveDeviceAuthResponse> approveDeviceAuth(
    protectoratev1device.ApproveDeviceAuthRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.DeviceService.approveDeviceAuth,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  Future<protectoratev1device.DenyDeviceAuthResponse> denyDeviceAuth(
    protectoratev1device.DenyDeviceAuthRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.DeviceService.denyDeviceAuth,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  Future<protectoratev1device.ListDevicesResponse> listDevices(
    protectoratev1device.ListDevicesRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.DeviceService.listDevices,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  Future<protectoratev1device.RevokeDeviceResponse> revokeDevice(
    protectoratev1device.RevokeDeviceRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.DeviceService.revokeDevice,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  Future<protectoratev1device.RenameDeviceResponse> renameDevice(
    protectoratev1device.RenameDeviceRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.DeviceService.renameDevice,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }
}
