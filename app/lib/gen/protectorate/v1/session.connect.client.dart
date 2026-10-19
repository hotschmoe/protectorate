//
//  Generated code. Do not modify.
//  source: protectorate/v1/session.proto
//

import "package:connectrpc/connect.dart" as connect;
import "session.pb.dart" as protectoratev1session;
import "session.connect.spec.dart" as specs;

/// SessionService manages tmux-backed harness sessions. Interactive access is
/// the WebSocket terminal at GET /ws/terminal/{name}, not an RPC.
/// When a session's command ends, the session stays in state EXITED with its
/// last screen, exit code and exit signal until it is killed, respawned, or
/// removed by the hub's dead-session retention sweep (--dead-retention).
/// Changes are announced on EventService: "session.created", "session.exited"
/// (command ended; payload has exit_code, exit_signal and exited_at) and
/// "session.removed" (session gone). The WebSocket terminal attaches to an
/// exited session read-only and sends a notice frame saying so.
extension type SessionServiceClient (connect.Transport _transport) {
  /// ListSessions returns all sessions, running and exited.
  Future<protectoratev1session.ListSessionsResponse> listSessions(
    protectoratev1session.ListSessionsRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.SessionService.listSessions,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  /// CreateSession starts a detached tmux session running command, or the
  /// default shell when command is empty. Names starting with "sleeve-" are
  /// reserved for sleeves and fail with FAILED_PRECONDITION. The command runs
  /// unsandboxed as the hub's OS user, so only the owner token may call it.
  Future<protectoratev1session.CreateSessionResponse> createSession(
    protectoratev1session.CreateSessionRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.SessionService.createSession,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  /// KillSession terminates a session and every process in it. It also removes
  /// an exited session, discarding its final screen.
  Future<protectoratev1session.KillSessionResponse> killSession(
    protectoratev1session.KillSessionRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.SessionService.killSession,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  /// RespawnSession restarts an exited session with its original command and
  /// working directory. The previous output stays in scrollback. It fails with
  /// FAILED_PRECONDITION while the session is still running, and for sessions
  /// named "sleeve-*", which only SleeveService.SpawnSleeve may restart. It
  /// reruns an unsandboxed command, so only the owner token may call it.
  Future<protectoratev1session.RespawnSessionResponse> respawnSession(
    protectoratev1session.RespawnSessionRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.SessionService.respawnSession,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  /// CapturePane returns the current text of a session's active pane. It also
  /// works on exited sessions, which is how a crash is diagnosed. Include at
  /// least one history line to see the whole final screen of an exited session.
  Future<protectoratev1session.CapturePaneResponse> capturePane(
    protectoratev1session.CapturePaneRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.SessionService.capturePane,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }
}
