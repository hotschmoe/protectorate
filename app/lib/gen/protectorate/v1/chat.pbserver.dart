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

import 'chat.pb.dart' as $2;
import 'chat.pbjson.dart';

export 'chat.pb.dart';

abstract class ChatServiceBase extends $pb.GeneratedService {
  $async.Future<$2.ChatEvent> watch(
      $pb.ServerContext ctx, $2.WatchRequest request);
  $async.Future<$2.SendResponse> send(
      $pb.ServerContext ctx, $2.SendRequest request);
  $async.Future<$2.InterruptResponse> interrupt(
      $pb.ServerContext ctx, $2.InterruptRequest request);
  $async.Future<$2.RespondResponse> respond(
      $pb.ServerContext ctx, $2.RespondRequest request);
  $async.Future<$2.CompactResponse> compact(
      $pb.ServerContext ctx, $2.CompactRequest request);
  $async.Future<$2.ListModelsResponse> listModels(
      $pb.ServerContext ctx, $2.ListModelsRequest request);
  $async.Future<$2.SetSelectionResponse> setSelection(
      $pb.ServerContext ctx, $2.SetSelectionRequest request);
  $async.Future<$2.ReportResponse> report(
      $pb.ServerContext ctx, $2.ReportRequest request);
  $async.Future<$2.WaitResponse> wait(
      $pb.ServerContext ctx, $2.WaitRequest request);

  $pb.GeneratedMessage createRequest($core.String methodName) {
    switch (methodName) {
      case 'Watch':
        return $2.WatchRequest();
      case 'Send':
        return $2.SendRequest();
      case 'Interrupt':
        return $2.InterruptRequest();
      case 'Respond':
        return $2.RespondRequest();
      case 'Compact':
        return $2.CompactRequest();
      case 'ListModels':
        return $2.ListModelsRequest();
      case 'SetSelection':
        return $2.SetSelectionRequest();
      case 'Report':
        return $2.ReportRequest();
      case 'Wait':
        return $2.WaitRequest();
      default:
        throw $core.ArgumentError('Unknown method: $methodName');
    }
  }

  $async.Future<$pb.GeneratedMessage> handleCall($pb.ServerContext ctx,
      $core.String methodName, $pb.GeneratedMessage request) {
    switch (methodName) {
      case 'Watch':
        return watch(ctx, request as $2.WatchRequest);
      case 'Send':
        return send(ctx, request as $2.SendRequest);
      case 'Interrupt':
        return interrupt(ctx, request as $2.InterruptRequest);
      case 'Respond':
        return respond(ctx, request as $2.RespondRequest);
      case 'Compact':
        return compact(ctx, request as $2.CompactRequest);
      case 'ListModels':
        return listModels(ctx, request as $2.ListModelsRequest);
      case 'SetSelection':
        return setSelection(ctx, request as $2.SetSelectionRequest);
      case 'Report':
        return report(ctx, request as $2.ReportRequest);
      case 'Wait':
        return wait(ctx, request as $2.WaitRequest);
      default:
        throw $core.ArgumentError('Unknown method: $methodName');
    }
  }

  $core.Map<$core.String, $core.dynamic> get $json => ChatServiceBase$json;
  $core.Map<$core.String, $core.Map<$core.String, $core.dynamic>>
      get $messageJson => ChatServiceBase$messageJson;
}
