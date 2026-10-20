//
//  Generated code. Do not modify.
//  source: protectorate/v1/hub.proto
//

import "package:connectrpc/connect.dart" as connect;
import "hub.pb.dart" as protectoratev1hub;
import "hub.connect.spec.dart" as specs;

/// HubService reports on the hub process itself.
extension type HubServiceClient (connect.Transport _transport) {
  /// GetStatus returns the hub version, host platform and uptime.
  Future<protectoratev1hub.GetStatusResponse> getStatus(
    protectoratev1hub.GetStatusRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.HubService.getStatus,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  /// GetSleeveRuntime reports the detected sandbox runtime and shared cache lower.
  Future<protectoratev1hub.GetSleeveRuntimeResponse> getSleeveRuntime(
    protectoratev1hub.GetSleeveRuntimeRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.HubService.getSleeveRuntime,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }
}
