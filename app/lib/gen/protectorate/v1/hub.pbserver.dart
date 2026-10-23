// This is a generated file - do not edit.
//
// Generated from protectorate/v1/hub.proto.

// @dart = 3.3

// ignore_for_file: annotate_overrides, camel_case_types, comment_references
// ignore_for_file: constant_identifier_names
// ignore_for_file: curly_braces_in_flow_control_structures
// ignore_for_file: deprecated_member_use_from_same_package, library_prefixes
// ignore_for_file: non_constant_identifier_names, prefer_relative_imports

import 'dart:async' as $async;
import 'dart:core' as $core;

import 'package:protobuf/protobuf.dart' as $pb;

import 'hub.pb.dart' as $2;
import 'hub.pbjson.dart';

export 'hub.pb.dart';

abstract class HubServiceBase extends $pb.GeneratedService {
  $async.Future<$2.GetStatusResponse> getStatus(
      $pb.ServerContext ctx, $2.GetStatusRequest request);
  $async.Future<$2.GetSleeveRuntimeResponse> getSleeveRuntime(
      $pb.ServerContext ctx, $2.GetSleeveRuntimeRequest request);

  $pb.GeneratedMessage createRequest($core.String methodName) {
    switch (methodName) {
      case 'GetStatus':
        return $2.GetStatusRequest();
      case 'GetSleeveRuntime':
        return $2.GetSleeveRuntimeRequest();
      default:
        throw $core.ArgumentError('Unknown method: $methodName');
    }
  }

  $async.Future<$pb.GeneratedMessage> handleCall($pb.ServerContext ctx,
      $core.String methodName, $pb.GeneratedMessage request) {
    switch (methodName) {
      case 'GetStatus':
        return getStatus(ctx, request as $2.GetStatusRequest);
      case 'GetSleeveRuntime':
        return getSleeveRuntime(ctx, request as $2.GetSleeveRuntimeRequest);
      default:
        throw $core.ArgumentError('Unknown method: $methodName');
    }
  }

  $core.Map<$core.String, $core.dynamic> get $json => HubServiceBase$json;
  $core.Map<$core.String, $core.Map<$core.String, $core.dynamic>>
      get $messageJson => HubServiceBase$messageJson;
}
