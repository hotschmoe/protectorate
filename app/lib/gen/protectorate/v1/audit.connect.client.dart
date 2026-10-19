//
//  Generated code. Do not modify.
//  source: protectorate/v1/audit.proto
//

import "package:connectrpc/connect.dart" as connect;
import "audit.pb.dart" as protectoratev1audit;
import "audit.connect.spec.dart" as specs;

/// AuditService reads the hub's audit log (docs/auth.md): every mutating call
/// and terminal attach, with who made it and how it ended. Only the owner
/// token may call it.
extension type AuditServiceClient (connect.Transport _transport) {
  /// ListAuditEntries returns entries newest first, one page at a time. A page
  /// reads a bounded part of the log, so a filter that matches rarely can come
  /// back short, or empty, with a next_page_token to continue.
  Future<protectoratev1audit.ListAuditEntriesResponse> listAuditEntries(
    protectoratev1audit.ListAuditEntriesRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.AuditService.listAuditEntries,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }
}
