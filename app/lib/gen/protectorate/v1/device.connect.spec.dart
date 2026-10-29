//
//  Generated code. Do not modify.
//  source: protectorate/v1/device.proto
//

import "package:connectrpc/connect.dart" as connect;
import "device.pb.dart" as protectoratev1device;

/// DeviceService manages the clients that may use the hub. Adding a device
/// returns its bearer token once; the hub keeps only a hash of it.
abstract final class DeviceService {
  /// Fully-qualified name of the DeviceService service.
  static const name = 'protectorate.v1.DeviceService';

  static const addDevice = connect.Spec(
    '/$name/AddDevice',
    connect.StreamType.unary,
    protectoratev1device.AddDeviceRequest.new,
    protectoratev1device.AddDeviceResponse.new,
  );

  static const listDevices = connect.Spec(
    '/$name/ListDevices',
    connect.StreamType.unary,
    protectoratev1device.ListDevicesRequest.new,
    protectoratev1device.ListDevicesResponse.new,
    idempotency: connect.Idempotency.noSideEffects,
  );

  static const revokeDevice = connect.Spec(
    '/$name/RevokeDevice',
    connect.StreamType.unary,
    protectoratev1device.RevokeDeviceRequest.new,
    protectoratev1device.RevokeDeviceResponse.new,
  );

  static const renameDevice = connect.Spec(
    '/$name/RenameDevice',
    connect.StreamType.unary,
    protectoratev1device.RenameDeviceRequest.new,
    protectoratev1device.RenameDeviceResponse.new,
  );
}
