//
//  Generated code. Do not modify.
//  source: protectorate/v1/event.proto
//

import "package:connectrpc/connect.dart" as connect;
import "event.pb.dart" as protectoratev1event;

abstract final class EventService {
  /// Fully-qualified name of the EventService service.
  static const name = 'protectorate.v1.EventService';

  /// Subscribe streams events until the client disconnects.
  static const subscribe = connect.Spec(
    '/$name/Subscribe',
    connect.StreamType.server,
    protectoratev1event.SubscribeRequest.new,
    protectoratev1event.Event.new,
  );
}
