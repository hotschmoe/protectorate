//
//  Generated code. Do not modify.
//  source: protectorate/v1/tailnet.proto
//

import "package:connectrpc/connect.dart" as connect;
import "tailnet.pb.dart" as protectoratev1tailnet;

/// TailnetService exposes the hub's own view of the tailnet for debugging.
abstract final class TailnetService {
  /// Fully-qualified name of the TailnetService service.
  static const name = 'protectorate.v1.TailnetService';

  /// GetStatus returns the hub's tailnet identity and every peer it knows.
  /// FAILED_PRECONDITION when the tailnet is not enabled on this hub.
  static const getStatus = connect.Spec(
    '/$name/GetStatus',
    connect.StreamType.unary,
    protectoratev1tailnet.TailnetServiceGetStatusRequest.new,
    protectoratev1tailnet.TailnetServiceGetStatusResponse.new,
    idempotency: connect.Idempotency.noSideEffects,
  );
}
