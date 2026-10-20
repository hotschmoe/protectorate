//
//  Generated code. Do not modify.
//  source: protectorate/v1/release.proto
//

import "package:connectrpc/connect.dart" as connect;
import "release.pb.dart" as protectoratev1release;
import "release.connect.spec.dart" as specs;

extension type ReleaseServiceClient (connect.Transport _transport) {
  Future<protectoratev1release.ReleaseInfo> checkRelease(
    protectoratev1release.CheckReleaseRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.ReleaseService.checkRelease,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  Stream<protectoratev1release.UpdateHubProgress> updateHub(
    protectoratev1release.UpdateHubRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).server(
      specs.ReleaseService.updateHub,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }
}
