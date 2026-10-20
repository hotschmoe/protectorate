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

import 'sleeve.pb.dart' as $4;
import 'sleeve.pbjson.dart';

export 'sleeve.pb.dart';

abstract class SleeveServiceBase extends $pb.GeneratedService {
  $async.Future<$4.SpawnSleeveResponse> spawnSleeve(
      $pb.ServerContext ctx, $4.SpawnSleeveRequest request);
  $async.Future<$4.ListSleevesResponse> listSleeves(
      $pb.ServerContext ctx, $4.ListSleevesRequest request);
  $async.Future<$4.ListWorkspacesResponse> listWorkspaces(
      $pb.ServerContext ctx, $4.ListWorkspacesRequest request);
  $async.Future<$4.BrowseWorkspacesResponse> browseWorkspaces(
      $pb.ServerContext ctx, $4.BrowseWorkspacesRequest request);
  $async.Future<$4.GetSleeveResponse> getSleeve(
      $pb.ServerContext ctx, $4.GetSleeveRequest request);
  $async.Future<$4.KillSleeveResponse> killSleeve(
      $pb.ServerContext ctx, $4.KillSleeveRequest request);
  $async.Future<$4.ResleeveSleeveResponse> resleeveSleeve(
      $pb.ServerContext ctx, $4.ResleeveSleeveRequest request);
  $async.Future<$4.SpawnEnvoyResponse> spawnEnvoy(
      $pb.ServerContext ctx, $4.SpawnEnvoyRequest request);
  $async.Future<$4.PruneSleevesResponse> pruneSleeves(
      $pb.ServerContext ctx, $4.PruneSleevesRequest request);

  $pb.GeneratedMessage createRequest($core.String methodName) {
    switch (methodName) {
      case 'SpawnSleeve':
        return $4.SpawnSleeveRequest();
      case 'ListSleeves':
        return $4.ListSleevesRequest();
      case 'ListWorkspaces':
        return $4.ListWorkspacesRequest();
      case 'BrowseWorkspaces':
        return $4.BrowseWorkspacesRequest();
      case 'GetSleeve':
        return $4.GetSleeveRequest();
      case 'KillSleeve':
        return $4.KillSleeveRequest();
      case 'ResleeveSleeve':
        return $4.ResleeveSleeveRequest();
      case 'SpawnEnvoy':
        return $4.SpawnEnvoyRequest();
      case 'PruneSleeves':
        return $4.PruneSleevesRequest();
      default:
        throw $core.ArgumentError('Unknown method: $methodName');
    }
  }

  $async.Future<$pb.GeneratedMessage> handleCall($pb.ServerContext ctx,
      $core.String methodName, $pb.GeneratedMessage request) {
    switch (methodName) {
      case 'SpawnSleeve':
        return spawnSleeve(ctx, request as $4.SpawnSleeveRequest);
      case 'ListSleeves':
        return listSleeves(ctx, request as $4.ListSleevesRequest);
      case 'ListWorkspaces':
        return listWorkspaces(ctx, request as $4.ListWorkspacesRequest);
      case 'BrowseWorkspaces':
        return browseWorkspaces(ctx, request as $4.BrowseWorkspacesRequest);
      case 'GetSleeve':
        return getSleeve(ctx, request as $4.GetSleeveRequest);
      case 'KillSleeve':
        return killSleeve(ctx, request as $4.KillSleeveRequest);
      case 'ResleeveSleeve':
        return resleeveSleeve(ctx, request as $4.ResleeveSleeveRequest);
      case 'SpawnEnvoy':
        return spawnEnvoy(ctx, request as $4.SpawnEnvoyRequest);
      case 'PruneSleeves':
        return pruneSleeves(ctx, request as $4.PruneSleevesRequest);
      default:
        throw $core.ArgumentError('Unknown method: $methodName');
    }
  }

  $core.Map<$core.String, $core.dynamic> get $json => SleeveServiceBase$json;
  $core.Map<$core.String, $core.Map<$core.String, $core.dynamic>>
      get $messageJson => SleeveServiceBase$messageJson;
}
