//
//  Generated code. Do not modify.
//  source: protectorate/v1/token.proto
//

import "package:connectrpc/connect.dart" as connect;
import "token.pb.dart" as protectoratev1token;

/// TokenService manages capabilities: short-lived macaroons only the hub can
/// mint and verify. A capability names its principal, scopes and expiry in
/// caveats; its holder may append caveats to narrow it, never to widen it.
/// The hub keeps one record per lineage so it can list and revoke it.
abstract final class TokenService {
  /// Fully-qualified name of the TokenService service.
  static const name = 'protectorate.v1.TokenService';

  /// CreateToken mints a capability for a script or tool. The secret is
  /// returned once and is not stored.
  static const createToken = connect.Spec(
    '/$name/CreateToken',
    connect.StreamType.unary,
    protectoratev1token.CreateTokenRequest.new,
    protectoratev1token.CreateTokenResponse.new,
  );

  /// ListTokens returns every capability lineage, including revoked and
  /// expired ones, without secrets.
  static const listTokens = connect.Spec(
    '/$name/ListTokens',
    connect.StreamType.unary,
    protectoratev1token.ListTokensRequest.new,
    protectoratev1token.ListTokensResponse.new,
    idempotency: connect.Idempotency.noSideEffects,
  );

  /// RevokeToken permanently disables a lineage: every token minted, renewed
  /// or narrowed from it. Revoking an already revoked lineage succeeds and
  /// keeps the original revocation time.
  static const revokeToken = connect.Spec(
    '/$name/RevokeToken',
    connect.StreamType.unary,
    protectoratev1token.RevokeTokenRequest.new,
    protectoratev1token.RevokeTokenResponse.new,
  );

  /// RenewToken returns a fresh token for the capability that authenticates
  /// the call: the same caveats, including any its holder appended, with a
  /// new expiry. Only a capability that has not expired may call it.
  static const renewToken = connect.Spec(
    '/$name/RenewToken',
    connect.StreamType.unary,
    protectoratev1token.RenewTokenRequest.new,
    protectoratev1token.RenewTokenResponse.new,
  );
}
/// GrantService mints task-scoped admin grants: capabilities whose scopes are
/// job_* and node_admin:<node>. The owner may grant any node (or a glob over
/// node names); the orchestrator may grant one shared node by name, and only
/// when the hub runs with --orchestrator-admin-grants. Every call made with a
/// grant is audited with its task.
abstract final class GrantService {
  /// Fully-qualified name of the GrantService service.
  static const name = 'protectorate.v1.GrantService';

  /// Admin mints a grant. The secret is returned once.
  static const admin = connect.Spec(
    '/$name/Admin',
    connect.StreamType.unary,
    protectoratev1token.GrantServiceAdminRequest.new,
    protectoratev1token.GrantServiceAdminResponse.new,
  );
}
