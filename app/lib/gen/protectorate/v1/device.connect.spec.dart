//
//  Generated code. Do not modify.
//  source: protectorate/v1/device.proto
//

import "package:connectrpc/connect.dart" as connect;
import "device.pb.dart" as protectoratev1device;

/// DeviceService connects new clients through approval by a trusted device.
abstract final class DeviceService {
  /// Fully-qualified name of the DeviceService service.
  static const name = 'protectorate.v1.DeviceService';

  /// StartDeviceAuth is unauthenticated and rate limited on the LAN.
  static const startDeviceAuth = connect.Spec(
    '/$name/StartDeviceAuth',
    connect.StreamType.unary,
    protectoratev1device.StartDeviceAuthRequest.new,
    protectoratev1device.StartDeviceAuthResponse.new,
  );

  /// PollDeviceAuth delivers a fresh token only once after approval.
  static const pollDeviceAuth = connect.Spec(
    '/$name/PollDeviceAuth',
    connect.StreamType.unary,
    protectoratev1device.PollDeviceAuthRequest.new,
    protectoratev1device.PollDeviceAuthResponse.new,
  );

  static const listDeviceAuth = connect.Spec(
    '/$name/ListDeviceAuth',
    connect.StreamType.unary,
    protectoratev1device.ListDeviceAuthRequest.new,
    protectoratev1device.ListDeviceAuthResponse.new,
    idempotency: connect.Idempotency.noSideEffects,
  );

  static const approveDeviceAuth = connect.Spec(
    '/$name/ApproveDeviceAuth',
    connect.StreamType.unary,
    protectoratev1device.ApproveDeviceAuthRequest.new,
    protectoratev1device.ApproveDeviceAuthResponse.new,
  );

  static const denyDeviceAuth = connect.Spec(
    '/$name/DenyDeviceAuth',
    connect.StreamType.unary,
    protectoratev1device.DenyDeviceAuthRequest.new,
    protectoratev1device.DenyDeviceAuthResponse.new,
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
