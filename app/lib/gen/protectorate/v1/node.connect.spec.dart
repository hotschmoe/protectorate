//
//  Generated code. Do not modify.
//  source: protectorate/v1/node.proto
//

import "package:connectrpc/connect.dart" as connect;
import "node.pb.dart" as protectoratev1node;
import "job.pb.dart" as protectoratev1job;
import "desk.pb.dart" as protectoratev1desk;

/// NodeAgentService runs on every node. The hub polls GetMetrics; WatchMetrics
/// is for live views. The job RPCs are called by the hub only and are unary
/// except RunJob, so they work over plain HTTP/1.1.
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

  /// PrepareJob tells the hub what the node already holds for a git job: whether
  /// it has the commit and which commits it knows, so the hub can send a
  /// smaller bundle.
  static const prepareJob = connect.Spec(
    '/$name/PrepareJob',
    connect.StreamType.unary,
    protectoratev1job.PrepareJobRequest.new,
    protectoratev1job.PrepareJobResponse.new,
  );

  /// UploadJobData appends one chunk of a job's source (a git bundle or an
  /// artifact file). Chunks must arrive in order; the last one carries the
  /// SHA-256 of the whole upload.
  static const uploadJobData = connect.Spec(
    '/$name/UploadJobData',
    connect.StreamType.unary,
    protectoratev1job.UploadJobDataRequest.new,
    protectoratev1job.UploadJobDataResponse.new,
  );

  /// RunJob checks the source out, runs the command and streams its output;
  /// matching artifacts follow the exit. The job dies with the stream: if the
  /// caller disconnects, the process tree is killed.
  static const runJob = connect.Spec(
    '/$name/RunJob',
    connect.StreamType.server,
    protectoratev1job.RunJobRequest.new,
    protectoratev1job.JobEvent.new,
  );

  /// CancelJob kills a running job's process tree.
  static const cancelJob = connect.Spec(
    '/$name/CancelJob',
    connect.StreamType.unary,
    protectoratev1job.NodeAgentServiceCancelJobRequest.new,
    protectoratev1job.NodeAgentServiceCancelJobResponse.new,
  );

  /// PutFile receives one chunk of a file or directory transfer into the
  /// node's file root. The hub relays the caller's chunks unchanged.
  static const putFile = connect.Spec(
    '/$name/PutFile',
    connect.StreamType.unary,
    protectoratev1node.PutFileRequest.new,
    protectoratev1node.PutFileResponse.new,
  );

  /// GetFile streams a file or directory out of the node's file root.
  static const getFile = connect.Spec(
    '/$name/GetFile',
    connect.StreamType.server,
    protectoratev1node.GetFileRequest.new,
    protectoratev1node.GetFileResponse.new,
  );

  /// DeskAction runs one desktop command in the node agent's interactive
  /// session. See DeskService.
  static const deskAction = connect.Spec(
    '/$name/DeskAction',
    connect.StreamType.unary,
    protectoratev1desk.DeskActionRequest.new,
    protectoratev1desk.DeskActionResponse.new,
  );

  /// ApplyUpdate swaps the agent's binary for a file already transferred into
  /// its file root and restarts the agent. See NodeService.ApplyUpdate.
  static const applyUpdate = connect.Spec(
    '/$name/ApplyUpdate',
    connect.StreamType.unary,
    protectoratev1node.ApplyUpdateRequest.new,
    protectoratev1node.ApplyUpdateResponse.new,
  );

  /// GetDeskState adopts the desk policy the hub sends (none clears it) and
  /// evaluates it now. The node enforces the policy itself on every input
  /// command, so the hub's view can be a few seconds old without harm.
  static const getDeskState = connect.Spec(
    '/$name/GetDeskState',
    connect.StreamType.unary,
    protectoratev1node.GetDeskStateRequest.new,
    protectoratev1desk.DeskState.new,
  );

  /// Uninstall unregisters the agent's startup (the logon task, systemd user
  /// unit or termux service `node install` registered) and stops the agent
  /// once it has answered. It deletes nothing; the response names what is left.
  static const uninstall = connect.Spec(
    '/$name/Uninstall',
    connect.StreamType.unary,
    protectoratev1node.UninstallRequest.new,
    protectoratev1node.UninstallResponse.new,
  );
}
/// NodeService runs on the hub: the inventory plus the latest metrics of each
/// node. Polls are announced on EventService as "node.metrics" (subject = node
/// name, payload = summary), "node.unreachable", "node.joined" (an invited
/// node answered its first poll) and "node.name_mismatch".
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

  /// AddNode adds or updates a node in the hub inventory. The hub polls the
  /// address with its node token, so only the owner token may call it.
  static const addNode = connect.Spec(
    '/$name/AddNode',
    connect.StreamType.unary,
    protectoratev1node.AddNodeRequest.new,
    protectoratev1node.NodeInfo.new,
  );

  /// RemoveNode takes a node out of service and forgets it. It drains the
  /// node first (no new jobs or leases; running jobs are waited for or
  /// cancelled as jobs says), optionally asks the agent to uninstall itself,
  /// expires an unused invite key, revokes the node's tailnet enrollment,
  /// releases its leases and removes the inventory entry. It streams the
  /// steps and fails, leaving the node in service, if any step fails. A
  /// revoked machine that comes back cannot rejoin without a new invite.
  /// Cancelling a pending invite is the same call. Only the owner token may
  /// call it.
  static const removeNode = connect.Spec(
    '/$name/RemoveNode',
    connect.StreamType.server,
    protectoratev1node.RemoveNodeRequest.new,
    protectoratev1node.RemoveNodeProgress.new,
  );

  /// InviteNode creates a single-use tailnet enrollment key for a new node,
  /// records the node in the inventory as invited and returns the one-line
  /// installers that use it. Inviting a pending node again expires its
  /// previous key. Only the owner token may call it.
  static const inviteNode = connect.Spec(
    '/$name/InviteNode',
    connect.StreamType.unary,
    protectoratev1node.InviteNodeRequest.new,
    protectoratev1node.InviteNodeResponse.new,
  );

  /// WatchNodeSetup follows a node from its invite to ready. It sends the
  /// current stage at once and again whenever the stage or its problems
  /// change, and ends after READY.
  static const watchNodeSetup = connect.Spec(
    '/$name/WatchNodeSetup',
    connect.StreamType.server,
    protectoratev1node.WatchNodeSetupRequest.new,
    protectoratev1node.NodeSetupProgress.new,
    idempotency: connect.Idempotency.noSideEffects,
  );

  /// PutFile copies a file or directory to a node, one chunk per call so it
  /// works over plain HTTP/1.1. A transfer is a sequence of calls sharing a
  /// transfer_id with consecutive offsets; the call with last set carries the
  /// size and SHA-256 of the whole (uncompressed) stream, and only then is the
  /// destination replaced, atomically. The hub relays chunks without storing
  /// them. The remote path is confined to the node's file root unless the
  /// node agent was started unconfined.
  static const putFile = connect.Spec(
    '/$name/PutFile',
    connect.StreamType.unary,
    protectoratev1node.PutFileRequest.new,
    protectoratev1node.PutFileResponse.new,
  );

  /// GetFile streams a file or directory from a node: a header, chunks and a
  /// trailer with the size and SHA-256 the receiver must verify.
  static const getFile = connect.Spec(
    '/$name/GetFile',
    connect.StreamType.server,
    protectoratev1node.GetFileRequest.new,
    protectoratev1node.GetFileResponse.new,
  );

  /// ApplyUpdate installs a new agent binary that PutFile placed in the node's
  /// file root. The node verifies size and SHA-256, runs the file's
  /// `version --json`, swaps it in keeping exactly one previous binary
  /// (<exe>.old) and restarts itself through its startup registration, or by
  /// re-executing. It answers before restarting. The new binary is on trial:
  /// unless the hub reaches it, the node rolls back to the previous binary.
  /// Only the owner token may call it. The hub's own node cannot be updated
  /// this way.
  static const applyUpdate = connect.Spec(
    '/$name/ApplyUpdate',
    connect.StreamType.unary,
    protectoratev1node.ApplyUpdateRequest.new,
    protectoratev1node.ApplyUpdateResponse.new,
  );

  /// UpdateNode brings a node to the hub's version: the hub sends the node
  /// the binary for its OS and architecture (its own executable for the hub's
  /// platform, otherwise protectorate-<os>-<arch> from its binary dir), has
  /// the node verify and swap it in as ApplyUpdate does, then watches until
  /// the node reports the new version or puts the old binary back. It streams
  /// the progress and ends when the update is confirmed, rolled back or
  /// skipped (the node is already at the hub's version and force is unset);
  /// any failure ends the stream with an error. Only the owner token may call
  /// it. The hub's own node cannot be updated this way.
  static const updateNode = connect.Spec(
    '/$name/UpdateNode',
    connect.StreamType.server,
    protectoratev1node.UpdateNodeRequest.new,
    protectoratev1node.UpdateNodeProgress.new,
  );

  /// SetDeskPolicy sets or clears (policy unset) when agents may drive a
  /// node's desktop. Only the owner token may call it. The hub keeps the
  /// policy and sends it to the node, which enforces it.
  static const setDeskPolicy = connect.Spec(
    '/$name/SetDeskPolicy',
    connect.StreamType.unary,
    protectoratev1node.SetDeskPolicyRequest.new,
    protectoratev1node.DeskPolicyStatus.new,
  );

  /// GetDeskPolicy returns a node's policy, its current evaluation and who
  /// holds the desktop lease.
  static const getDeskPolicy = connect.Spec(
    '/$name/GetDeskPolicy',
    connect.StreamType.unary,
    protectoratev1node.GetDeskPolicyRequest.new,
    protectoratev1node.DeskPolicyStatus.new,
  );
}
