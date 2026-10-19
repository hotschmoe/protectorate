//
//  Generated code. Do not modify.
//  source: protectorate/v1/backlog.proto
//

import "package:connectrpc/connect.dart" as connect;
import "backlog.pb.dart" as protectoratev1backlog;
import "backlog.connect.spec.dart" as specs;

/// BacklogService fronts the markdown file at $PROTECTORATE_HOME/backlog.md.
/// Mutations publish backlog.* events on EventService.
extension type BacklogServiceClient (connect.Transport _transport) {
  /// ListItems returns backlog items, optionally filtered by project, state
  /// and harness.
  Future<protectoratev1backlog.ListItemsResponse> listItems(
    protectoratev1backlog.ListItemsRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.BacklogService.listItems,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  /// AddItem appends a new TODO item to a project and returns it with its id.
  Future<protectoratev1backlog.AddItemResponse> addItem(
    protectoratev1backlog.AddItemRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.BacklogService.addItem,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  /// UpdateItem sets an item's state, claiming or releasing it.
  Future<protectoratev1backlog.UpdateItemResponse> updateItem(
    protectoratev1backlog.UpdateItemRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.BacklogService.updateItem,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }
}
