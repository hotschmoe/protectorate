//
//  Generated code. Do not modify.
//  source: protectorate/v1/session.proto
//

import "package:connectrpc/connect.dart" as connect;
import "session.pb.dart" as protectoratev1session;
import "session.connect.spec.dart" as specs;

/// SessionService manages tmux-backed harness sessions. Interactive access is
/// the WebSocket terminal at GET /ws/terminal/{name}, not an RPC.
extension type SessionServiceClient (connect.Transport _transport) {
  /// ListSessions returns all live sessions.
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
  /// default shell when command is empty.
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

  /// KillSession terminates a session and every process in it.
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

  /// CapturePane returns the current text of a session's active pane.
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
