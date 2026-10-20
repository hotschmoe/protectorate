//
//  Generated code. Do not modify.
//  source: protectorate/v1/hub.proto
//

import "package:connectrpc/connect.dart" as connect;
import "hub.pb.dart" as protectoratev1hub;

/// HubService reports on the hub process itself.
abstract final class HubService {
  /// Fully-qualified name of the HubService service.
  static const name = 'protectorate.v1.HubService';

  /// GetStatus returns the hub version, host platform and uptime.
  static const getStatus = connect.Spec(
    '/$name/GetStatus',
    connect.StreamType.unary,
    protectoratev1hub.GetStatusRequest.new,
    protectoratev1hub.GetStatusResponse.new,
    idempotency: connect.Idempotency.noSideEffects,
  );

  /// GetSleeveRuntime reports the detected sandbox runtime and shared cache lower.
  static const getSleeveRuntime = connect.Spec(
    '/$name/GetSleeveRuntime',
    connect.StreamType.unary,
    protectoratev1hub.GetSleeveRuntimeRequest.new,
    protectoratev1hub.GetSleeveRuntimeResponse.new,
    idempotency: connect.Idempotency.noSideEffects,
  );
}
