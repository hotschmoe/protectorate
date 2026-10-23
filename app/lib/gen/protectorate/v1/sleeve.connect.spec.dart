//
//  Generated code. Do not modify.
//  source: protectorate/v1/sleeve.proto
//

import "package:connectrpc/connect.dart" as connect;
import "sleeve.pb.dart" as protectoratev1sleeve;

/// SleeveService manages sleeves on the hub machine: one workspace plus one
/// harness, run inside a Linux namespace sandbox in a systemd scope with CPU and
/// memory limits. In chat mode (the default for agent harnesses) the harness
/// runs its official headless protocol and is driven through ChatService. In
/// terminal mode it is a tmux session ("sleeve-<name>"), so CapturePane and the
/// WebSocket terminal work on it. The mode is fixed at spawn. Sleeves are Linux-only.
/// Changes are announced on EventService as "sleeve.spawned", "sleeve.exited",
/// "sleeve.oom", "sleeve.disk_warning", "sleeve.disk_limit" and
/// "sleeve.restored" (subject = sleeve name). A terminal sleeve whose harness
/// screen is recognized also announces "sleeve.activity" with its working,
/// blocked or idle state whenever that changes.
abstract final class SleeveService {
  /// Fully-qualified name of the SleeveService service.
  static const name = 'protectorate.v1.SleeveService';

  /// SpawnSleeve starts a harness in a new isolated sleeve on the hub. The
  /// workspace is an existing directory (bound read-write at the same path) or
  /// a git URL cloned into the hub's workspaces directory. Branch options clone
  /// local checkouts too; brief is delivered as the first chat message.
  /// Respawning the name
  /// of an exited sleeve reuses its home directory and workspace and replaces
  /// its kept session.
  static const spawnSleeve = connect.Spec(
    '/$name/SpawnSleeve',
    connect.StreamType.unary,
    protectoratev1sleeve.SpawnSleeveRequest.new,
    protectoratev1sleeve.SpawnSleeveResponse.new,
  );

  /// ListSleeves returns every known sleeve, running or exited, with its latest
  /// resource usage.
  static const listSleeves = connect.Spec(
    '/$name/ListSleeves',
    connect.StreamType.unary,
    protectoratev1sleeve.ListSleevesRequest.new,
    protectoratev1sleeve.ListSleevesResponse.new,
    idempotency: connect.Idempotency.noSideEffects,
  );

  /// ListWorkspaces returns the directories SpawnSleeve would accept as an
  /// existing workspace: git checkouts and other directories under the hub's
  /// workspaces directory (cloned or created by earlier sleeves), and the
  /// workspaces of known sleeves elsewhere on disk. Read-only reference entries
  /// are also returned with read_only set. It never walks the rest of
  /// the filesystem; any other absolute path can still be given to
  /// SpawnSleeve directly.
  static const listWorkspaces = connect.Spec(
    '/$name/ListWorkspaces',
    connect.StreamType.unary,
    protectoratev1sleeve.ListWorkspacesRequest.new,
    protectoratev1sleeve.ListWorkspacesResponse.new,
    idempotency: connect.Idempotency.noSideEffects,
  );

  /// BrowseWorkspaces lists configured roots when path is empty, otherwise
  /// subdirectories of path. Resolved paths must stay inside a configured root.
  /// Sleeve identity tokens cannot call this RPC, even with read scope.
  static const browseWorkspaces = connect.Spec(
    '/$name/BrowseWorkspaces',
    connect.StreamType.unary,
    protectoratev1sleeve.BrowseWorkspacesRequest.new,
    protectoratev1sleeve.BrowseWorkspacesResponse.new,
    idempotency: connect.Idempotency.noSideEffects,
  );

  /// GetSleeve returns one sleeve with its latest resource usage.
  static const getSleeve = connect.Spec(
    '/$name/GetSleeve',
    connect.StreamType.unary,
    protectoratev1sleeve.GetSleeveRequest.new,
    protectoratev1sleeve.GetSleeveResponse.new,
    idempotency: connect.Idempotency.noSideEffects,
  );

  /// GetTerminalStatus reads the bottom of a running terminal sleeve's screen
  /// now and returns what the harness is doing and which screen rules matched.
  static const getTerminalStatus = connect.Spec(
    '/$name/GetTerminalStatus',
    connect.StreamType.unary,
    protectoratev1sleeve.GetTerminalStatusRequest.new,
    protectoratev1sleeve.GetTerminalStatusResponse.new,
    idempotency: connect.Idempotency.noSideEffects,
  );

  /// KillSleeve stops a sleeve's harness and every process in its sandbox and
  /// retains its exited record. The workspace and the sleeve's home directory
  /// are kept on disk unless drop_caches removes regenerable caches.
  static const killSleeve = connect.Spec(
    '/$name/KillSleeve',
    connect.StreamType.unary,
    protectoratev1sleeve.KillSleeveRequest.new,
    protectoratev1sleeve.KillSleeveResponse.new,
  );

