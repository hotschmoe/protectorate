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

import 'job.pb.dart' as $2;
import 'node.pb.dart' as $5;
import 'node.pbjson.dart';

export 'node.pb.dart';

abstract class NodeAgentServiceBase extends $pb.GeneratedService {
  $async.Future<$5.NodeMetrics> getMetrics(
      $pb.ServerContext ctx, $5.GetMetricsRequest request);
  $async.Future<$5.NodeMetrics> watchMetrics(
      $pb.ServerContext ctx, $5.WatchMetricsRequest request);
  $async.Future<$2.PrepareJobResponse> prepareJob(
      $pb.ServerContext ctx, $2.PrepareJobRequest request);
  $async.Future<$2.UploadJobDataResponse> uploadJobData(
      $pb.ServerContext ctx, $2.UploadJobDataRequest request);
  $async.Future<$2.JobEvent> runJob(
      $pb.ServerContext ctx, $2.RunJobRequest request);
  $async.Future<$2.JobEvent> attachJob(
      $pb.ServerContext ctx, $2.AttachJobRequest request);
  $async.Future<$2.NodeAgentServiceListJobsResponse> listJobs(
      $pb.ServerContext ctx, $2.NodeAgentServiceListJobsRequest request);
  $async.Future<$2.NodeAgentServiceCancelJobResponse> cancelJob(
      $pb.ServerContext ctx, $2.NodeAgentServiceCancelJobRequest request);
  $async.Future<$5.PutFileResponse> putFile(
      $pb.ServerContext ctx, $5.PutFileRequest request);
  $async.Future<$5.GetFileResponse> getFile(
      $pb.ServerContext ctx, $5.GetFileRequest request);
  $async.Future<$5.ApplyUpdateResponse> applyUpdate(
      $pb.ServerContext ctx, $5.ApplyUpdateRequest request);
  $async.Future<$5.UninstallResponse> uninstall(
      $pb.ServerContext ctx, $5.UninstallRequest request);
  $async.Future<$5.RenewCertificateResponse> renewCertificate(
      $pb.ServerContext ctx, $5.RenewCertificateRequest request);

  $pb.GeneratedMessage createRequest($core.String methodName) {
    switch (methodName) {
      case 'GetMetrics':
        return $5.GetMetricsRequest();
      case 'WatchMetrics':
        return $5.WatchMetricsRequest();
      case 'PrepareJob':
        return $2.PrepareJobRequest();
      case 'UploadJobData':
        return $2.UploadJobDataRequest();
      case 'RunJob':
        return $2.RunJobRequest();
      case 'AttachJob':
        return $2.AttachJobRequest();
      case 'ListJobs':
        return $2.NodeAgentServiceListJobsRequest();
      case 'CancelJob':
        return $2.NodeAgentServiceCancelJobRequest();
      case 'PutFile':
        return $5.PutFileRequest();
      case 'GetFile':
        return $5.GetFileRequest();
      case 'ApplyUpdate':
        return $5.ApplyUpdateRequest();
      case 'Uninstall':
        return $5.UninstallRequest();
      case 'RenewCertificate':
        return $5.RenewCertificateRequest();
      default:
        throw $core.ArgumentError('Unknown method: $methodName');
    }
  }

  $async.Future<$pb.GeneratedMessage> handleCall($pb.ServerContext ctx,
      $core.String methodName, $pb.GeneratedMessage request) {
    switch (methodName) {
      case 'GetMetrics':
        return getMetrics(ctx, request as $5.GetMetricsRequest);
      case 'WatchMetrics':
        return watchMetrics(ctx, request as $5.WatchMetricsRequest);
      case 'PrepareJob':
        return prepareJob(ctx, request as $2.PrepareJobRequest);
      case 'UploadJobData':
        return uploadJobData(ctx, request as $2.UploadJobDataRequest);
      case 'RunJob':
        return runJob(ctx, request as $2.RunJobRequest);
      case 'AttachJob':
        return attachJob(ctx, request as $2.AttachJobRequest);
      case 'ListJobs':
        return listJobs(ctx, request as $2.NodeAgentServiceListJobsRequest);
      case 'CancelJob':
        return cancelJob(ctx, request as $2.NodeAgentServiceCancelJobRequest);
      case 'PutFile':
        return putFile(ctx, request as $5.PutFileRequest);
      case 'GetFile':
        return getFile(ctx, request as $5.GetFileRequest);
      case 'ApplyUpdate':
        return applyUpdate(ctx, request as $5.ApplyUpdateRequest);
      case 'Uninstall':
        return uninstall(ctx, request as $5.UninstallRequest);
      case 'RenewCertificate':
        return renewCertificate(ctx, request as $5.RenewCertificateRequest);
      default:
        throw $core.ArgumentError('Unknown method: $methodName');
    }
  }

