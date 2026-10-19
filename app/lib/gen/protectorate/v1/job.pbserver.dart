// This is a generated file - do not edit.
//
// Generated from protectorate/v1/job.proto.

// @dart = 3.3

// ignore_for_file: annotate_overrides, camel_case_types, comment_references
// ignore_for_file: constant_identifier_names
// ignore_for_file: curly_braces_in_flow_control_structures
// ignore_for_file: deprecated_member_use_from_same_package, library_prefixes
// ignore_for_file: non_constant_identifier_names, prefer_relative_imports

import 'dart:async' as $async;
import 'dart:core' as $core;

import 'package:protobuf/protobuf.dart' as $pb;

import 'job.pb.dart' as $3;
import 'job.pbjson.dart';

export 'job.pb.dart';

abstract class JobServiceBase extends $pb.GeneratedService {
  $async.Future<$3.Job> submitJob(
      $pb.ServerContext ctx, $3.SubmitJobRequest request);
  $async.Future<$3.Job> getJob($pb.ServerContext ctx, $3.GetJobRequest request);
  $async.Future<$3.ListJobsResponse> listJobs(
      $pb.ServerContext ctx, $3.ListJobsRequest request);
  $async.Future<$3.Job> cancelJob(
      $pb.ServerContext ctx, $3.CancelJobRequest request);
  $async.Future<$3.WatchJobResponse> watchJob(
      $pb.ServerContext ctx, $3.WatchJobRequest request);

  $pb.GeneratedMessage createRequest($core.String methodName) {
    switch (methodName) {
      case 'SubmitJob':
        return $3.SubmitJobRequest();
      case 'GetJob':
        return $3.GetJobRequest();
      case 'ListJobs':
        return $3.ListJobsRequest();
      case 'CancelJob':
        return $3.CancelJobRequest();
      case 'WatchJob':
        return $3.WatchJobRequest();
      default:
        throw $core.ArgumentError('Unknown method: $methodName');
    }
  }

  $async.Future<$pb.GeneratedMessage> handleCall($pb.ServerContext ctx,
      $core.String methodName, $pb.GeneratedMessage request) {
    switch (methodName) {
      case 'SubmitJob':
        return submitJob(ctx, request as $3.SubmitJobRequest);
      case 'GetJob':
        return getJob(ctx, request as $3.GetJobRequest);
      case 'ListJobs':
        return listJobs(ctx, request as $3.ListJobsRequest);
      case 'CancelJob':
        return cancelJob(ctx, request as $3.CancelJobRequest);
      case 'WatchJob':
        return watchJob(ctx, request as $3.WatchJobRequest);
      default:
        throw $core.ArgumentError('Unknown method: $methodName');
    }
  }

  $core.Map<$core.String, $core.dynamic> get $json => JobServiceBase$json;
  $core.Map<$core.String, $core.Map<$core.String, $core.dynamic>>
      get $messageJson => JobServiceBase$messageJson;
}
