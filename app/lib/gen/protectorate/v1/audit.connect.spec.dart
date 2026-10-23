//
//  Generated code. Do not modify.
//  source: protectorate/v1/audit.proto
//

import "package:connectrpc/connect.dart" as connect;
import "audit.pb.dart" as protectoratev1audit;

/// AuditService reads the hub's audit log (docs/auth.md): every mutating call
/// and terminal attach, with who made it and how it ended. Each entry carries
/// the hash of the one before it, and the hub records the newest hash outside
/// the log, so an edited, removed or truncated entry breaks the chain. Only
/// the owner token may call it.
abstract final class AuditService {
  /// Fully-qualified name of the AuditService service.
  static const name = 'protectorate.v1.AuditService';

  /// ListAuditEntries returns entries newest first, one page at a time. A page
  /// reads a bounded part of the log, so a filter that matches rarely can come
  /// back short, or empty, with a next_page_token to continue.
  static const listAuditEntries = connect.Spec(
    '/$name/ListAuditEntries',
    connect.StreamType.unary,
    protectoratev1audit.ListAuditEntriesRequest.new,
    protectoratev1audit.ListAuditEntriesResponse.new,
    idempotency: connect.Idempotency.noSideEffects,
  );

  /// VerifyAudit walks the whole chain from the first entry and checks it
  /// against the recorded head. A break is reported, not raised.
  static const verifyAudit = connect.Spec(
    '/$name/VerifyAudit',
    connect.StreamType.unary,
    protectoratev1audit.VerifyAuditRequest.new,
    protectoratev1audit.VerifyAuditResponse.new,
    idempotency: connect.Idempotency.noSideEffects,
  );
}
