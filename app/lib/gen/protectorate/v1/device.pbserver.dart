// This is a generated file - do not edit.
//
// Generated from protectorate/v1/device.proto.

// @dart = 3.3

// ignore_for_file: annotate_overrides, camel_case_types, comment_references
// ignore_for_file: constant_identifier_names
// ignore_for_file: curly_braces_in_flow_control_structures
// ignore_for_file: deprecated_member_use_from_same_package, library_prefixes
// ignore_for_file: non_constant_identifier_names, prefer_relative_imports

import 'dart:async' as $async;
import 'dart:core' as $core;

import 'package:protobuf/protobuf.dart' as $pb;

import 'device.pb.dart' as $1;
import 'device.pbjson.dart';

export 'device.pb.dart';

abstract class DeviceServiceBase extends $pb.GeneratedService {
  $async.Future<$1.AddDeviceResponse> addDevice(
      $pb.ServerContext ctx, $1.AddDeviceRequest request);
  $async.Future<$1.ListDevicesResponse> listDevices(
      $pb.ServerContext ctx, $1.ListDevicesRequest request);
  $async.Future<$1.RevokeDeviceResponse> revokeDevice(
      $pb.ServerContext ctx, $1.RevokeDeviceRequest request);
  $async.Future<$1.RenameDeviceResponse> renameDevice(
      $pb.ServerContext ctx, $1.RenameDeviceRequest request);

  $pb.GeneratedMessage createRequest($core.String methodName) {
    switch (methodName) {
      case 'AddDevice':
        return $1.AddDeviceRequest();
      case 'ListDevices':
        return $1.ListDevicesRequest();
      case 'RevokeDevice':
        return $1.RevokeDeviceRequest();
      case 'RenameDevice':
        return $1.RenameDeviceRequest();
      default:
        throw $core.ArgumentError('Unknown method: $methodName');
    }
  }

  $async.Future<$pb.GeneratedMessage> handleCall($pb.ServerContext ctx,
      $core.String methodName, $pb.GeneratedMessage request) {
    switch (methodName) {
      case 'AddDevice':
        return addDevice(ctx, request as $1.AddDeviceRequest);
      case 'ListDevices':
        return listDevices(ctx, request as $1.ListDevicesRequest);
      case 'RevokeDevice':
        return revokeDevice(ctx, request as $1.RevokeDeviceRequest);
      case 'RenameDevice':
        return renameDevice(ctx, request as $1.RenameDeviceRequest);
      default:
        throw $core.ArgumentError('Unknown method: $methodName');
    }
  }

  $core.Map<$core.String, $core.dynamic> get $json => DeviceServiceBase$json;
  $core.Map<$core.String, $core.Map<$core.String, $core.dynamic>>
      get $messageJson => DeviceServiceBase$messageJson;
}
