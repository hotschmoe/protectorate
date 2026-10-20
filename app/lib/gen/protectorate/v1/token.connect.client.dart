//
//  Generated code. Do not modify.
//  source: protectorate/v1/token.proto
//

import "package:connectrpc/connect.dart" as connect;
import "token.pb.dart" as protectoratev1token;
import "token.connect.spec.dart" as specs;

/// TokenService issues and revokes scoped API tokens. Only the owner token (the
/// one in $PROTECTORATE_HOME/token) may call it; no scope grants access.
extension type TokenServiceClient (connect.Transport _transport) {
  /// CreateToken issues a scoped token. The secret is returned once and only
  /// its SHA-256 hash is stored, so it cannot be recovered later.
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

  /// ListTokens returns every issued token, including revoked and expired
  /// ones, without secrets.
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

  /// RevokeToken permanently disables a token. Revoking an already revoked
  /// token succeeds and keeps the original revocation time.
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
}
/// GrantService mints task-scoped admin grants: short-lived tokens whose
/// scopes are job_* and node_admin:<node>. The owner may grant any node (or a
/// glob over node names); the orchestrator's token may grant one shared node
/// by name, and only when the hub runs with --orchestrator-admin-grants. Every
/// call made with a grant is audited with its task.
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
