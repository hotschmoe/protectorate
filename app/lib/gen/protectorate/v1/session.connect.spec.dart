//
//  Generated code. Do not modify.
//  source: protectorate/v1/session.proto
//

import "package:connectrpc/connect.dart" as connect;
import "session.pb.dart" as protectoratev1session;

/// SessionService manages tmux-backed harness sessions. Interactive access is
/// the WebSocket terminal at GET /ws/terminal/{name}, not an RPC.
/// When a session's command ends, the session stays in state EXITED with its
/// last screen, exit code and exit signal until it is killed, respawned, or
/// removed by the hub's dead-session retention sweep (--dead-retention).
/// Changes are announced on EventService: "session.created", "session.exited"
/// (command ended; payload has exit_code, exit_signal and exited_at) and
/// "session.removed" (session gone). The WebSocket terminal attaches to an
/// exited session read-only and sends a notice frame saying so.
abstract final class SessionService {
  /// Fully-qualified name of the SessionService service.
  static const name = 'protectorate.v1.SessionService';

  /// ListSessions returns all sessions, running and exited.
  static const listSessions = connect.Spec(
    '/$name/ListSessions',
    connect.StreamType.unary,
    protectoratev1session.ListSessionsRequest.new,
    protectoratev1session.ListSessionsResponse.new,
    idempotency: connect.Idempotency.noSideEffects,
  );

  /// CreateSession starts a detached tmux session running command, or the
  /// default shell when command is empty. Names starting with "sleeve-" are
  /// reserved for sleeves and fail with FAILED_PRECONDITION. The command runs
  /// unsandboxed as the hub's OS user, so only the owner token may call it.
  static const createSession = connect.Spec(
    '/$name/CreateSession',
    connect.StreamType.unary,
    protectoratev1session.CreateSessionRequest.new,
    protectoratev1session.CreateSessionResponse.new,
  );

  /// KillSession terminates a session and every process in it. It also removes
  /// an exited session, discarding its final screen.
  static const killSession = connect.Spec(
    '/$name/KillSession',
    connect.StreamType.unary,
    protectoratev1session.KillSessionRequest.new,
    protectoratev1session.KillSessionResponse.new,
  );

  /// RespawnSession restarts an exited session with its original command and
  /// working directory. The previous output stays in scrollback. It fails with
  /// FAILED_PRECONDITION while the session is still running, and for sessions
  /// named "sleeve-*", which only SleeveService.SpawnSleeve may restart. It
  /// reruns an unsandboxed command, so only the owner token may call it.
  static const respawnSession = connect.Spec(
    '/$name/RespawnSession',
    connect.StreamType.unary,
    protectoratev1session.RespawnSessionRequest.new,
    protectoratev1session.RespawnSessionResponse.new,
  );

  /// CapturePane returns the current text of a session's active pane. It also
  /// works on exited sessions, which is how a crash is diagnosed. Include at
  /// least one history line to see the whole final screen of an exited session.
  static const capturePane = connect.Spec(
    '/$name/CapturePane',
    connect.StreamType.unary,
    protectoratev1session.CapturePaneRequest.new,
    protectoratev1session.CapturePaneResponse.new,
    idempotency: connect.Idempotency.noSideEffects,
  );
}
