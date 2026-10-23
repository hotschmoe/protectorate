//
//  Generated code. Do not modify.
//  source: protectorate/v1/approval.proto
//

import "package:connectrpc/connect.dart" as connect;
import "approval.pb.dart" as protectoratev1approval;

/// ApprovalService lists and decides the requests the hub opens instead of
/// running a high-impact call made by Envoy, a sleeve or a grant Envoy
/// minted: an RPC marked destructive, minting an admin grant, an admin job,
/// and launching a sleeve whose harness executable or profile changed since
/// its last launch. Agents never reach care nodes; placement refuses them.
/// Calls by the owner and approved devices are never gated. The caller gets
/// an aborted error carrying the request id; repeating the same call with the
/// protectorate-approval header set to that id runs it once approved, and
/// fails once denied or expired. The protectorate CLI and MCP bridge repeat
/// it themselves. Requests expire after ten minutes and live only in hub
/// memory. Changes are announced as "approval.requested" and
/// "approval.decided" (subject = request id). Only the owner and approved
/// devices may call this service.
abstract final class ApprovalService {
  /// Fully-qualified name of the ApprovalService service.
  static const name = 'protectorate.v1.ApprovalService';

  /// ListApprovals returns pending requests, oldest first, or with all every
  /// request the hub still remembers.
  static const listApprovals = connect.Spec(
    '/$name/ListApprovals',
    connect.StreamType.unary,
    protectoratev1approval.ListApprovalsRequest.new,
    protectoratev1approval.ListApprovalsResponse.new,
    idempotency: connect.Idempotency.noSideEffects,
  );

  /// DecideApproval approves or denies a pending request. Each decision is
  /// audited.
  static const decideApproval = connect.Spec(
    '/$name/DecideApproval',
    connect.StreamType.unary,
    protectoratev1approval.DecideApprovalRequest.new,
    protectoratev1approval.DecideApprovalResponse.new,
  );
}
