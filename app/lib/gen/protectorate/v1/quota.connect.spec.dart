//
//  Generated code. Do not modify.
//  source: protectorate/v1/quota.proto
//

import "package:connectrpc/connect.dart" as connect;
import "quota.pb.dart" as protectoratev1quota;

/// QuotaService reports subscription usage. Updates are also announced on
/// EventService as events with kind "quota.updated" (subject = provider,
/// payload = the ProviderQuota in proto-JSON form).
abstract final class QuotaService {
  /// Fully-qualified name of the QuotaService service.
  static const name = 'protectorate.v1.QuotaService';

  /// GetQuota returns the usage windows, resets and freshness of every
  /// provider subscription (Claude, Codex, Grok, Antigravity, local LLMs).
  static const getQuota = connect.Spec(
    '/$name/GetQuota',
    connect.StreamType.unary,
    protectoratev1quota.GetQuotaRequest.new,
    protectoratev1quota.GetQuotaResponse.new,
    idempotency: connect.Idempotency.noSideEffects,
  );
}
