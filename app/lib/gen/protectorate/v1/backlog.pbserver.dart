// This is a generated file - do not edit.
//
// Generated from protectorate/v1/backlog.proto.

// @dart = 3.3

// ignore_for_file: annotate_overrides, camel_case_types, comment_references
// ignore_for_file: constant_identifier_names
// ignore_for_file: curly_braces_in_flow_control_structures
// ignore_for_file: deprecated_member_use_from_same_package, library_prefixes
// ignore_for_file: non_constant_identifier_names, prefer_relative_imports

import 'dart:async' as $async;
import 'dart:core' as $core;

import 'package:protobuf/protobuf.dart' as $pb;

import 'backlog.pb.dart' as $1;
import 'backlog.pbjson.dart';

export 'backlog.pb.dart';

abstract class BacklogServiceBase extends $pb.GeneratedService {
  $async.Future<$1.ListItemsResponse> listItems(
      $pb.ServerContext ctx, $1.ListItemsRequest request);
  $async.Future<$1.AddItemResponse> addItem(
      $pb.ServerContext ctx, $1.AddItemRequest request);
  $async.Future<$1.UpdateItemResponse> updateItem(
      $pb.ServerContext ctx, $1.UpdateItemRequest request);

  $pb.GeneratedMessage createRequest($core.String methodName) {
    switch (methodName) {
      case 'ListItems':
        return $1.ListItemsRequest();
      case 'AddItem':
        return $1.AddItemRequest();
      case 'UpdateItem':
        return $1.UpdateItemRequest();
      default:
        throw $core.ArgumentError('Unknown method: $methodName');
    }
  }

  $async.Future<$pb.GeneratedMessage> handleCall($pb.ServerContext ctx,
      $core.String methodName, $pb.GeneratedMessage request) {
    switch (methodName) {
      case 'ListItems':
        return listItems(ctx, request as $1.ListItemsRequest);
      case 'AddItem':
        return addItem(ctx, request as $1.AddItemRequest);
      case 'UpdateItem':
        return updateItem(ctx, request as $1.UpdateItemRequest);
      default:
        throw $core.ArgumentError('Unknown method: $methodName');
    }
  }

  $core.Map<$core.String, $core.dynamic> get $json => BacklogServiceBase$json;
  $core.Map<$core.String, $core.Map<$core.String, $core.dynamic>>
      get $messageJson => BacklogServiceBase$messageJson;
}
