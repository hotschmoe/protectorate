//
//  Generated code. Do not modify.
//  source: protectorate/v1/approval.proto
//

import "package:connectrpc/connect.dart" as connect;
import "approval.pb.dart" as protectoratev1approval;
import "approval.connect.spec.dart" as specs;

/// ApprovalService lists and decides the requests the hub opens instead of
/// running a high-impact call made by Envoy, a sleeve or a grant Envoy
/// minted: an RPC marked destructive, and launching a sleeve whose harness
/// executable or profile changed since its last launch.
/// Calls by the owner and approved devices are never gated. The caller gets
/// an aborted error carrying the request id; repeating the same call with the
/// protectorate-approval header set to that id runs it once approved, and
/// fails once denied or expired. The protectorate CLI and MCP bridge repeat
/// it themselves. Requests expire after ten minutes and live only in hub
/// memory. Changes are announced as "approval.requested" and
/// "approval.decided" (subject = request id). Only the owner and approved
/// devices may call this service.
extension type ApprovalServiceClient (connect.Transport _transport) {
  /// ListApprovals returns pending requests, oldest first, or with all every
  /// request the hub still remembers.
  Future<protectoratev1approval.ListApprovalsResponse> listApprovals(
    protectoratev1approval.ListApprovalsRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.ApprovalService.listApprovals,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  /// DecideApproval approves or denies a pending request. Each decision is
  /// audited.
  Future<protectoratev1approval.DecideApprovalResponse> decideApproval(
    protectoratev1approval.DecideApprovalRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.ApprovalService.decideApproval,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }
}
