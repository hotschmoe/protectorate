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

import 'message.pb.dart' as $1;
import 'message.pbjson.dart';

export 'message.pb.dart';

abstract class MessageServiceBase extends $pb.GeneratedService {
  $async.Future<$1.MessageServiceSendResponse> send(
      $pb.ServerContext ctx, $1.MessageServiceSendRequest request);
  $async.Future<$1.InboxResponse> inbox(
      $pb.ServerContext ctx, $1.InboxRequest request);
  $async.Future<$1.AckResponse> ack(
      $pb.ServerContext ctx, $1.AckRequest request);
  $async.Future<$1.ReplyResponse> reply(
      $pb.ServerContext ctx, $1.ReplyRequest request);
  $async.Future<$1.ThreadResponse> thread(
      $pb.ServerContext ctx, $1.ThreadRequest request);
  $async.Future<$1.ListThreadsResponse> listThreads(
      $pb.ServerContext ctx, $1.ListThreadsRequest request);

  $pb.GeneratedMessage createRequest($core.String methodName) {
    switch (methodName) {
      case 'Send':
        return $1.MessageServiceSendRequest();
      case 'Inbox':
        return $1.InboxRequest();
      case 'Ack':
        return $1.AckRequest();
      case 'Reply':
        return $1.ReplyRequest();
      case 'Thread':
        return $1.ThreadRequest();
      case 'ListThreads':
        return $1.ListThreadsRequest();
      default:
        throw $core.ArgumentError('Unknown method: $methodName');
    }
  }

  $async.Future<$pb.GeneratedMessage> handleCall($pb.ServerContext ctx,
      $core.String methodName, $pb.GeneratedMessage request) {
    switch (methodName) {
      case 'Send':
        return send(ctx, request as $1.MessageServiceSendRequest);
      case 'Inbox':
        return inbox(ctx, request as $1.InboxRequest);
      case 'Ack':
        return ack(ctx, request as $1.AckRequest);
      case 'Reply':
        return reply(ctx, request as $1.ReplyRequest);
      case 'Thread':
        return thread(ctx, request as $1.ThreadRequest);
      case 'ListThreads':
        return listThreads(ctx, request as $1.ListThreadsRequest);
      default:
        throw $core.ArgumentError('Unknown method: $methodName');
    }
  }

  $core.Map<$core.String, $core.dynamic> get $json => MessageServiceBase$json;
  $core.Map<$core.String, $core.Map<$core.String, $core.dynamic>>
      get $messageJson => MessageServiceBase$messageJson;
}
