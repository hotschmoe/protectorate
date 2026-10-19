//
//  Generated code. Do not modify.
//  source: protectorate/v1/quota.proto
//

import "package:connectrpc/connect.dart" as connect;
import "quota.pb.dart" as protectoratev1quota;
import "quota.connect.spec.dart" as specs;

/// QuotaService reports subscription usage. Updates are also announced on
/// EventService as events with kind "quota.updated" (subject = provider,
/// payload = the ProviderQuota in proto-JSON form).
extension type QuotaServiceClient (connect.Transport _transport) {
  /// GetQuota returns the usage windows, resets and freshness of every
  /// provider subscription (Claude, Codex, Grok, Antigravity, local LLMs).
  Future<protectoratev1quota.GetQuotaResponse> getQuota(
    protectoratev1quota.GetQuotaRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.QuotaService.getQuota,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  /// Suggest ranks the harnesses a sleeve can run (claude, codex, grok and
  /// pi) by usable spare capacity before reset, best first. Only FRESH quota
  /// data counts as capacity: STALE and UNKNOWN providers are listed as not
  /// recommended. A harness scores the capacity that is usable now (the
  /// tightest window) and expected to expire unused at a reset, weighted
  /// toward the sooner reset. pi on the local LLM has no quota and is always
  /// recommended as the fallback, ranked below any subscription with real
  /// capacity to burn. Antigravity has no sleeve harness and is not listed.
  Future<protectoratev1quota.SuggestResponse> suggest(
    protectoratev1quota.SuggestRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.QuotaService.suggest,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }
}
