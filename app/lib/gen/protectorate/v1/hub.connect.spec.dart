//
//  Generated code. Do not modify.
//  source: protectorate/v1/hub.proto
//

import "package:connectrpc/connect.dart" as connect;
import "hub.pb.dart" as protectoratev1hub;

abstract final class HubService {
  /// Fully-qualified name of the HubService service.
  static const name = 'protectorate.v1.HubService';

  static const getStatus = connect.Spec(
    '/$name/GetStatus',
    connect.StreamType.unary,
    protectoratev1hub.GetStatusRequest.new,
    protectoratev1hub.GetStatusResponse.new,
  );
}
