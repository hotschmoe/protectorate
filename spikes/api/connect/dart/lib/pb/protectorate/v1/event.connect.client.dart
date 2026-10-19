//
//  Generated code. Do not modify.
//  source: protectorate/v1/event.proto
//

import "package:connectrpc/connect.dart" as connect;
import "event.pb.dart" as protectoratev1event;
import "event.connect.spec.dart" as specs;

extension type EventServiceClient (connect.Transport _transport) {
  /// Subscribe streams events until the client disconnects.
  Stream<protectoratev1event.Event> subscribe(
    protectoratev1event.SubscribeRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).server(
      specs.EventService.subscribe,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }
}
