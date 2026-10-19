//
//  Generated code. Do not modify.
//  source: protectorate/v1/chat.proto
//

import "package:connectrpc/connect.dart" as connect;
import "chat.pb.dart" as protectoratev1chat;

/// ChatService drives a sleeve in chat mode: the harness runs its own official
/// headless protocol inside the sleeve and the hub normalizes what it emits
/// into ChatEvents. The hub keeps each sleeve's events in an append-only log,
/// so any number of clients can watch and a reconnecting client resumes from
/// the last seq it saw. The harness's own session store stays the full record;
/// the log is the view. A sleeve is in chat mode for life, or in terminal mode
/// and then has no chat.
abstract final class ChatService {
  /// Fully-qualified name of the ChatService service.
  static const name = 'protectorate.v1.ChatService';

  /// Watch replays the sleeve's events after after_seq, then streams new ones
  /// until the client disconnects or the sleeve is killed.
  static const watch = connect.Spec(
    '/$name/Watch',
    connect.StreamType.server,
    protectoratev1chat.WatchRequest.new,
    protectoratev1chat.ChatEvent.new,
    idempotency: connect.Idempotency.noSideEffects,
  );

  /// Send delivers a user message. It starts a turn when the harness is idle;
  /// during a turn it steers that turn on harnesses that support it and fails
  /// with failed_precondition on the rest. Retrying with the same request_id
  /// returns the original result instead of sending twice.
  static const send = connect.Spec(
    '/$name/Send',
    connect.StreamType.unary,
    protectoratev1chat.SendRequest.new,
    protectoratev1chat.SendResponse.new,
  );

  /// Interrupt cancels the running turn. It is a no-op when the harness is idle.
  static const interrupt = connect.Spec(
    '/$name/Interrupt',
    connect.StreamType.unary,
    protectoratev1chat.InterruptRequest.new,
    protectoratev1chat.InterruptResponse.new,
  );

  /// Respond answers a pending approval.
  static const respond = connect.Spec(
    '/$name/Respond',
    connect.StreamType.unary,
    protectoratev1chat.RespondRequest.new,
    protectoratev1chat.RespondResponse.new,
  );

  /// Compact runs the harness's own native compaction between turns. It fails
  /// with failed_precondition while a turn is running or an approval is
  /// pending, and returns after the harness has confirmed the compaction.
  static const compact = connect.Spec(
    '/$name/Compact',
    connect.StreamType.unary,
    protectoratev1chat.CompactRequest.new,
    protectoratev1chat.CompactResponse.new,
  );
}
