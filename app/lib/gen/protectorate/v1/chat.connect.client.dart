//
//  Generated code. Do not modify.
//  source: protectorate/v1/chat.proto
//

import "package:connectrpc/connect.dart" as connect;
import "chat.pb.dart" as protectoratev1chat;
import "chat.connect.spec.dart" as specs;

/// ChatService drives a sleeve in chat mode: the harness runs its own official
/// headless protocol inside the sleeve and the hub normalizes what it emits
/// into ChatEvents. The hub keeps each sleeve's events in an append-only log,
/// so any number of clients can watch and a reconnecting client resumes from
/// the last seq it saw. The harness's own session store stays the full record;
/// the log is the view. Only one driver owns a native session at a time, so a
/// sleeve is either in chat mode or in terminal mode (SetMode switches, keeping
/// the native session id).
extension type ChatServiceClient (connect.Transport _transport) {
  /// Watch replays the sleeve's events after after_seq, then streams new ones
  /// until the client disconnects or the sleeve is killed.
  Stream<protectoratev1chat.ChatEvent> watch(
    protectoratev1chat.WatchRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).server(
      specs.ChatService.watch,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  /// Send delivers a user message. It starts a turn when the harness is idle;
  /// during a turn it steers that turn on harnesses that support it and fails
  /// with failed_precondition on the rest. Retrying with the same request_id
  /// returns the original result instead of sending twice.
  Future<protectoratev1chat.SendResponse> send(
    protectoratev1chat.SendRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.ChatService.send,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  /// Interrupt cancels the running turn. It is a no-op when the harness is idle.
  Future<protectoratev1chat.InterruptResponse> interrupt(
    protectoratev1chat.InterruptRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.ChatService.interrupt,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  /// Respond answers a pending approval.
  Future<protectoratev1chat.RespondResponse> respond(
    protectoratev1chat.RespondRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.ChatService.respond,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  /// Compact runs the harness's own native compaction between turns. It fails
  /// with failed_precondition while a turn is running or an approval is
  /// pending, and returns after the harness has confirmed the compaction.
  Future<protectoratev1chat.CompactResponse> compact(
    protectoratev1chat.CompactRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.ChatService.compact,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  /// SetMode switches a sleeve between chat and terminal ("take over"). The
  /// current driver is closed first, then the other mode resumes the same
  /// native session id.
  Future<protectoratev1chat.SetModeResponse> setMode(
    protectoratev1chat.SetModeRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.ChatService.setMode,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }
}
