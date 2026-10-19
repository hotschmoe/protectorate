//
//  Generated code. Do not modify.
//  source: protectorate/v1/node.proto
//

import "package:connectrpc/connect.dart" as connect;
import "node.pb.dart" as protectoratev1node;

/// NodeAgentService runs on every node (and in-process for the hub's own
/// node). The hub polls GetMetrics; WatchMetrics is for live views.
abstract final class NodeAgentService {
  /// Fully-qualified name of the NodeAgentService service.
  static const name = 'protectorate.v1.NodeAgentService';

  /// GetMetrics collects this node's current metrics.
  static const getMetrics = connect.Spec(
    '/$name/GetMetrics',
    connect.StreamType.unary,
    protectoratev1node.GetMetricsRequest.new,
    protectoratev1node.NodeMetrics.new,
    idempotency: connect.Idempotency.noSideEffects,
  );

  /// WatchMetrics streams this node's metrics at an interval.
  static const watchMetrics = connect.Spec(
    '/$name/WatchMetrics',
    connect.StreamType.server,
    protectoratev1node.WatchMetricsRequest.new,
    protectoratev1node.NodeMetrics.new,
    idempotency: connect.Idempotency.noSideEffects,
  );
}
/// NodeService runs on the hub: the inventory plus the latest metrics of each
/// node. Polls are announced on EventService as "node.metrics" (subject = node
/// name, payload = summary) and "node.unreachable".
abstract final class NodeService {
  /// Fully-qualified name of the NodeService service.
  static const name = 'protectorate.v1.NodeService';

  /// ListNodes returns every node with its latest metrics summary,
  /// freshness and alerts.
  static const listNodes = connect.Spec(
    '/$name/ListNodes',
    connect.StreamType.unary,
    protectoratev1node.ListNodesRequest.new,
    protectoratev1node.ListNodesResponse.new,
    idempotency: connect.Idempotency.noSideEffects,
  );

  /// GetNode returns one node with full metrics: filesystems, GPUs,
  /// containers, engine disk usage and watched directories.
  static const getNode = connect.Spec(
    '/$name/GetNode',
    connect.StreamType.unary,
    protectoratev1node.GetNodeRequest.new,
    protectoratev1node.NodeStatus.new,
    idempotency: connect.Idempotency.noSideEffects,
  );

  /// AddNode adds or updates a node in the hub inventory.
  static const addNode = connect.Spec(
    '/$name/AddNode',
    connect.StreamType.unary,
    protectoratev1node.AddNodeRequest.new,
    protectoratev1node.NodeInfo.new,
  );

  /// RemoveNode removes a node from the hub inventory. It does not touch the
  /// machine itself.
  static const removeNode = connect.Spec(
    '/$name/RemoveNode',
    connect.StreamType.unary,
    protectoratev1node.RemoveNodeRequest.new,
    protectoratev1node.RemoveNodeResponse.new,
  );
}
