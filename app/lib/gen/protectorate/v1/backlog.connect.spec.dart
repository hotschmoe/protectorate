//
//  Generated code. Do not modify.
//  source: protectorate/v1/backlog.proto
//

import "package:connectrpc/connect.dart" as connect;
import "backlog.pb.dart" as protectoratev1backlog;

/// BacklogService fronts the markdown file at $PROTECTORATE_HOME/backlog.md.
/// Mutations publish backlog.* events on EventService.
abstract final class BacklogService {
  /// Fully-qualified name of the BacklogService service.
  static const name = 'protectorate.v1.BacklogService';

  /// ListItems returns backlog items, optionally filtered by project, state
  /// and harness.
  static const listItems = connect.Spec(
    '/$name/ListItems',
    connect.StreamType.unary,
    protectoratev1backlog.ListItemsRequest.new,
    protectoratev1backlog.ListItemsResponse.new,
    idempotency: connect.Idempotency.noSideEffects,
  );

  /// AddItem appends a new TODO item to a project and returns it with its id.
  static const addItem = connect.Spec(
    '/$name/AddItem',
    connect.StreamType.unary,
    protectoratev1backlog.AddItemRequest.new,
    protectoratev1backlog.AddItemResponse.new,
  );

  /// UpdateItem sets an item's state, claiming or releasing it.
  static const updateItem = connect.Spec(
    '/$name/UpdateItem',
    connect.StreamType.unary,
    protectoratev1backlog.UpdateItemRequest.new,
    protectoratev1backlog.UpdateItemResponse.new,
  );
}
