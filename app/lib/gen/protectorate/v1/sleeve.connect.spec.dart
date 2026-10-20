//
//  Generated code. Do not modify.
//  source: protectorate/v1/sleeve.proto
//

import "package:connectrpc/connect.dart" as connect;
import "sleeve.pb.dart" as protectoratev1sleeve;

/// SleeveService manages sleeves on the hub machine: one workspace plus one
/// harness, run inside a bubblewrap sandbox in a systemd scope with CPU and
/// memory limits. In chat mode (the default for agent harnesses) the harness
/// runs its official headless protocol and is driven through ChatService. In
/// terminal mode it is a tmux session ("sleeve-<name>"), so CapturePane and the
/// WebSocket terminal work on it. The mode is fixed at spawn. Sleeves are Linux-only.
/// Changes are announced on EventService as "sleeve.spawned", "sleeve.exited"
/// and "sleeve.oom" (subject = sleeve name).
abstract final class SleeveService {
  /// Fully-qualified name of the SleeveService service.
  static const name = 'protectorate.v1.SleeveService';

  /// SpawnSleeve starts a harness in a new isolated sleeve on the hub. The
  /// workspace is an existing directory (bound read-write at the same path) or
  /// a git URL cloned into the hub's workspaces directory. Respawning the name
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
  /// workspaces of known sleeves elsewhere on disk. It never walks the rest of
  /// the filesystem; any other absolute path can still be given to
  /// SpawnSleeve directly.
  static const listWorkspaces = connect.Spec(
    '/$name/ListWorkspaces',
    connect.StreamType.unary,
    protectoratev1sleeve.ListWorkspacesRequest.new,
    protectoratev1sleeve.ListWorkspacesResponse.new,
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

  /// KillSleeve stops a sleeve's harness and every process in its sandbox and
  /// forgets the sleeve. The workspace and the sleeve's home directory are kept
  /// on disk.
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
  /// named "envoy" in $PROTECTORATE_HOME/orchestrator with its own scoped
  /// token. It picks the harness (the top of QuotaService.Suggest unless one
  /// is given), stops a running Envoy, revokes every earlier orchestrator
  /// token, mints a new one, writes the token file and the generated context
  /// files (AGENTS.md, CLAUDE.md, hosts.md) and starts the harness. AGENTS.md
  /// is byte-identical across respawns unless the CLI, the tool catalog, the
  /// hub URL, the harness or the scopes change, so a resumed session keeps its
  /// prompt-cache prefix. Only the owner token may call it, and the token it
  /// mints never holds it.
  static const spawnEnvoy = connect.Spec(
    '/$name/SpawnEnvoy',
    connect.StreamType.unary,
    protectoratev1sleeve.SpawnEnvoyRequest.new,
    protectoratev1sleeve.SpawnEnvoyResponse.new,
  );
}
