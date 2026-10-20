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

import 'sleeve.pb.dart' as $3;
import 'sleeve.pbjson.dart';

export 'sleeve.pb.dart';

abstract class SleeveServiceBase extends $pb.GeneratedService {
  $async.Future<$3.SpawnSleeveResponse> spawnSleeve(
      $pb.ServerContext ctx, $3.SpawnSleeveRequest request);
  $async.Future<$3.ListSleevesResponse> listSleeves(
      $pb.ServerContext ctx, $3.ListSleevesRequest request);
  $async.Future<$3.ListWorkspacesResponse> listWorkspaces(
      $pb.ServerContext ctx, $3.ListWorkspacesRequest request);
  $async.Future<$3.GetSleeveResponse> getSleeve(
      $pb.ServerContext ctx, $3.GetSleeveRequest request);
  $async.Future<$3.KillSleeveResponse> killSleeve(
      $pb.ServerContext ctx, $3.KillSleeveRequest request);
  $async.Future<$3.ResleeveSleeveResponse> resleeveSleeve(
      $pb.ServerContext ctx, $3.ResleeveSleeveRequest request);

  $pb.GeneratedMessage createRequest($core.String methodName) {
    switch (methodName) {
      case 'SpawnSleeve':
        return $3.SpawnSleeveRequest();
      case 'ListSleeves':
        return $3.ListSleevesRequest();
      case 'ListWorkspaces':
        return $3.ListWorkspacesRequest();
      case 'GetSleeve':
        return $3.GetSleeveRequest();
      case 'KillSleeve':
        return $3.KillSleeveRequest();
      case 'ResleeveSleeve':
        return $3.ResleeveSleeveRequest();
      default:
        throw $core.ArgumentError('Unknown method: $methodName');
    }
  }

  $async.Future<$pb.GeneratedMessage> handleCall($pb.ServerContext ctx,
      $core.String methodName, $pb.GeneratedMessage request) {
    switch (methodName) {
      case 'SpawnSleeve':
        return spawnSleeve(ctx, request as $3.SpawnSleeveRequest);
      case 'ListSleeves':
        return listSleeves(ctx, request as $3.ListSleevesRequest);
      case 'ListWorkspaces':
        return listWorkspaces(ctx, request as $3.ListWorkspacesRequest);
      case 'GetSleeve':
        return getSleeve(ctx, request as $3.GetSleeveRequest);
      case 'KillSleeve':
        return killSleeve(ctx, request as $3.KillSleeveRequest);
      case 'ResleeveSleeve':
        return resleeveSleeve(ctx, request as $3.ResleeveSleeveRequest);
      default:
        throw $core.ArgumentError('Unknown method: $methodName');
    }
  }

  $core.Map<$core.String, $core.dynamic> get $json => SleeveServiceBase$json;
  $core.Map<$core.String, $core.Map<$core.String, $core.dynamic>>
      get $messageJson => SleeveServiceBase$messageJson;
}
