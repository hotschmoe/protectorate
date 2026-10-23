//
//  Generated code. Do not modify.
//  source: protectorate/v1/token.proto
//

import "package:connectrpc/connect.dart" as connect;
import "token.pb.dart" as protectoratev1token;
import "token.connect.spec.dart" as specs;

/// TokenService manages capabilities: short-lived macaroons only the hub can
/// mint and verify. A capability names its principal, scopes and expiry in
/// caveats; its holder may append caveats to narrow it, never to widen it.
/// The hub keeps one record per lineage so it can list and revoke it.
extension type TokenServiceClient (connect.Transport _transport) {
  /// CreateToken mints a capability for a script or tool. The secret is
  /// returned once and is not stored.
  Future<protectoratev1token.CreateTokenResponse> createToken(
    protectoratev1token.CreateTokenRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.TokenService.createToken,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  /// ListTokens returns every capability lineage, including revoked and
  /// expired ones, without secrets.
  Future<protectoratev1token.ListTokensResponse> listTokens(
    protectoratev1token.ListTokensRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.TokenService.listTokens,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  /// RevokeToken permanently disables a lineage: every token minted, renewed
  /// or narrowed from it. Revoking an already revoked lineage succeeds and
  /// keeps the original revocation time.
  Future<protectoratev1token.RevokeTokenResponse> revokeToken(
    protectoratev1token.RevokeTokenRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.TokenService.revokeToken,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  /// RenewToken returns a fresh token for the capability that authenticates
  /// the call: the same caveats, including any its holder appended, with a
  /// new expiry. Only a capability that has not expired may call it.
  Future<protectoratev1token.RenewTokenResponse> renewToken(
    protectoratev1token.RenewTokenRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.TokenService.renewToken,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }
}
/// GrantService mints task-scoped admin grants: capabilities whose scopes are
/// job_* and node_admin:<node>. The owner may grant any node (or a glob over
/// node names); the orchestrator may grant one shared node by name, and only
/// when the hub runs with --orchestrator-admin-grants. Every call made with a
/// grant is audited with its task.
extension type GrantServiceClient (connect.Transport _transport) {
  /// Admin mints a grant. The secret is returned once.
  Future<protectoratev1token.GrantServiceAdminResponse> admin(
    protectoratev1token.GrantServiceAdminRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.GrantService.admin,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }
}
