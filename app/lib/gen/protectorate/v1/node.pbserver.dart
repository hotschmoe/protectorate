// This is a generated file - do not edit.
//
// Generated from protectorate/v1/node.proto.

// @dart = 3.3

// ignore_for_file: annotate_overrides, camel_case_types, comment_references
// ignore_for_file: constant_identifier_names
// ignore_for_file: curly_braces_in_flow_control_structures
// ignore_for_file: deprecated_member_use_from_same_package, library_prefixes
// ignore_for_file: non_constant_identifier_names, prefer_relative_imports

import 'dart:async' as $async;
import 'dart:core' as $core;

import 'package:protobuf/protobuf.dart' as $pb;

import 'desk.pb.dart' as $4;
import 'job.pb.dart' as $3;
import 'node.pb.dart' as $6;
import 'node.pbjson.dart';

export 'node.pb.dart';

abstract class NodeAgentServiceBase extends $pb.GeneratedService {
  $async.Future<$6.NodeMetrics> getMetrics(
      $pb.ServerContext ctx, $6.GetMetricsRequest request);
  $async.Future<$6.NodeMetrics> watchMetrics(
      $pb.ServerContext ctx, $6.WatchMetricsRequest request);
  $async.Future<$3.PrepareJobResponse> prepareJob(
      $pb.ServerContext ctx, $3.PrepareJobRequest request);
  $async.Future<$3.UploadJobDataResponse> uploadJobData(
      $pb.ServerContext ctx, $3.UploadJobDataRequest request);
  $async.Future<$3.JobEvent> runJob(
      $pb.ServerContext ctx, $3.RunJobRequest request);
  $async.Future<$3.NodeAgentServiceCancelJobResponse> cancelJob(
      $pb.ServerContext ctx, $3.NodeAgentServiceCancelJobRequest request);
  $async.Future<$6.PutFileResponse> putFile(
      $pb.ServerContext ctx, $6.PutFileRequest request);
  $async.Future<$6.GetFileResponse> getFile(
      $pb.ServerContext ctx, $6.GetFileRequest request);
  $async.Future<$4.DeskActionResponse> deskAction(
      $pb.ServerContext ctx, $4.DeskActionRequest request);

  $pb.GeneratedMessage createRequest($core.String methodName) {
    switch (methodName) {
      case 'GetMetrics':
        return $6.GetMetricsRequest();
      case 'WatchMetrics':
        return $6.WatchMetricsRequest();
      case 'PrepareJob':
        return $3.PrepareJobRequest();
      case 'UploadJobData':
        return $3.UploadJobDataRequest();
      case 'RunJob':
        return $3.RunJobRequest();
      case 'CancelJob':
        return $3.NodeAgentServiceCancelJobRequest();
      case 'PutFile':
        return $6.PutFileRequest();
      case 'GetFile':
        return $6.GetFileRequest();
      case 'DeskAction':
        return $4.DeskActionRequest();
      default:
        throw $core.ArgumentError('Unknown method: $methodName');
    }
  }

  $async.Future<$pb.GeneratedMessage> handleCall($pb.ServerContext ctx,
      $core.String methodName, $pb.GeneratedMessage request) {
    switch (methodName) {
      case 'GetMetrics':
        return getMetrics(ctx, request as $6.GetMetricsRequest);
      case 'WatchMetrics':
        return watchMetrics(ctx, request as $6.WatchMetricsRequest);
      case 'PrepareJob':
        return prepareJob(ctx, request as $3.PrepareJobRequest);
      case 'UploadJobData':
        return uploadJobData(ctx, request as $3.UploadJobDataRequest);
      case 'RunJob':
        return runJob(ctx, request as $3.RunJobRequest);
      case 'CancelJob':
        return cancelJob(ctx, request as $3.NodeAgentServiceCancelJobRequest);
      case 'PutFile':
        return putFile(ctx, request as $6.PutFileRequest);
      case 'GetFile':
        return getFile(ctx, request as $6.GetFileRequest);
      case 'DeskAction':
        return deskAction(ctx, request as $4.DeskActionRequest);
      default:
        throw $core.ArgumentError('Unknown method: $methodName');
    }
  }

  $core.Map<$core.String, $core.dynamic> get $json => NodeAgentServiceBase$json;
  $core.Map<$core.String, $core.Map<$core.String, $core.dynamic>>
      get $messageJson => NodeAgentServiceBase$messageJson;
}

abstract class NodeServiceBase extends $pb.GeneratedService {
  $async.Future<$6.ListNodesResponse> listNodes(
      $pb.ServerContext ctx, $6.ListNodesRequest request);
  $async.Future<$6.NodeStatus> getNode(
      $pb.ServerContext ctx, $6.GetNodeRequest request);
  $async.Future<$6.NodeInfo> addNode(
      $pb.ServerContext ctx, $6.AddNodeRequest request);
  $async.Future<$6.RemoveNodeResponse> removeNode(
      $pb.ServerContext ctx, $6.RemoveNodeRequest request);
  $async.Future<$6.PutFileResponse> putFile(
      $pb.ServerContext ctx, $6.PutFileRequest request);
  $async.Future<$6.GetFileResponse> getFile(
      $pb.ServerContext ctx, $6.GetFileRequest request);

  $pb.GeneratedMessage createRequest($core.String methodName) {
    switch (methodName) {
      case 'ListNodes':
        return $6.ListNodesRequest();
      case 'GetNode':
        return $6.GetNodeRequest();
      case 'AddNode':
        return $6.AddNodeRequest();
      case 'RemoveNode':
        return $6.RemoveNodeRequest();
      case 'PutFile':
        return $6.PutFileRequest();
      case 'GetFile':
        return $6.GetFileRequest();
      default:
        throw $core.ArgumentError('Unknown method: $methodName');
    }
  }

  $async.Future<$pb.GeneratedMessage> handleCall($pb.ServerContext ctx,
      $core.String methodName, $pb.GeneratedMessage request) {
    switch (methodName) {
      case 'ListNodes':
        return listNodes(ctx, request as $6.ListNodesRequest);
      case 'GetNode':
        return getNode(ctx, request as $6.GetNodeRequest);
      case 'AddNode':
        return addNode(ctx, request as $6.AddNodeRequest);
      case 'RemoveNode':
        return removeNode(ctx, request as $6.RemoveNodeRequest);
      case 'PutFile':
        return putFile(ctx, request as $6.PutFileRequest);
      case 'GetFile':
        return getFile(ctx, request as $6.GetFileRequest);
      default:
        throw $core.ArgumentError('Unknown method: $methodName');
    }
  }

  $core.Map<$core.String, $core.dynamic> get $json => NodeServiceBase$json;
  $core.Map<$core.String, $core.Map<$core.String, $core.dynamic>>
      get $messageJson => NodeServiceBase$messageJson;
}
