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
  /// matching artifacts follow the exit. The job does not depend on the
  /// stream: if the caller disconnects it keeps running (bounded by its
  /// timeout) and AttachJob picks the stream up again.
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

  /// AttachJob streams a job again from an output offset: started, the output
  /// from from_offset, then artifacts and the result once it ended. A finished
  /// job is kept for an hour. NOT_FOUND when the node never had the job.
  Stream<protectoratev1job.JobEvent> attachJob(
    protectoratev1job.AttachJobRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).server(
      specs.NodeAgentService.attachJob,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  /// ListJobs lists the jobs the node is running or still holds the result of.
  Future<protectoratev1job.NodeAgentServiceListJobsResponse> listJobs(
    protectoratev1job.NodeAgentServiceListJobsRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.NodeAgentService.listJobs,
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

  /// PutFile receives one chunk of a file or directory transfer into the
  /// node's file root. The hub relays the caller's chunks unchanged.
  Future<protectoratev1node.PutFileResponse> putFile(
    protectoratev1node.PutFileRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.NodeAgentService.putFile,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  /// GetFile streams a file or directory out of the node's file root.
  Stream<protectoratev1node.GetFileResponse> getFile(
    protectoratev1node.GetFileRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).server(
      specs.NodeAgentService.getFile,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  /// ApplyUpdate swaps the agent's binary for a file already transferred into
  /// its file root and restarts the agent. See NodeService.ApplyUpdate.
  Future<protectoratev1node.ApplyUpdateResponse> applyUpdate(
    protectoratev1node.ApplyUpdateRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.NodeAgentService.applyUpdate,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  /// Uninstall unregisters the agent's startup (the logon task, systemd user
  /// unit or termux service `node install` registered) and stops the agent
  /// once it has answered. It deletes nothing; the response names what is left.
  Future<protectoratev1node.UninstallResponse> uninstall(
    protectoratev1node.UninstallRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.NodeAgentService.uninstall,
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

  /// RemoveNode takes a node out of service and forgets it. It drains the
  /// node first (no new jobs or leases; running jobs are waited for or
  /// cancelled as jobs says), optionally asks the agent to uninstall itself,
  /// expires an unused invite key, revokes the node's tailnet enrollment,
  /// releases its leases and removes the inventory entry. It streams the
  /// steps and fails, leaving the node in service, if any step fails. A
  /// revoked machine that comes back cannot rejoin without a new invite.
  /// Cancelling a pending invite is the same call. Only the owner token may
  /// call it.
  Stream<protectoratev1node.RemoveNodeProgress> removeNode(
    protectoratev1node.RemoveNodeRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).server(
      specs.NodeService.removeNode,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  /// InviteNode creates a single-use tailnet enrollment key for a new node,
  /// records the node in the inventory as invited and returns the one-line
  /// installers that use it. Inviting a pending node again expires its
  /// previous key. Only the owner token may call it.
  Future<protectoratev1node.InviteNodeResponse> inviteNode(
    protectoratev1node.InviteNodeRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.NodeService.inviteNode,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  /// WatchNodeSetup follows a node from its invite to ready. It sends the
  /// current stage at once and again whenever the stage or its problems
  /// change, and ends after READY.
  Stream<protectoratev1node.NodeSetupProgress> watchNodeSetup(
    protectoratev1node.WatchNodeSetupRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).server(
      specs.NodeService.watchNodeSetup,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  /// PutFile copies a file or directory to a node, one chunk per call so it
  /// works over plain HTTP/1.1. A transfer is a sequence of calls sharing a
  /// transfer_id with consecutive offsets; the call with last set carries the
  /// size and SHA-256 of the whole (uncompressed) stream, and only then is the
  /// destination replaced, atomically. The hub relays chunks without storing
  /// them. The remote path is confined to the node's file root unless the
  /// node agent was started unconfined.
  Future<protectoratev1node.PutFileResponse> putFile(
    protectoratev1node.PutFileRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.NodeService.putFile,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  /// GetFile streams a file or directory from a node: a header, chunks and a
  /// trailer with the size and SHA-256 the receiver must verify.
  Stream<protectoratev1node.GetFileResponse> getFile(
    protectoratev1node.GetFileRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).server(
      specs.NodeService.getFile,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  /// ApplyUpdate installs a new agent binary that PutFile placed in the node's
  /// file root. The node verifies size and SHA-256, runs the file's
  /// `version --json`, swaps it in keeping exactly one previous binary
  /// (<exe>.old) and restarts itself through its startup registration, or by
  /// re-executing. It answers before restarting. The new binary is on trial:
  /// unless the hub reaches it, the node rolls back to the previous binary.
  /// Only the owner token may call it. The hub's own node cannot be updated
  /// this way.
  Future<protectoratev1node.ApplyUpdateResponse> applyUpdate(
    protectoratev1node.ApplyUpdateRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.NodeService.applyUpdate,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  /// UpdateNode brings a node to the hub's version: the hub sends the node
  /// the binary for its OS and architecture (its own executable for the hub's
  /// platform, otherwise protectorate-<os>-<arch> from its binary dir), has
  /// the node verify and swap it in as ApplyUpdate does, then watches until
  /// the node reports the new version or puts the old binary back. It streams
  /// the progress and ends when the update is confirmed, rolled back or
  /// skipped (the node is already at the hub's version and force is unset);
  /// any failure ends the stream with an error. Only the owner token may call
  /// it. The hub's own node cannot be updated this way.
  Stream<protectoratev1node.UpdateNodeProgress> updateNode(
    protectoratev1node.UpdateNodeRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).server(
      specs.NodeService.updateNode,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  /// SetNodeTier changes a node's tier in the inventory and its
  /// tag:tier-<tier> tag on the tailnet, and announces "node.tier". Only the
  /// owner token may call it. The hub's own node is always "shared".
  Future<protectoratev1node.NodeInfo> setNodeTier(
    protectoratev1node.SetNodeTierRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.NodeService.setNodeTier,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }
}
