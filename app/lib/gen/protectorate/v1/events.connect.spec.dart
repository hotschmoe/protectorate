//
//  Generated code. Do not modify.
//  source: protectorate/v1/events.proto
//

import "package:connectrpc/connect.dart" as connect;
import "events.pb.dart" as protectoratev1events;

abstract final class EventService {
  /// Fully-qualified name of the EventService service.
  static const name = 'protectorate.v1.EventService';

  static const subscribe = connect.Spec(
    '/$name/Subscribe',
    connect.StreamType.server,
    protectoratev1events.SubscribeRequest.new,
    protectoratev1events.Event.new,
  );
}
