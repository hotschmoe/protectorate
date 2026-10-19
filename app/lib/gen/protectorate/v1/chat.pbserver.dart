// This is a generated file - do not edit.
//
// Generated from protectorate/v1/chat.proto.

// @dart = 3.3

// ignore_for_file: annotate_overrides, camel_case_types, comment_references
// ignore_for_file: constant_identifier_names
// ignore_for_file: curly_braces_in_flow_control_structures
// ignore_for_file: deprecated_member_use_from_same_package, library_prefixes
// ignore_for_file: non_constant_identifier_names, prefer_relative_imports

import 'dart:async' as $async;
import 'dart:core' as $core;

import 'package:protobuf/protobuf.dart' as $pb;

import 'chat.pb.dart' as $1;
import 'chat.pbjson.dart';

export 'chat.pb.dart';

abstract class ChatServiceBase extends $pb.GeneratedService {
  $async.Future<$1.ChatEvent> watch(
      $pb.ServerContext ctx, $1.WatchRequest request);
  $async.Future<$1.SendResponse> send(
      $pb.ServerContext ctx, $1.SendRequest request);
  $async.Future<$1.InterruptResponse> interrupt(
      $pb.ServerContext ctx, $1.InterruptRequest request);
  $async.Future<$1.RespondResponse> respond(
      $pb.ServerContext ctx, $1.RespondRequest request);
  $async.Future<$1.CompactResponse> compact(
      $pb.ServerContext ctx, $1.CompactRequest request);
  $async.Future<$1.SetModeResponse> setMode(
      $pb.ServerContext ctx, $1.SetModeRequest request);

  $pb.GeneratedMessage createRequest($core.String methodName) {
    switch (methodName) {
      case 'Watch':
        return $1.WatchRequest();
      case 'Send':
        return $1.SendRequest();
      case 'Interrupt':
        return $1.InterruptRequest();
      case 'Respond':
        return $1.RespondRequest();
      case 'Compact':
        return $1.CompactRequest();
      case 'SetMode':
        return $1.SetModeRequest();
      default:
        throw $core.ArgumentError('Unknown method: $methodName');
    }
  }

  $async.Future<$pb.GeneratedMessage> handleCall($pb.ServerContext ctx,
      $core.String methodName, $pb.GeneratedMessage request) {
    switch (methodName) {
      case 'Watch':
        return watch(ctx, request as $1.WatchRequest);
      case 'Send':
        return send(ctx, request as $1.SendRequest);
      case 'Interrupt':
        return interrupt(ctx, request as $1.InterruptRequest);
      case 'Respond':
        return respond(ctx, request as $1.RespondRequest);
      case 'Compact':
        return compact(ctx, request as $1.CompactRequest);
      case 'SetMode':
        return setMode(ctx, request as $1.SetModeRequest);
      default:
        throw $core.ArgumentError('Unknown method: $methodName');
    }
  }

  $core.Map<$core.String, $core.dynamic> get $json => ChatServiceBase$json;
  $core.Map<$core.String, $core.Map<$core.String, $core.dynamic>>
      get $messageJson => ChatServiceBase$messageJson;
}
