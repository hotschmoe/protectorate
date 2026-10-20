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

import 'device.pb.dart' as $2;
import 'device.pbjson.dart';

export 'device.pb.dart';

abstract class DeviceServiceBase extends $pb.GeneratedService {
  $async.Future<$2.StartDeviceAuthResponse> startDeviceAuth(
      $pb.ServerContext ctx, $2.StartDeviceAuthRequest request);
  $async.Future<$2.PollDeviceAuthResponse> pollDeviceAuth(
      $pb.ServerContext ctx, $2.PollDeviceAuthRequest request);
  $async.Future<$2.ListDeviceAuthResponse> listDeviceAuth(
      $pb.ServerContext ctx, $2.ListDeviceAuthRequest request);
  $async.Future<$2.ApproveDeviceAuthResponse> approveDeviceAuth(
      $pb.ServerContext ctx, $2.ApproveDeviceAuthRequest request);
  $async.Future<$2.DenyDeviceAuthResponse> denyDeviceAuth(
      $pb.ServerContext ctx, $2.DenyDeviceAuthRequest request);
  $async.Future<$2.ListDevicesResponse> listDevices(
      $pb.ServerContext ctx, $2.ListDevicesRequest request);
  $async.Future<$2.RevokeDeviceResponse> revokeDevice(
      $pb.ServerContext ctx, $2.RevokeDeviceRequest request);
  $async.Future<$2.RenameDeviceResponse> renameDevice(
      $pb.ServerContext ctx, $2.RenameDeviceRequest request);

  $pb.GeneratedMessage createRequest($core.String methodName) {
    switch (methodName) {
      case 'StartDeviceAuth':
        return $2.StartDeviceAuthRequest();
      case 'PollDeviceAuth':
        return $2.PollDeviceAuthRequest();
      case 'ListDeviceAuth':
        return $2.ListDeviceAuthRequest();
      case 'ApproveDeviceAuth':
        return $2.ApproveDeviceAuthRequest();
      case 'DenyDeviceAuth':
        return $2.DenyDeviceAuthRequest();
      case 'ListDevices':
        return $2.ListDevicesRequest();
      case 'RevokeDevice':
        return $2.RevokeDeviceRequest();
      case 'RenameDevice':
        return $2.RenameDeviceRequest();
      default:
        throw $core.ArgumentError('Unknown method: $methodName');
    }
  }

  $async.Future<$pb.GeneratedMessage> handleCall($pb.ServerContext ctx,
      $core.String methodName, $pb.GeneratedMessage request) {
    switch (methodName) {
      case 'StartDeviceAuth':
        return startDeviceAuth(ctx, request as $2.StartDeviceAuthRequest);
      case 'PollDeviceAuth':
        return pollDeviceAuth(ctx, request as $2.PollDeviceAuthRequest);
      case 'ListDeviceAuth':
        return listDeviceAuth(ctx, request as $2.ListDeviceAuthRequest);
      case 'ApproveDeviceAuth':
        return approveDeviceAuth(ctx, request as $2.ApproveDeviceAuthRequest);
      case 'DenyDeviceAuth':
        return denyDeviceAuth(ctx, request as $2.DenyDeviceAuthRequest);
      case 'ListDevices':
        return listDevices(ctx, request as $2.ListDevicesRequest);
      case 'RevokeDevice':
        return revokeDevice(ctx, request as $2.RevokeDeviceRequest);
      case 'RenameDevice':
        return renameDevice(ctx, request as $2.RenameDeviceRequest);
      default:
        throw $core.ArgumentError('Unknown method: $methodName');
    }
  }

  $core.Map<$core.String, $core.dynamic> get $json => DeviceServiceBase$json;
  $core.Map<$core.String, $core.Map<$core.String, $core.dynamic>>
      get $messageJson => DeviceServiceBase$messageJson;
}
