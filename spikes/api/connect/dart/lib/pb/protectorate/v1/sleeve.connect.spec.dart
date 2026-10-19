//
//  Generated code. Do not modify.
//  source: protectorate/v1/sleeve.proto
//

import "package:connectrpc/connect.dart" as connect;
import "sleeve.pb.dart" as protectoratev1sleeve;

/// Manages sleeves.
abstract final class SleeveService {
  /// Fully-qualified name of the SleeveService service.
  static const name = 'protectorate.v1.SleeveService';

  /// List returns all sleeves.
  static const list = connect.Spec(
    '/$name/List',
    connect.StreamType.unary,
    protectoratev1sleeve.ListRequest.new,
    protectoratev1sleeve.ListResponse.new,
    idempotency: connect.Idempotency.noSideEffects,
  );

  /// Spawn creates a sleeve running the given harness.
  static const spawn = connect.Spec(
    '/$name/Spawn',
    connect.StreamType.unary,
    protectoratev1sleeve.SpawnRequest.new,
    protectoratev1sleeve.SpawnResponse.new,
  );
}
