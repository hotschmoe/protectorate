//
//  Generated code. Do not modify.
//  source: protectorate/v1/device.proto
//

import "package:connectrpc/connect.dart" as connect;
import "device.pb.dart" as protectoratev1device;
import "device.connect.spec.dart" as specs;

/// DeviceService manages the clients that may use the hub. Adding a device
/// returns its bearer token once; the hub keeps only a hash of it.
extension type DeviceServiceClient (connect.Transport _transport) {
  Future<protectoratev1device.AddDeviceResponse> addDevice(
    protectoratev1device.AddDeviceRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.DeviceService.addDevice,
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
