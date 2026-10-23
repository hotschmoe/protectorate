// This is a generated file - do not edit.
//
// Generated from protectorate/v1/audit.proto.

// @dart = 3.3

// ignore_for_file: annotate_overrides, camel_case_types, comment_references
// ignore_for_file: constant_identifier_names
// ignore_for_file: curly_braces_in_flow_control_structures
// ignore_for_file: deprecated_member_use_from_same_package, library_prefixes
// ignore_for_file: non_constant_identifier_names, prefer_relative_imports

import 'dart:async' as $async;
import 'dart:core' as $core;

import 'package:protobuf/protobuf.dart' as $pb;

import 'audit.pb.dart' as $1;
import 'audit.pbjson.dart';

export 'audit.pb.dart';

abstract class AuditServiceBase extends $pb.GeneratedService {
  $async.Future<$1.ListAuditEntriesResponse> listAuditEntries(
      $pb.ServerContext ctx, $1.ListAuditEntriesRequest request);
  $async.Future<$1.VerifyAuditResponse> verifyAudit(
      $pb.ServerContext ctx, $1.VerifyAuditRequest request);

  $pb.GeneratedMessage createRequest($core.String methodName) {
    switch (methodName) {
      case 'ListAuditEntries':
        return $1.ListAuditEntriesRequest();
      case 'VerifyAudit':
        return $1.VerifyAuditRequest();
      default:
        throw $core.ArgumentError('Unknown method: $methodName');
    }
  }

  $async.Future<$pb.GeneratedMessage> handleCall($pb.ServerContext ctx,
      $core.String methodName, $pb.GeneratedMessage request) {
    switch (methodName) {
      case 'ListAuditEntries':
        return listAuditEntries(ctx, request as $1.ListAuditEntriesRequest);
      case 'VerifyAudit':
        return verifyAudit(ctx, request as $1.VerifyAuditRequest);
      default:
        throw $core.ArgumentError('Unknown method: $methodName');
    }
  }

  $core.Map<$core.String, $core.dynamic> get $json => AuditServiceBase$json;
  $core.Map<$core.String, $core.Map<$core.String, $core.dynamic>>
      get $messageJson => AuditServiceBase$messageJson;
}
