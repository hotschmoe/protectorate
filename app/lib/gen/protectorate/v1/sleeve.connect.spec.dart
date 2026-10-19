//
//  Generated code. Do not modify.
//  source: protectorate/v1/sleeve.proto
//

import "package:connectrpc/connect.dart" as connect;
import "sleeve.pb.dart" as protectoratev1sleeve;

/// SleeveService manages sleeves on the hub machine: one workspace plus one
/// harness, run inside a bubblewrap sandbox in a systemd scope with CPU and
/// memory limits. Each sleeve is a tmux session ("sleeve-<name>"), so
/// CapturePane and the WebSocket terminal work on it. Sleeves are Linux-only.
/// Changes are announced on EventService as "sleeve.spawned", "sleeve.exited"
/// and "sleeve.oom" (subject = sleeve name).
abstract final class SleeveService {
  /// Fully-qualified name of the SleeveService service.
  static const name = 'protectorate.v1.SleeveService';

  /// SpawnSleeve starts a harness in a new isolated sleeve on the hub. The
  /// workspace is an existing directory (bound read-write at the same path) or
  /// a git URL cloned into the hub's workspaces directory. Respawning the name
  /// of an exited sleeve reuses its home directory and workspace.
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
}
