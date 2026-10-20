//
//  Generated code. Do not modify.
//  source: protectorate/v1/release.proto
//

import "package:connectrpc/connect.dart" as connect;
import "release.pb.dart" as protectoratev1release;

abstract final class ReleaseService {
  /// Fully-qualified name of the ReleaseService service.
  static const name = 'protectorate.v1.ReleaseService';

  static const checkRelease = connect.Spec(
    '/$name/CheckRelease',
    connect.StreamType.unary,
    protectoratev1release.CheckReleaseRequest.new,
    protectoratev1release.ReleaseInfo.new,
    idempotency: connect.Idempotency.noSideEffects,
  );

  static const updateHub = connect.Spec(
    '/$name/UpdateHub',
    connect.StreamType.server,
    protectoratev1release.UpdateHubRequest.new,
    protectoratev1release.UpdateHubProgress.new,
  );
}
