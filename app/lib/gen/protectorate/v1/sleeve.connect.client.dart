//
//  Generated code. Do not modify.
//  source: protectorate/v1/sleeve.proto
//

import "package:connectrpc/connect.dart" as connect;
import "sleeve.pb.dart" as protectoratev1sleeve;
import "sleeve.connect.spec.dart" as specs;

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
extension type SleeveServiceClient (connect.Transport _transport) {
  /// SpawnSleeve starts a harness in a new isolated sleeve on the hub. The
  /// workspace is an existing directory (bound read-write at the same path) or
  /// a git URL cloned into the hub's workspaces directory. Branch options clone
  /// local checkouts too; brief is delivered as the first chat message.
  /// Respawning the name
  /// of an exited sleeve reuses its home directory and workspace and replaces
  /// its kept session.
  Future<protectoratev1sleeve.SpawnSleeveResponse> spawnSleeve(
    protectoratev1sleeve.SpawnSleeveRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.SleeveService.spawnSleeve,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  /// ListSleeves returns every known sleeve, running or exited, with its latest
  /// resource usage.
  Future<protectoratev1sleeve.ListSleevesResponse> listSleeves(
    protectoratev1sleeve.ListSleevesRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.SleeveService.listSleeves,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  /// ListWorkspaces returns the directories SpawnSleeve would accept as an
  /// existing workspace: git checkouts and other directories under the hub's
  /// workspaces directory (cloned or created by earlier sleeves), and the
  /// workspaces of known sleeves elsewhere on disk. Read-only reference entries
  /// are also returned with read_only set. It never walks the rest of
  /// the filesystem; any other absolute path can still be given to
  /// SpawnSleeve directly.
  Future<protectoratev1sleeve.ListWorkspacesResponse> listWorkspaces(
    protectoratev1sleeve.ListWorkspacesRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.SleeveService.listWorkspaces,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  /// BrowseWorkspaces lists configured roots when path is empty, otherwise
  /// subdirectories of path. Resolved paths must stay inside a configured root.
  /// Sleeve identity tokens cannot call this RPC, even with read scope.
  Future<protectoratev1sleeve.BrowseWorkspacesResponse> browseWorkspaces(
    protectoratev1sleeve.BrowseWorkspacesRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.SleeveService.browseWorkspaces,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  /// GetSleeve returns one sleeve with its latest resource usage.
  Future<protectoratev1sleeve.GetSleeveResponse> getSleeve(
    protectoratev1sleeve.GetSleeveRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.SleeveService.getSleeve,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  /// GetTerminalStatus reads the bottom of a running terminal sleeve's screen
  /// now and returns what the harness is doing and which screen rules matched.
  Future<protectoratev1sleeve.GetTerminalStatusResponse> getTerminalStatus(
    protectoratev1sleeve.GetTerminalStatusRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.SleeveService.getTerminalStatus,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  /// KillSleeve stops a sleeve's harness and every process in its sandbox and
  /// retains its exited record. The workspace and the sleeve's home directory
  /// are kept on disk unless drop_caches removes regenerable caches.
  Future<protectoratev1sleeve.KillSleeveResponse> killSleeve(
    protectoratev1sleeve.KillSleeveRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.SleeveService.killSleeve,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

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
  Future<protectoratev1sleeve.ResleeveSleeveResponse> resleeveSleeve(
    protectoratev1sleeve.ResleeveSleeveRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.SleeveService.resleeveSleeve,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  /// SpawnEnvoy creates or respawns Envoy, the orchestrator: a chat sleeve
  /// named "envoy" in $PROTECTORATE_HOME/orchestrator with its own
  /// capability. It picks the harness (the top of QuotaService.Suggest unless
  /// one is given), stops a running Envoy, revokes every earlier orchestrator
  /// capability, mints a new one, writes the token file (which the hub keeps
  /// fresh while Envoy runs) and the generated context
  /// files (AGENTS.md, CLAUDE.md) and starts the harness. AGENTS.md
  /// is byte-identical across respawns unless the CLI, the tool catalog, the
  /// hub URL, or the scopes change, so a resumed session keeps its
  /// prompt-cache prefix. Only the owner token may call it, and the token it
  /// mints never holds it.
  Future<protectoratev1sleeve.SpawnEnvoyResponse> spawnEnvoy(
    protectoratev1sleeve.SpawnEnvoyRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.SleeveService.spawnEnvoy,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  /// PruneSleeves inventories disk and removes eligible data only with yes.
  Future<protectoratev1sleeve.PruneSleevesResponse> pruneSleeves(
    protectoratev1sleeve.PruneSleevesRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.SleeveService.pruneSleeves,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  /// CreateCheckpoint snapshots the sleeve's git workspace: tracked files and
  /// untracked files that are not ignored. The hub also checkpoints a chat
  /// sleeve when a turn settles and before a resleeve. Checkpoints live in a
  /// private store under the hub's state, never in the workspace repository,
  /// so no push can carry them; the newest 20 are kept. If nothing changed
  /// since the latest checkpoint, that checkpoint is returned with unchanged.
  Future<protectoratev1sleeve.CreateCheckpointResponse> createCheckpoint(
    protectoratev1sleeve.CreateCheckpointRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.SleeveService.createCheckpoint,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  /// ListCheckpoints returns the sleeve's checkpoints, oldest first.
  Future<protectoratev1sleeve.ListCheckpointsResponse> listCheckpoints(
    protectoratev1sleeve.ListCheckpointsRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.SleeveService.listCheckpoints,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  /// DiffCheckpoint returns a unified diff from checkpoint from_id to
  /// checkpoint to_id, or to the current workspace when to_id is 0. Output
  /// beyond 1 MiB is cut off and marked truncated.
  Future<protectoratev1sleeve.DiffCheckpointResponse> diffCheckpoint(
    protectoratev1sleeve.DiffCheckpointRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.SleeveService.diffCheckpoint,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  /// RestoreCheckpoint makes the workspace's non-ignored files match a
  /// checkpoint, removing files the checkpoint lacks. Ignored files, the
  /// workspace's .git directory and its index are untouched. It first
  /// checkpoints the current state, returned as before, so a restore can be
  /// undone. A chat sleeve must be idle.
  Future<protectoratev1sleeve.RestoreCheckpointResponse> restoreCheckpoint(
    protectoratev1sleeve.RestoreCheckpointRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.SleeveService.restoreCheckpoint,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }
}
