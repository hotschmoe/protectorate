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
}
