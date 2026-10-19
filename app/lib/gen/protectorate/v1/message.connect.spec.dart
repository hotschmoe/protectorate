//
//  Generated code. Do not modify.
//  source: protectorate/v1/message.proto
//

import "package:connectrpc/connect.dart" as connect;
import "message.pb.dart" as protectoratev1message;

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
abstract final class MessageService {
  /// Fully-qualified name of the MessageService service.
  static const name = 'protectorate.v1.MessageService';

  /// Send addresses a message to a sleeve name, "project:<id-or-name>",
  /// "orchestrator" or "owner". Retrying with the same request_id returns the
  /// original message instead of sending twice.
  static const send = connect.Spec(
    '/$name/Send',
    connect.StreamType.unary,
    protectoratev1message.MessageServiceSendRequest.new,
    protectoratev1message.MessageServiceSendResponse.new,
  );

  /// Inbox lists the messages addressed to the caller, oldest first, and marks
  /// them read unless peek is set. The owner may read another mailbox with as.
  static const inbox = connect.Spec(
    '/$name/Inbox',
    connect.StreamType.unary,
    protectoratev1message.InboxRequest.new,
    protectoratev1message.InboxResponse.new,
  );

  /// Ack confirms a message. It stops the ack timer.
  static const ack = connect.Spec(
    '/$name/Ack',
    connect.StreamType.unary,
    protectoratev1message.AckRequest.new,
    protectoratev1message.AckResponse.new,
  );

  /// Reply answers a message in the same thread, addressed to its sender, and
  /// acks the original.
  static const reply = connect.Spec(
    '/$name/Reply',
    connect.StreamType.unary,
    protectoratev1message.ReplyRequest.new,
    protectoratev1message.ReplyResponse.new,
  );

  /// Thread returns every message of a thread in the order sent.
  static const thread = connect.Spec(
    '/$name/Thread',
    connect.StreamType.unary,
    protectoratev1message.ThreadRequest.new,
    protectoratev1message.ThreadResponse.new,
    idempotency: connect.Idempotency.noSideEffects,
  );

  /// ListThreads summarizes threads, most recently active first: who takes
  /// part, the last message, and what is still open. Unread counts are the
  /// caller's own.
  static const listThreads = connect.Spec(
    '/$name/ListThreads',
    connect.StreamType.unary,
    protectoratev1message.ListThreadsRequest.new,
    protectoratev1message.ListThreadsResponse.new,
    idempotency: connect.Idempotency.noSideEffects,
  );
}