  /// ResleeveSleeve moves a chat sleeve to another harness on the same
  /// workspace, under the same name, without asking the outgoing harness
  /// anything. The hub takes the chat over (no other sender can start a
  /// turn), waits for it to be IDLE (interrupting a turn that runs past wait),
  /// composes facts in code (git status, diff --stat, recent commits, backlog
  /// and mail, the last user messages and the final assistant message from
  /// the outgoing harness's own transcript), has a separate model write a
  /// Goal/State/Unfinished/Next/Gotchas summary (the subscription with the
  /// most spare quota other than the outgoing harness, else pi with the local
  /// LLM), overwrites <workspace>/.protectorate/HANDOFF.md (kept out of git
  /// through .git/info/exclude, archived under the hub's state directory),
  /// replaces the harness and sends it one plain user message naming the
  /// handoff. The summary runs while the outgoing turn winds down. The old
  /// native session stays on disk for a later resume. If the new harness
  /// cannot start, the old one is respawned on its own session. Announced as
  /// "sleeve.resleeved"; a summary by the fallback model, or none, also emits
  /// "resleeve.warning".
  static const resleeveSleeve = connect.Spec(
    '/$name/ResleeveSleeve',
    connect.StreamType.unary,
    protectoratev1sleeve.ResleeveSleeveRequest.new,
    protectoratev1sleeve.ResleeveSleeveResponse.new,
  );

  /// SpawnEnvoy creates or respawns Envoy, the orchestrator: a chat sleeve
  /// named "envoy" in $PROTECTORATE_HOME/orchestrator with its own
  /// capability. It picks the harness (the top of QuotaService.Suggest unless
  /// one is given), stops a running Envoy, revokes every earlier orchestrator
  /// capability, mints a new one, writes the token file (which the hub keeps
  /// fresh while Envoy runs) and the generated context
  /// files (AGENTS.md, CLAUDE.md, hosts.md) and starts the harness. AGENTS.md
  /// is byte-identical across respawns unless the CLI, the tool catalog, the
  /// hub URL, or the scopes change, so a resumed session keeps its
  /// prompt-cache prefix. Only the owner token may call it, and the token it
  /// mints never holds it.
  static const spawnEnvoy = connect.Spec(
    '/$name/SpawnEnvoy',
    connect.StreamType.unary,
    protectoratev1sleeve.SpawnEnvoyRequest.new,
    protectoratev1sleeve.SpawnEnvoyResponse.new,
  );

  /// PruneSleeves inventories disk and removes eligible data only with yes.
  static const pruneSleeves = connect.Spec(
    '/$name/PruneSleeves',
    connect.StreamType.unary,
    protectoratev1sleeve.PruneSleevesRequest.new,
    protectoratev1sleeve.PruneSleevesResponse.new,
  );

  /// CreateCheckpoint snapshots the sleeve's git workspace: tracked files and
  /// untracked files that are not ignored. The hub also checkpoints a chat
  /// sleeve when a turn settles and before a resleeve. Checkpoints live in a
  /// private store under the hub's state, never in the workspace repository,
  /// so no push can carry them; the newest 20 are kept. If nothing changed
  /// since the latest checkpoint, that checkpoint is returned with unchanged.
  static const createCheckpoint = connect.Spec(
    '/$name/CreateCheckpoint',
    connect.StreamType.unary,
    protectoratev1sleeve.CreateCheckpointRequest.new,
    protectoratev1sleeve.CreateCheckpointResponse.new,
  );

  /// ListCheckpoints returns the sleeve's checkpoints, oldest first.
  static const listCheckpoints = connect.Spec(
    '/$name/ListCheckpoints',
    connect.StreamType.unary,
    protectoratev1sleeve.ListCheckpointsRequest.new,
    protectoratev1sleeve.ListCheckpointsResponse.new,
    idempotency: connect.Idempotency.noSideEffects,
  );

  /// DiffCheckpoint returns a unified diff from checkpoint from_id to
  /// checkpoint to_id, or to the current workspace when to_id is 0. Output
  /// beyond 1 MiB is cut off and marked truncated.
  static const diffCheckpoint = connect.Spec(
    '/$name/DiffCheckpoint',
    connect.StreamType.unary,
    protectoratev1sleeve.DiffCheckpointRequest.new,
    protectoratev1sleeve.DiffCheckpointResponse.new,
    idempotency: connect.Idempotency.noSideEffects,
  );

  /// RestoreCheckpoint makes the workspace's non-ignored files match a
  /// checkpoint, removing files the checkpoint lacks. Ignored files, the
  /// workspace's .git directory and its index are untouched. It first
  /// checkpoints the current state, returned as before, so a restore can be
  /// undone. A chat sleeve must be idle.
  static const restoreCheckpoint = connect.Spec(
    '/$name/RestoreCheckpoint',
    connect.StreamType.unary,
    protectoratev1sleeve.RestoreCheckpointRequest.new,
    protectoratev1sleeve.RestoreCheckpointResponse.new,
  );
}
