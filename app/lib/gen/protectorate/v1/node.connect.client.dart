//
//  Generated code. Do not modify.
//  source: protectorate/v1/node.proto
//

import "package:connectrpc/connect.dart" as connect;
import "node.pb.dart" as protectoratev1node;
import "node.connect.spec.dart" as specs;

/// NodeAgentService runs on every node (and in-process for the hub's own
/// node). The hub polls GetMetrics; WatchMetrics is for live views.
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
}
/// NodeService runs on the hub: the inventory plus the latest metrics of each
/// node. Polls are announced on EventService as "node.metrics" (subject = node
/// name, payload = summary) and "node.unreachable".
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

  /// AddNode adds or updates a node in the hub inventory.
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
