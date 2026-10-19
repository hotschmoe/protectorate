// This is a generated file - do not edit.
//
// Generated from protectorate/v1/session.proto.

// @dart = 3.3

// ignore_for_file: annotate_overrides, camel_case_types, comment_references
// ignore_for_file: constant_identifier_names
// ignore_for_file: curly_braces_in_flow_control_structures
// ignore_for_file: deprecated_member_use_from_same_package, library_prefixes
// ignore_for_file: non_constant_identifier_names, prefer_relative_imports

import 'dart:async' as $async;
import 'dart:core' as $core;

import 'package:protobuf/protobuf.dart' as $pb;

import 'session.pb.dart' as $1;
import 'session.pbjson.dart';

export 'session.pb.dart';

abstract class SessionServiceBase extends $pb.GeneratedService {
  $async.Future<$1.ListSessionsResponse> listSessions(
      $pb.ServerContext ctx, $1.ListSessionsRequest request);
  $async.Future<$1.CreateSessionResponse> createSession(
      $pb.ServerContext ctx, $1.CreateSessionRequest request);
  $async.Future<$1.KillSessionResponse> killSession(
      $pb.ServerContext ctx, $1.KillSessionRequest request);
  $async.Future<$1.CapturePaneResponse> capturePane(
      $pb.ServerContext ctx, $1.CapturePaneRequest request);

  $pb.GeneratedMessage createRequest($core.String methodName) {
    switch (methodName) {
      case 'ListSessions':
        return $1.ListSessionsRequest();
      case 'CreateSession':
        return $1.CreateSessionRequest();
      case 'KillSession':
        return $1.KillSessionRequest();
      case 'CapturePane':
        return $1.CapturePaneRequest();
      default:
        throw $core.ArgumentError('Unknown method: $methodName');
    }
  }

  $async.Future<$pb.GeneratedMessage> handleCall($pb.ServerContext ctx,
      $core.String methodName, $pb.GeneratedMessage request) {
    switch (methodName) {
      case 'ListSessions':
        return listSessions(ctx, request as $1.ListSessionsRequest);
      case 'CreateSession':
        return createSession(ctx, request as $1.CreateSessionRequest);
      case 'KillSession':
        return killSession(ctx, request as $1.KillSessionRequest);
      case 'CapturePane':
        return capturePane(ctx, request as $1.CapturePaneRequest);
      default:
        throw $core.ArgumentError('Unknown method: $methodName');
    }
  }

  $core.Map<$core.String, $core.dynamic> get $json => SessionServiceBase$json;
  $core.Map<$core.String, $core.Map<$core.String, $core.dynamic>>
      get $messageJson => SessionServiceBase$messageJson;
}
