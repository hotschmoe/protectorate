//
//  Generated code. Do not modify.
//  source: protectorate/v1/message.proto
//

import "package:connectrpc/connect.dart" as connect;
import "message.pb.dart" as protectoratev1message;
import "message.connect.spec.dart" as specs;

/// MessageService is the hub's mailbox between sleeves, the orchestrator and
/// the owner. The sender of a message is the authenticated caller (a sleeve's
/// own token names the sleeve), never a claimed name. Messages are kept on the
/// hub and survive restarts.
/// Delivery is deterministic hub code. A message to a sleeve in chat mode is
/// sent into its chat as an ordinary user message ("from" = "sleeve:<name>")
/// at its next IDLE, never during a running turn; the text says how to reply
/// and ack. A sleeve in terminal mode gets no injection and reads its Inbox.
/// A project address resolves to the sleeves currently working on that project;
/// with none live the message waits, and a request also becomes a backlog note.
/// An ack_required message not acked within the timeout (default 30 minutes)
/// is delivered once more, then escalated to the orchestrator.
/// Changes are announced on EventService as "msg.sent", "msg.delivered",
/// "msg.acked", "msg.reminded" and "msg.escalated" (subject = message id).
extension type MessageServiceClient (connect.Transport _transport) {
  /// Wait blocks until a reply or ack to an outgoing message, or incoming mail
  /// or an outgoing ack in a thread. Existing outcomes return immediately.
  Future<protectoratev1message.MessageServiceWaitResponse> wait(
    protectoratev1message.MessageServiceWaitRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.MessageService.wait,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  /// Send addresses a message to a sleeve name, "project:<id-or-name>",
  /// "orchestrator" or "owner". Retrying with the same request_id returns the
  /// original message instead of sending twice.
  Future<protectoratev1message.MessageServiceSendResponse> send(
    protectoratev1message.MessageServiceSendRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.MessageService.send,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  /// Inbox lists the messages addressed to the caller, oldest first, and marks
  /// them read unless peek is set. The owner may read another mailbox with as.
  Future<protectoratev1message.InboxResponse> inbox(
    protectoratev1message.InboxRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.MessageService.inbox,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  /// Ack confirms a message. It stops the ack timer.
  Future<protectoratev1message.AckResponse> ack(
    protectoratev1message.AckRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.MessageService.ack,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  /// Reply answers a message in the same thread, addressed to its sender, and
  /// acks the original.
  Future<protectoratev1message.ReplyResponse> reply(
    protectoratev1message.ReplyRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.MessageService.reply,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  /// Thread returns every message of a thread in the order sent.
  Future<protectoratev1message.ThreadResponse> thread(
    protectoratev1message.ThreadRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.MessageService.thread,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  /// ListThreads summarizes threads, most recently active first: who takes
  /// part, the last message, and what is still open. Unread counts are the
  /// caller's own.
  Future<protectoratev1message.ListThreadsResponse> listThreads(
    protectoratev1message.ListThreadsRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.MessageService.listThreads,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }
}
