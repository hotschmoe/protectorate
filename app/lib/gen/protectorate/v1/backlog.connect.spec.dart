//
//  Generated code. Do not modify.
//  source: protectorate/v1/backlog.proto
//

import "package:connectrpc/connect.dart" as connect;
import "backlog.pb.dart" as protectoratev1backlog;

/// BacklogService keeps the owner's notes on the hub (docs/backlog.md). The
/// owner writes them in the app or CLI; the orchestrator reads and edits them
/// through the same API. Mutations publish note.* events on EventService.
abstract final class BacklogService {
  /// Fully-qualified name of the BacklogService service.
  static const name = 'protectorate.v1.BacklogService';

  /// ListNotes returns notes, pinned first and then most recently updated,
  /// optionally filtered by project tag and a text query.
  static const listNotes = connect.Spec(
    '/$name/ListNotes',
    connect.StreamType.unary,
    protectoratev1backlog.ListNotesRequest.new,
    protectoratev1backlog.ListNotesResponse.new,
    idempotency: connect.Idempotency.noSideEffects,
  );

  /// GetNote returns one note by id.
  static const getNote = connect.Spec(
    '/$name/GetNote',
    connect.StreamType.unary,
    protectoratev1backlog.GetNoteRequest.new,
    protectoratev1backlog.GetNoteResponse.new,
    idempotency: connect.Idempotency.noSideEffects,
  );

  /// CreateNote adds a note and returns it with its id.
  static const createNote = connect.Spec(
    '/$name/CreateNote',
    connect.StreamType.unary,
    protectoratev1backlog.CreateNoteRequest.new,
    protectoratev1backlog.CreateNoteResponse.new,
  );

  /// UpdateNote changes the fields that are set and leaves the others alone.
  static const updateNote = connect.Spec(
    '/$name/UpdateNote',
    connect.StreamType.unary,
    protectoratev1backlog.UpdateNoteRequest.new,
    protectoratev1backlog.UpdateNoteResponse.new,
  );

  /// DeleteNote removes a note for good.
  static const deleteNote = connect.Spec(
    '/$name/DeleteNote',
    connect.StreamType.unary,
    protectoratev1backlog.DeleteNoteRequest.new,
    protectoratev1backlog.DeleteNoteResponse.new,
  );
}
