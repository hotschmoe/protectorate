// This is a generated file - do not edit.
//
// Generated from protectorate/v1/sleeve.proto.

// @dart = 3.3

// ignore_for_file: annotate_overrides, camel_case_types, comment_references
// ignore_for_file: constant_identifier_names
// ignore_for_file: curly_braces_in_flow_control_structures
// ignore_for_file: deprecated_member_use_from_same_package, library_prefixes
// ignore_for_file: non_constant_identifier_names, prefer_relative_imports

import 'dart:async' as $async;
import 'dart:core' as $core;

import 'package:protobuf/protobuf.dart' as $pb;

import 'sleeve.pb.dart' as $0;
import 'sleeve.pbjson.dart';

export 'sleeve.pb.dart';

abstract class SleeveServiceBase extends $pb.GeneratedService {
  $async.Future<$0.ListResponse> list(
      $pb.ServerContext ctx, $0.ListRequest request);
  $async.Future<$0.SpawnResponse> spawn(
      $pb.ServerContext ctx, $0.SpawnRequest request);

  $pb.GeneratedMessage createRequest($core.String methodName) {
    switch (methodName) {
      case 'List':
        return $0.ListRequest();
      case 'Spawn':
        return $0.SpawnRequest();
      default:
        throw $core.ArgumentError('Unknown method: $methodName');
    }
  }

  $async.Future<$pb.GeneratedMessage> handleCall($pb.ServerContext ctx,
      $core.String methodName, $pb.GeneratedMessage request) {
    switch (methodName) {
      case 'List':
        return list(ctx, request as $0.ListRequest);
      case 'Spawn':
        return spawn(ctx, request as $0.SpawnRequest);
      default:
        throw $core.ArgumentError('Unknown method: $methodName');
    }
  }

  $core.Map<$core.String, $core.dynamic> get $json => SleeveServiceBase$json;
  $core.Map<$core.String, $core.Map<$core.String, $core.dynamic>>
      get $messageJson => SleeveServiceBase$messageJson;
}
