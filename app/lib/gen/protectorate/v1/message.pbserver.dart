// This is a generated file - do not edit.
//
// Generated from protectorate/v1/message.proto.

// @dart = 3.3

// ignore_for_file: annotate_overrides, camel_case_types, comment_references
// ignore_for_file: constant_identifier_names
// ignore_for_file: curly_braces_in_flow_control_structures
// ignore_for_file: deprecated_member_use_from_same_package, library_prefixes
// ignore_for_file: non_constant_identifier_names, prefer_relative_imports

import 'dart:async' as $async;
import 'dart:core' as $core;

import 'package:protobuf/protobuf.dart' as $pb;

import 'message.pb.dart' as $2;
import 'message.pbjson.dart';

export 'message.pb.dart';

abstract class MessageServiceBase extends $pb.GeneratedService {
  $async.Future<$2.MessageServiceWaitResponse> wait(
      $pb.ServerContext ctx, $2.MessageServiceWaitRequest request);
  $async.Future<$2.MessageServiceSendResponse> send(
      $pb.ServerContext ctx, $2.MessageServiceSendRequest request);
  $async.Future<$2.InboxResponse> inbox(
      $pb.ServerContext ctx, $2.InboxRequest request);
  $async.Future<$2.AckResponse> ack(
      $pb.ServerContext ctx, $2.AckRequest request);
  $async.Future<$2.ReplyResponse> reply(
      $pb.ServerContext ctx, $2.ReplyRequest request);
  $async.Future<$2.ThreadResponse> thread(
      $pb.ServerContext ctx, $2.ThreadRequest request);
  $async.Future<$2.ListThreadsResponse> listThreads(
      $pb.ServerContext ctx, $2.ListThreadsRequest request);

  $pb.GeneratedMessage createRequest($core.String methodName) {
    switch (methodName) {
      case 'Wait':
        return $2.MessageServiceWaitRequest();
      case 'Send':
        return $2.MessageServiceSendRequest();
      case 'Inbox':
        return $2.InboxRequest();
      case 'Ack':
        return $2.AckRequest();
      case 'Reply':
        return $2.ReplyRequest();
      case 'Thread':
        return $2.ThreadRequest();
      case 'ListThreads':
        return $2.ListThreadsRequest();
      default:
        throw $core.ArgumentError('Unknown method: $methodName');
    }
  }

  $async.Future<$pb.GeneratedMessage> handleCall($pb.ServerContext ctx,
      $core.String methodName, $pb.GeneratedMessage request) {
    switch (methodName) {
      case 'Wait':
        return wait(ctx, request as $2.MessageServiceWaitRequest);
      case 'Send':
        return send(ctx, request as $2.MessageServiceSendRequest);
      case 'Inbox':
        return inbox(ctx, request as $2.InboxRequest);
      case 'Ack':
        return ack(ctx, request as $2.AckRequest);
      case 'Reply':
        return reply(ctx, request as $2.ReplyRequest);
      case 'Thread':
        return thread(ctx, request as $2.ThreadRequest);
      case 'ListThreads':
        return listThreads(ctx, request as $2.ListThreadsRequest);
      default:
        throw $core.ArgumentError('Unknown method: $methodName');
    }
  }

  $core.Map<$core.String, $core.dynamic> get $json => MessageServiceBase$json;
  $core.Map<$core.String, $core.Map<$core.String, $core.dynamic>>
      get $messageJson => MessageServiceBase$messageJson;
}
