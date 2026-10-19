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

import 'desk.pb.dart' as $2;
import 'desk.pbjson.dart';

export 'desk.pb.dart';

abstract class DeskServiceBase extends $pb.GeneratedService {
  $async.Future<$2.ScreenshotResponse> screenshot(
      $pb.ServerContext ctx, $2.ScreenshotRequest request);
  $async.Future<$2.ListWindowsResponse> listWindows(
      $pb.ServerContext ctx, $2.ListWindowsRequest request);
  $async.Future<$2.ForegroundResponse> foreground(
      $pb.ServerContext ctx, $2.ForegroundRequest request);
  $async.Future<$2.InputResponse> click(
      $pb.ServerContext ctx, $2.ClickRequest request);
  $async.Future<$2.InputResponse> move(
      $pb.ServerContext ctx, $2.MoveRequest request);
  $async.Future<$2.InputResponse> type(
      $pb.ServerContext ctx, $2.TypeRequest request);
  $async.Future<$2.InputResponse> key(
      $pb.ServerContext ctx, $2.KeyRequest request);
  $async.Future<$2.RunResponse> run(
      $pb.ServerContext ctx, $2.RunRequest request);

  $pb.GeneratedMessage createRequest($core.String methodName) {
    switch (methodName) {
      case 'Screenshot':
        return $2.ScreenshotRequest();
      case 'ListWindows':
        return $2.ListWindowsRequest();
      case 'Foreground':
        return $2.ForegroundRequest();
      case 'Click':
        return $2.ClickRequest();
      case 'Move':
        return $2.MoveRequest();
      case 'Type':
        return $2.TypeRequest();
      case 'Key':
        return $2.KeyRequest();
      case 'Run':
        return $2.RunRequest();
      default:
        throw $core.ArgumentError('Unknown method: $methodName');
    }
  }

  $async.Future<$pb.GeneratedMessage> handleCall($pb.ServerContext ctx,
      $core.String methodName, $pb.GeneratedMessage request) {
    switch (methodName) {
      case 'Screenshot':
        return screenshot(ctx, request as $2.ScreenshotRequest);
      case 'ListWindows':
        return listWindows(ctx, request as $2.ListWindowsRequest);
      case 'Foreground':
        return foreground(ctx, request as $2.ForegroundRequest);
      case 'Click':
        return click(ctx, request as $2.ClickRequest);
      case 'Move':
        return move(ctx, request as $2.MoveRequest);
      case 'Type':
        return type(ctx, request as $2.TypeRequest);
      case 'Key':
        return key(ctx, request as $2.KeyRequest);
      case 'Run':
        return run(ctx, request as $2.RunRequest);
      default:
        throw $core.ArgumentError('Unknown method: $methodName');
    }
  }

  $core.Map<$core.String, $core.dynamic> get $json => DeskServiceBase$json;
  $core.Map<$core.String, $core.Map<$core.String, $core.dynamic>>
      get $messageJson => DeskServiceBase$messageJson;
}
