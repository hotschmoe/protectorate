// This is a generated file - do not edit.
//
// Generated from protectorate/v1/lease.proto.

// @dart = 3.3

// ignore_for_file: annotate_overrides, camel_case_types, comment_references
// ignore_for_file: constant_identifier_names
// ignore_for_file: curly_braces_in_flow_control_structures
// ignore_for_file: deprecated_member_use_from_same_package, library_prefixes
// ignore_for_file: non_constant_identifier_names, prefer_relative_imports

import 'dart:async' as $async;
import 'dart:core' as $core;

import 'package:protobuf/protobuf.dart' as $pb;

import 'lease.pb.dart' as $2;
import 'lease.pbjson.dart';

export 'lease.pb.dart';

abstract class LeaseServiceBase extends $pb.GeneratedService {
  $async.Future<$2.Lease> acquireLease(
      $pb.ServerContext ctx, $2.AcquireLeaseRequest request);
  $async.Future<$2.Lease> renewLease(
      $pb.ServerContext ctx, $2.RenewLeaseRequest request);
  $async.Future<$2.ReleaseLeaseResponse> releaseLease(
      $pb.ServerContext ctx, $2.ReleaseLeaseRequest request);
  $async.Future<$2.ListLeasesResponse> listLeases(
      $pb.ServerContext ctx, $2.ListLeasesRequest request);

  $pb.GeneratedMessage createRequest($core.String methodName) {
    switch (methodName) {
      case 'AcquireLease':
        return $2.AcquireLeaseRequest();
      case 'RenewLease':
        return $2.RenewLeaseRequest();
      case 'ReleaseLease':
        return $2.ReleaseLeaseRequest();
      case 'ListLeases':
        return $2.ListLeasesRequest();
      default:
        throw $core.ArgumentError('Unknown method: $methodName');
    }
  }

  $async.Future<$pb.GeneratedMessage> handleCall($pb.ServerContext ctx,
      $core.String methodName, $pb.GeneratedMessage request) {
    switch (methodName) {
      case 'AcquireLease':
        return acquireLease(ctx, request as $2.AcquireLeaseRequest);
      case 'RenewLease':
        return renewLease(ctx, request as $2.RenewLeaseRequest);
      case 'ReleaseLease':
        return releaseLease(ctx, request as $2.ReleaseLeaseRequest);
      case 'ListLeases':
        return listLeases(ctx, request as $2.ListLeasesRequest);
      default:
        throw $core.ArgumentError('Unknown method: $methodName');
    }
  }

  $core.Map<$core.String, $core.dynamic> get $json => LeaseServiceBase$json;
  $core.Map<$core.String, $core.Map<$core.String, $core.dynamic>>
      get $messageJson => LeaseServiceBase$messageJson;
}
