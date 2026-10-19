//
//  Generated code. Do not modify.
//  source: protectorate/v1/tailnet.proto
//

import "package:connectrpc/connect.dart" as connect;
import "tailnet.pb.dart" as protectoratev1tailnet;
import "tailnet.connect.spec.dart" as specs;

/// TailnetService exposes the hub's own view of the tailnet for debugging.
extension type TailnetServiceClient (connect.Transport _transport) {
  /// GetStatus returns the hub's tailnet identity and every peer it knows.
  /// FAILED_PRECONDITION when the tailnet is not enabled on this hub.
  Future<protectoratev1tailnet.TailnetServiceGetStatusResponse> getStatus(
    protectoratev1tailnet.TailnetServiceGetStatusRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.TailnetService.getStatus,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }
}
