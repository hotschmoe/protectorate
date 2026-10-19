//
//  Generated code. Do not modify.
//  source: protectorate/v1/token.proto
//

import "package:connectrpc/connect.dart" as connect;
import "token.pb.dart" as protectoratev1token;

/// TokenService issues and revokes scoped API tokens. Only the owner token (the
/// one in $PROTECTORATE_HOME/token) may call it; no scope grants access.
abstract final class TokenService {
  /// Fully-qualified name of the TokenService service.
  static const name = 'protectorate.v1.TokenService';

  /// CreateToken issues a scoped token. The secret is returned once and only
  /// its SHA-256 hash is stored, so it cannot be recovered later.
  static const createToken = connect.Spec(
    '/$name/CreateToken',
    connect.StreamType.unary,
    protectoratev1token.CreateTokenRequest.new,
    protectoratev1token.CreateTokenResponse.new,
  );

  /// ListTokens returns every issued token, including revoked and expired
  /// ones, without secrets.
  static const listTokens = connect.Spec(
    '/$name/ListTokens',
    connect.StreamType.unary,
    protectoratev1token.ListTokensRequest.new,
    protectoratev1token.ListTokensResponse.new,
    idempotency: connect.Idempotency.noSideEffects,
  );

  /// RevokeToken permanently disables a token. Revoking an already revoked
  /// token succeeds and keeps the original revocation time.
  static const revokeToken = connect.Spec(
    '/$name/RevokeToken',
    connect.StreamType.unary,
    protectoratev1token.RevokeTokenRequest.new,
    protectoratev1token.RevokeTokenResponse.new,
  );
}
