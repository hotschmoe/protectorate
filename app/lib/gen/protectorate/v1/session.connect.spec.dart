//
//  Generated code. Do not modify.
//  source: protectorate/v1/session.proto
//

import "package:connectrpc/connect.dart" as connect;
import "session.pb.dart" as protectoratev1session;

/// SessionService manages tmux-backed harness sessions. Interactive access is
/// the WebSocket terminal at GET /ws/terminal/{name}, not an RPC.
abstract final class SessionService {
  /// Fully-qualified name of the SessionService service.
  static const name = 'protectorate.v1.SessionService';

  static const listSessions = connect.Spec(
    '/$name/ListSessions',
    connect.StreamType.unary,
    protectoratev1session.ListSessionsRequest.new,
    protectoratev1session.ListSessionsResponse.new,
  );

  static const createSession = connect.Spec(
    '/$name/CreateSession',
    connect.StreamType.unary,
    protectoratev1session.CreateSessionRequest.new,
    protectoratev1session.CreateSessionResponse.new,
  );

  static const killSession = connect.Spec(
    '/$name/KillSession',
    connect.StreamType.unary,
    protectoratev1session.KillSessionRequest.new,
    protectoratev1session.KillSessionResponse.new,
  );

  static const capturePane = connect.Spec(
    '/$name/CapturePane',
    connect.StreamType.unary,
    protectoratev1session.CapturePaneRequest.new,
    protectoratev1session.CapturePaneResponse.new,
  );
}
