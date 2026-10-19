//
//  Generated code. Do not modify.
//  source: protectorate/v1/sleeve.proto
//

import "package:connectrpc/connect.dart" as connect;
import "sleeve.pb.dart" as protectoratev1sleeve;
import "sleeve.connect.spec.dart" as specs;

/// Manages sleeves.
extension type SleeveServiceClient (connect.Transport _transport) {
  /// List returns all sleeves.
  Future<protectoratev1sleeve.ListResponse> list(
    protectoratev1sleeve.ListRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.SleeveService.list,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  /// Spawn creates a sleeve running the given harness.
  Future<protectoratev1sleeve.SpawnResponse> spawn(
    protectoratev1sleeve.SpawnRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.SleeveService.spawn,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }
}
