//
//  Generated code. Do not modify.
//  source: protectorate/v1/sleeve.proto
//

import "package:connectrpc/connect.dart" as connect;
import "sleeve.pb.dart" as protectoratev1sleeve;
import "sleeve.connect.spec.dart" as specs;

/// SleeveService manages sleeves on the hub machine: one workspace plus one
/// harness, run inside a bubblewrap sandbox in a systemd scope with CPU and
/// memory limits. In chat mode (the default for agent harnesses) the harness
/// runs its official headless protocol and is driven through ChatService. In
/// terminal mode it is a tmux session ("sleeve-<name>"), so CapturePane and the
/// WebSocket terminal work on it. The mode is fixed at spawn. Sleeves are Linux-only.
/// Changes are announced on EventService as "sleeve.spawned", "sleeve.exited"
/// and "sleeve.oom" (subject = sleeve name).
extension type SleeveServiceClient (connect.Transport _transport) {
  /// SpawnSleeve starts a harness in a new isolated sleeve on the hub. The
  /// workspace is an existing directory (bound read-write at the same path) or
  /// a git URL cloned into the hub's workspaces directory. Respawning the name
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

  /// KillSleeve stops a sleeve's harness and every process in its sandbox and
  /// forgets the sleeve. The workspace and the sleeve's home directory are kept
  /// on disk.
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
}
