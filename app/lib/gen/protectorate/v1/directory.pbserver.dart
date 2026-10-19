// This is a generated file - do not edit.
//
// Generated from protectorate/v1/directory.proto.

// @dart = 3.3

// ignore_for_file: annotate_overrides, camel_case_types, comment_references
// ignore_for_file: constant_identifier_names
// ignore_for_file: curly_braces_in_flow_control_structures
// ignore_for_file: deprecated_member_use_from_same_package, library_prefixes
// ignore_for_file: non_constant_identifier_names, prefer_relative_imports

import 'dart:async' as $async;
import 'dart:core' as $core;

import 'package:protobuf/protobuf.dart' as $pb;

import 'directory.pb.dart' as $0;
import 'directory.pbjson.dart';

export 'directory.pb.dart';

abstract class DirectoryServiceBase extends $pb.GeneratedService {
  $async.Future<$0.ListProjectsResponse> listProjects(
      $pb.ServerContext ctx, $0.ListProjectsRequest request);
  $async.Future<$0.GetProjectResponse> getProject(
      $pb.ServerContext ctx, $0.GetProjectRequest request);

  $pb.GeneratedMessage createRequest($core.String methodName) {
    switch (methodName) {
      case 'ListProjects':
        return $0.ListProjectsRequest();
      case 'GetProject':
        return $0.GetProjectRequest();
      default:
        throw $core.ArgumentError('Unknown method: $methodName');
    }
  }

  $async.Future<$pb.GeneratedMessage> handleCall($pb.ServerContext ctx,
      $core.String methodName, $pb.GeneratedMessage request) {
    switch (methodName) {
      case 'ListProjects':
        return listProjects(ctx, request as $0.ListProjectsRequest);
      case 'GetProject':
        return getProject(ctx, request as $0.GetProjectRequest);
      default:
        throw $core.ArgumentError('Unknown method: $methodName');
    }
  }

  $core.Map<$core.String, $core.dynamic> get $json => DirectoryServiceBase$json;
  $core.Map<$core.String, $core.Map<$core.String, $core.dynamic>>
      get $messageJson => DirectoryServiceBase$messageJson;
}
