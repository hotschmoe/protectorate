//
//  Generated code. Do not modify.
//  source: protectorate/v1/events.proto
//

import "package:connectrpc/connect.dart" as connect;
import "events.pb.dart" as protectoratev1events;
import "events.connect.spec.dart" as specs;

/// EventService streams hub events. It is server-streaming only, so the MCP
/// bridge does not expose it as a tool.
extension type EventServiceClient(connect.Transport _transport) {
  /// Subscribe streams every event the hub publishes until the client disconnects.
  Stream<protectoratev1events.Event> subscribe(
    protectoratev1events.SubscribeRequest input, {
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
