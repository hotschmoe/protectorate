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

  /// Suggest ranks the harnesses a sleeve can run (claude, codex, grok and
  /// pi) by usable spare capacity before reset, best first. Only FRESH quota
  /// data counts as capacity: STALE and UNKNOWN providers are listed as not
  /// recommended. A harness scores the capacity that is usable now (the
  /// tightest window) and expected to expire unused at a reset, weighted
  /// toward the sooner reset. pi on the local LLM has no quota and is always
  /// recommended as the fallback, ranked below any subscription with real
  /// capacity to burn. Antigravity has no sleeve harness and is not listed.
  static const suggest = connect.Spec(
    '/$name/Suggest',
    connect.StreamType.unary,
    protectoratev1quota.SuggestRequest.new,
    protectoratev1quota.SuggestResponse.new,
    idempotency: connect.Idempotency.noSideEffects,
  );
}
