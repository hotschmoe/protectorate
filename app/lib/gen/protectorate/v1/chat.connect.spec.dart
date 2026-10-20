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
/// and then has only self-reported result events in this log.
abstract final class ChatService {
  /// Fully-qualified name of the ChatService service.
  static const name = 'protectorate.v1.ChatService';

  /// Watch replays the sleeve's events after after_seq, then streams new ones
  /// until the client disconnects or the sleeve is killed. Terminal sleeves
  /// expose their result history without starting a headless harness.
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

  /// Native catalog; no inference turn is started. Select by sleeve or harness.
  static const listModels = connect.Spec(
    '/$name/ListModels',
    connect.StreamType.unary,
    protectoratev1chat.ListModelsRequest.new,
    protectoratev1chat.ListModelsResponse.new,
    idempotency: connect.Idempotency.noSideEffects,
  );

  /// Replace the requested selection between turns, without restarting.
  /// Empty fields restore the captured native defaults. Unsupported drivers fail.
  static const setSelection = connect.Spec(
    '/$name/SetSelection',
    connect.StreamType.unary,
    protectoratev1chat.SetSelectionRequest.new,
    protectoratev1chat.SetSelectionResponse.new,
  );

  /// Report replaces this sleeve's result for its current brief. Only the
  /// sleeve's own identity token may report, never the owner or another sleeve.
  static const report = connect.Spec(
    '/$name/Report',
    connect.StreamType.unary,
    protectoratev1chat.ReportRequest.new,
    protectoratev1chat.ReportResponse.new,
  );

  /// Wait blocks on hub events until a sleeve is idle without pending mail,
  /// or has reported for its current brief. Exits and timeouts fail the call.
  /// Available as the read-only MCP tool chat_wait; no polling is needed.
  static const wait = connect.Spec(
    '/$name/Wait',
    connect.StreamType.unary,
    protectoratev1chat.WaitRequest.new,
    protectoratev1chat.WaitResponse.new,
    idempotency: connect.Idempotency.noSideEffects,
  );
}
