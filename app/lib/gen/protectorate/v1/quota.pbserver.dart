// This is a generated file - do not edit.
//
// Generated from protectorate/v1/quota.proto.

// @dart = 3.3

// ignore_for_file: annotate_overrides, camel_case_types, comment_references
// ignore_for_file: constant_identifier_names
// ignore_for_file: curly_braces_in_flow_control_structures
// ignore_for_file: deprecated_member_use_from_same_package, library_prefixes
// ignore_for_file: non_constant_identifier_names, prefer_relative_imports

import 'dart:async' as $async;
import 'dart:core' as $core;

import 'package:protobuf/protobuf.dart' as $pb;

import 'quota.pb.dart' as $2;
import 'quota.pbjson.dart';

export 'quota.pb.dart';

abstract class QuotaServiceBase extends $pb.GeneratedService {
  $async.Future<$2.GetQuotaResponse> getQuota(
      $pb.ServerContext ctx, $2.GetQuotaRequest request);

  $pb.GeneratedMessage createRequest($core.String methodName) {
    switch (methodName) {
      case 'GetQuota':
        return $2.GetQuotaRequest();
      default:
        throw $core.ArgumentError('Unknown method: $methodName');
    }
  }

  $async.Future<$pb.GeneratedMessage> handleCall($pb.ServerContext ctx,
      $core.String methodName, $pb.GeneratedMessage request) {
    switch (methodName) {
      case 'GetQuota':
        return getQuota(ctx, request as $2.GetQuotaRequest);
      default:
        throw $core.ArgumentError('Unknown method: $methodName');
    }
  }

  $core.Map<$core.String, $core.dynamic> get $json => QuotaServiceBase$json;
  $core.Map<$core.String, $core.Map<$core.String, $core.dynamic>>
      get $messageJson => QuotaServiceBase$messageJson;
}
