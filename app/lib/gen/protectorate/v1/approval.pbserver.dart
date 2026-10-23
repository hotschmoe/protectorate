// This is a generated file - do not edit.
//
// Generated from protectorate/v1/approval.proto.

// @dart = 3.3

// ignore_for_file: annotate_overrides, camel_case_types, comment_references
// ignore_for_file: constant_identifier_names
// ignore_for_file: curly_braces_in_flow_control_structures
// ignore_for_file: deprecated_member_use_from_same_package, library_prefixes
// ignore_for_file: non_constant_identifier_names, prefer_relative_imports

import 'dart:async' as $async;
import 'dart:core' as $core;

import 'package:protobuf/protobuf.dart' as $pb;

import 'approval.pb.dart' as $1;
import 'approval.pbjson.dart';

export 'approval.pb.dart';

abstract class ApprovalServiceBase extends $pb.GeneratedService {
  $async.Future<$1.ListApprovalsResponse> listApprovals(
      $pb.ServerContext ctx, $1.ListApprovalsRequest request);
  $async.Future<$1.DecideApprovalResponse> decideApproval(
      $pb.ServerContext ctx, $1.DecideApprovalRequest request);

  $pb.GeneratedMessage createRequest($core.String methodName) {
    switch (methodName) {
      case 'ListApprovals':
        return $1.ListApprovalsRequest();
      case 'DecideApproval':
        return $1.DecideApprovalRequest();
      default:
        throw $core.ArgumentError('Unknown method: $methodName');
    }
  }

  $async.Future<$pb.GeneratedMessage> handleCall($pb.ServerContext ctx,
      $core.String methodName, $pb.GeneratedMessage request) {
    switch (methodName) {
      case 'ListApprovals':
        return listApprovals(ctx, request as $1.ListApprovalsRequest);
      case 'DecideApproval':
        return decideApproval(ctx, request as $1.DecideApprovalRequest);
      default:
        throw $core.ArgumentError('Unknown method: $methodName');
    }
  }

  $core.Map<$core.String, $core.dynamic> get $json => ApprovalServiceBase$json;
  $core.Map<$core.String, $core.Map<$core.String, $core.dynamic>>
      get $messageJson => ApprovalServiceBase$messageJson;
}