  $core.Map<$core.String, $core.dynamic> get $json => NodeAgentServiceBase$json;
  $core.Map<$core.String, $core.Map<$core.String, $core.dynamic>>
      get $messageJson => NodeAgentServiceBase$messageJson;
}

abstract class NodeServiceBase extends $pb.GeneratedService {
  $async.Future<$5.ListNodesResponse> listNodes(
      $pb.ServerContext ctx, $5.ListNodesRequest request);
  $async.Future<$5.NodeStatus> getNode(
      $pb.ServerContext ctx, $5.GetNodeRequest request);
  $async.Future<$5.NodeInfo> addNode(
      $pb.ServerContext ctx, $5.AddNodeRequest request);
  $async.Future<$5.RemoveNodeProgress> removeNode(
      $pb.ServerContext ctx, $5.RemoveNodeRequest request);
  $async.Future<$5.InviteNodeResponse> inviteNode(
      $pb.ServerContext ctx, $5.InviteNodeRequest request);
  $async.Future<$5.JoinNodeResponse> joinNode(
      $pb.ServerContext ctx, $5.JoinNodeRequest request);
  $async.Future<$5.NodeSetupProgress> watchNodeSetup(
      $pb.ServerContext ctx, $5.WatchNodeSetupRequest request);
  $async.Future<$5.PutFileResponse> putFile(
      $pb.ServerContext ctx, $5.PutFileRequest request);
  $async.Future<$5.GetFileResponse> getFile(
      $pb.ServerContext ctx, $5.GetFileRequest request);
  $async.Future<$5.ApplyUpdateResponse> applyUpdate(
      $pb.ServerContext ctx, $5.ApplyUpdateRequest request);
  $async.Future<$5.UpdateNodeProgress> updateNode(
      $pb.ServerContext ctx, $5.UpdateNodeRequest request);
  $async.Future<$5.NodeInfo> setNodeTier(
      $pb.ServerContext ctx, $5.SetNodeTierRequest request);

  $pb.GeneratedMessage createRequest($core.String methodName) {
    switch (methodName) {
      case 'ListNodes':
        return $5.ListNodesRequest();
      case 'GetNode':
        return $5.GetNodeRequest();
      case 'AddNode':
        return $5.AddNodeRequest();
      case 'RemoveNode':
        return $5.RemoveNodeRequest();
      case 'InviteNode':
        return $5.InviteNodeRequest();
      case 'JoinNode':
        return $5.JoinNodeRequest();
      case 'WatchNodeSetup':
        return $5.WatchNodeSetupRequest();
      case 'PutFile':
        return $5.PutFileRequest();
      case 'GetFile':
        return $5.GetFileRequest();
      case 'ApplyUpdate':
        return $5.ApplyUpdateRequest();
      case 'UpdateNode':
        return $5.UpdateNodeRequest();
      case 'SetNodeTier':
        return $5.SetNodeTierRequest();
      default:
        throw $core.ArgumentError('Unknown method: $methodName');
    }
  }

  $async.Future<$pb.GeneratedMessage> handleCall($pb.ServerContext ctx,
      $core.String methodName, $pb.GeneratedMessage request) {
    switch (methodName) {
      case 'ListNodes':
        return listNodes(ctx, request as $5.ListNodesRequest);
      case 'GetNode':
        return getNode(ctx, request as $5.GetNodeRequest);
      case 'AddNode':
        return addNode(ctx, request as $5.AddNodeRequest);
      case 'RemoveNode':
        return removeNode(ctx, request as $5.RemoveNodeRequest);
      case 'InviteNode':
        return inviteNode(ctx, request as $5.InviteNodeRequest);
      case 'JoinNode':
        return joinNode(ctx, request as $5.JoinNodeRequest);
      case 'WatchNodeSetup':
        return watchNodeSetup(ctx, request as $5.WatchNodeSetupRequest);
      case 'PutFile':
        return putFile(ctx, request as $5.PutFileRequest);
      case 'GetFile':
        return getFile(ctx, request as $5.GetFileRequest);
      case 'ApplyUpdate':
        return applyUpdate(ctx, request as $5.ApplyUpdateRequest);
      case 'UpdateNode':
        return updateNode(ctx, request as $5.UpdateNodeRequest);
      case 'SetNodeTier':
        return setNodeTier(ctx, request as $5.SetNodeTierRequest);
      default:
        throw $core.ArgumentError('Unknown method: $methodName');
    }
  }

  $core.Map<$core.String, $core.dynamic> get $json => NodeServiceBase$json;
  $core.Map<$core.String, $core.Map<$core.String, $core.dynamic>>
      get $messageJson => NodeServiceBase$messageJson;
}
