// This is a generated file - do not edit.
//
// Generated from protectorate/v1/desk.proto.

// @dart = 3.3

// ignore_for_file: annotate_overrides, camel_case_types, comment_references
// ignore_for_file: constant_identifier_names
// ignore_for_file: curly_braces_in_flow_control_structures
// ignore_for_file: deprecated_member_use_from_same_package, library_prefixes
// ignore_for_file: non_constant_identifier_names, prefer_relative_imports

import 'dart:async' as $async;
import 'dart:core' as $core;

import 'package:protobuf/protobuf.dart' as $pb;

import 'desk.pb.dart' as $0;
import 'desk.pbjson.dart';

export 'desk.pb.dart';

abstract class DeskServiceBase extends $pb.GeneratedService {
  $async.Future<$0.ScreenshotResponse> screenshot(
      $pb.ServerContext ctx, $0.ScreenshotRequest request);
  $async.Future<$0.ListWindowsResponse> listWindows(
      $pb.ServerContext ctx, $0.ListWindowsRequest request);
  $async.Future<$0.InputResponse> click(
      $pb.ServerContext ctx, $0.ClickRequest request);
  $async.Future<$0.InputResponse> move(
      $pb.ServerContext ctx, $0.MoveRequest request);
  $async.Future<$0.InputResponse> type(
      $pb.ServerContext ctx, $0.TypeRequest request);
  $async.Future<$0.InputResponse> key(
      $pb.ServerContext ctx, $0.KeyRequest request);
  $async.Future<$0.RunResponse> run(
      $pb.ServerContext ctx, $0.RunRequest request);

  $pb.GeneratedMessage createRequest($core.String methodName) {
    switch (methodName) {
      case 'Screenshot':
        return $0.ScreenshotRequest();
      case 'ListWindows':
        return $0.ListWindowsRequest();
      case 'Click':
        return $0.ClickRequest();
      case 'Move':
        return $0.MoveRequest();
      case 'Type':
        return $0.TypeRequest();
      case 'Key':
        return $0.KeyRequest();
      case 'Run':
        return $0.RunRequest();
      default:
        throw $core.ArgumentError('Unknown method: $methodName');
    }
  }

  $async.Future<$pb.GeneratedMessage> handleCall($pb.ServerContext ctx,
      $core.String methodName, $pb.GeneratedMessage request) {
    switch (methodName) {
      case 'Screenshot':
        return screenshot(ctx, request as $0.ScreenshotRequest);
      case 'ListWindows':
        return listWindows(ctx, request as $0.ListWindowsRequest);
      case 'Click':
        return click(ctx, request as $0.ClickRequest);
      case 'Move':
        return move(ctx, request as $0.MoveRequest);
      case 'Type':
        return type(ctx, request as $0.TypeRequest);
      case 'Key':
        return key(ctx, request as $0.KeyRequest);
      case 'Run':
        return run(ctx, request as $0.RunRequest);
      default:
        throw $core.ArgumentError('Unknown method: $methodName');
    }
  }

  $core.Map<$core.String, $core.dynamic> get $json => DeskServiceBase$json;
  $core.Map<$core.String, $core.Map<$core.String, $core.dynamic>>
      get $messageJson => DeskServiceBase$messageJson;
}
