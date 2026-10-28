// This is a generated file - do not edit.
//
// Generated from protectorate/v1/release.proto.

// @dart = 3.3

// ignore_for_file: annotate_overrides, camel_case_types, comment_references
// ignore_for_file: constant_identifier_names
// ignore_for_file: curly_braces_in_flow_control_structures
// ignore_for_file: deprecated_member_use_from_same_package, library_prefixes
// ignore_for_file: non_constant_identifier_names, prefer_relative_imports

import 'dart:async' as $async;
import 'dart:core' as $core;

import 'package:protobuf/protobuf.dart' as $pb;

import 'release.pb.dart' as $0;
import 'release.pbjson.dart';

export 'release.pb.dart';

abstract class ReleaseServiceBase extends $pb.GeneratedService {
  $async.Future<$0.ReleaseInfo> checkRelease(
      $pb.ServerContext ctx, $0.CheckReleaseRequest request);
  $async.Future<$0.UpdateHubProgress> updateHub(
      $pb.ServerContext ctx, $0.UpdateHubRequest request);

  $pb.GeneratedMessage createRequest($core.String methodName) {
    switch (methodName) {
      case 'CheckRelease':
        return $0.CheckReleaseRequest();
      case 'UpdateHub':
        return $0.UpdateHubRequest();
      default:
        throw $core.ArgumentError('Unknown method: $methodName');
    }
  }

  $async.Future<$pb.GeneratedMessage> handleCall($pb.ServerContext ctx,
      $core.String methodName, $pb.GeneratedMessage request) {
    switch (methodName) {
      case 'CheckRelease':
        return checkRelease(ctx, request as $0.CheckReleaseRequest);
      case 'UpdateHub':
        return updateHub(ctx, request as $0.UpdateHubRequest);
      default:
        throw $core.ArgumentError('Unknown method: $methodName');
    }
  }

  $core.Map<$core.String, $core.dynamic> get $json => ReleaseServiceBase$json;
  $core.Map<$core.String, $core.Map<$core.String, $core.dynamic>>
      get $messageJson => ReleaseServiceBase$messageJson;
}
