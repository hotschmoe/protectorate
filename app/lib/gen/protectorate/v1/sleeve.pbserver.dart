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

import 'sleeve.pb.dart' as $1;
import 'sleeve.pbjson.dart';

export 'sleeve.pb.dart';

abstract class SleeveServiceBase extends $pb.GeneratedService {
  $async.Future<$1.SpawnSleeveResponse> spawnSleeve(
      $pb.ServerContext ctx, $1.SpawnSleeveRequest request);
  $async.Future<$1.ListSleevesResponse> listSleeves(
      $pb.ServerContext ctx, $1.ListSleevesRequest request);
  $async.Future<$1.GetSleeveResponse> getSleeve(
      $pb.ServerContext ctx, $1.GetSleeveRequest request);
  $async.Future<$1.KillSleeveResponse> killSleeve(
      $pb.ServerContext ctx, $1.KillSleeveRequest request);

  $pb.GeneratedMessage createRequest($core.String methodName) {
    switch (methodName) {
      case 'SpawnSleeve':
        return $1.SpawnSleeveRequest();
      case 'ListSleeves':
        return $1.ListSleevesRequest();
      case 'GetSleeve':
        return $1.GetSleeveRequest();
      case 'KillSleeve':
        return $1.KillSleeveRequest();
      default:
        throw $core.ArgumentError('Unknown method: $methodName');
    }
  }

  $async.Future<$pb.GeneratedMessage> handleCall($pb.ServerContext ctx,
      $core.String methodName, $pb.GeneratedMessage request) {
    switch (methodName) {
      case 'SpawnSleeve':
        return spawnSleeve(ctx, request as $1.SpawnSleeveRequest);
      case 'ListSleeves':
        return listSleeves(ctx, request as $1.ListSleevesRequest);
      case 'GetSleeve':
        return getSleeve(ctx, request as $1.GetSleeveRequest);
      case 'KillSleeve':
        return killSleeve(ctx, request as $1.KillSleeveRequest);
      default:
        throw $core.ArgumentError('Unknown method: $methodName');
    }
  }

  $core.Map<$core.String, $core.dynamic> get $json => SleeveServiceBase$json;
  $core.Map<$core.String, $core.Map<$core.String, $core.dynamic>>
      get $messageJson => SleeveServiceBase$messageJson;
}
