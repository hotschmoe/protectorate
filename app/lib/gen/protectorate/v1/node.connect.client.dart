//
//  Generated code. Do not modify.
//  source: protectorate/v1/node.proto
//

import "package:connectrpc/connect.dart" as connect;
import "node.pb.dart" as protectoratev1node;
import "node.connect.spec.dart" as specs;
import "job.pb.dart" as protectoratev1job;

/// NodeAgentService runs on every node. The hub polls GetMetrics; WatchMetrics
/// is for live views. The job RPCs are called by the hub only and are unary
/// except RunJob, so they work over plain HTTP/1.1.
extension type NodeAgentServiceClient (connect.Transport _transport) {
  /// GetMetrics collects this node's current metrics.
  Future<protectoratev1node.NodeMetrics> getMetrics(
    protectoratev1node.GetMetricsRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.NodeAgentService.getMetrics,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  /// WatchMetrics streams this node's metrics at an interval.
  Stream<protectoratev1node.NodeMetrics> watchMetrics(
    protectoratev1node.WatchMetricsRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).server(
      specs.NodeAgentService.watchMetrics,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  /// PrepareJob tells the hub what the node already holds for a git job: whether
  /// it has the commit and which commits it knows, so the hub can send a
  /// smaller bundle.
  Future<protectoratev1job.PrepareJobResponse> prepareJob(
    protectoratev1job.PrepareJobRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.NodeAgentService.prepareJob,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  /// UploadJobData appends one chunk of a job's source (a git bundle or an
  /// artifact file). Chunks must arrive in order; the last one carries the
  /// SHA-256 of the whole upload.
  Future<protectoratev1job.UploadJobDataResponse> uploadJobData(
    protectoratev1job.UploadJobDataRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.NodeAgentService.uploadJobData,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  /// RunJob checks the source out, runs the command and streams its output;
  /// matching artifacts follow the exit. The job dies with the stream: if the
  /// caller disconnects, the process tree is killed.
  Stream<protectoratev1job.JobEvent> runJob(
    protectoratev1job.RunJobRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).server(
      specs.NodeAgentService.runJob,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  /// CancelJob kills a running job's process tree.
  Future<protectoratev1job.NodeAgentServiceCancelJobResponse> cancelJob(
    protectoratev1job.NodeAgentServiceCancelJobRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.NodeAgentService.cancelJob,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }
}
/// NodeService runs on the hub: the inventory plus the latest metrics of each
/// node. Polls are announced on EventService as "node.metrics" (subject = node
/// name, payload = summary), "node.unreachable", "node.joined" (an invited
/// node answered its first poll) and "node.name_mismatch".
extension type NodeServiceClient (connect.Transport _transport) {
  /// ListNodes returns every node with its latest metrics summary,
  /// freshness and alerts.
  Future<protectoratev1node.ListNodesResponse> listNodes(
    protectoratev1node.ListNodesRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.NodeService.listNodes,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  /// GetNode returns one node with full metrics: filesystems, GPUs,
  /// containers, engine disk usage and watched directories.
  Future<protectoratev1node.NodeStatus> getNode(
    protectoratev1node.GetNodeRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.NodeService.getNode,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  /// AddNode adds or updates a node in the hub inventory. The hub polls the
  /// address with its node token, so only the owner token may call it.
  Future<protectoratev1node.NodeInfo> addNode(
    protectoratev1node.AddNodeRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.NodeService.addNode,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  /// RemoveNode removes a node from the hub inventory. It does not touch the
  /// machine itself.
  Future<protectoratev1node.RemoveNodeResponse> removeNode(
    protectoratev1node.RemoveNodeRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.NodeService.removeNode,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }
}
