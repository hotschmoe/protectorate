//
//  Generated code. Do not modify.
//  source: protectorate/v1/backlog.proto
//

import "package:connectrpc/connect.dart" as connect;
import "backlog.pb.dart" as protectoratev1backlog;
import "backlog.connect.spec.dart" as specs;

/// BacklogService keeps the owner's notes on the hub (docs/backlog.md). The
/// owner writes them in the app or CLI; the orchestrator reads and edits them
/// through the same API. Mutations publish note.* events on EventService.
extension type BacklogServiceClient (connect.Transport _transport) {
  /// ListNotes returns notes, pinned first and then most recently updated,
  /// open by default, optionally filtered by project tag and a text query.
  /// Set state to "done" for completed notes or "all" for both states.
  Future<protectoratev1backlog.ListNotesResponse> listNotes(
    protectoratev1backlog.ListNotesRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.BacklogService.listNotes,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  /// GetNote returns one note by id.
  Future<protectoratev1backlog.GetNoteResponse> getNote(
    protectoratev1backlog.GetNoteRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.BacklogService.getNote,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  /// CreateNote adds a note and returns it with its id.
  Future<protectoratev1backlog.CreateNoteResponse> createNote(
    protectoratev1backlog.CreateNoteRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.BacklogService.createNote,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  /// UpdateNote changes the fields that are set and leaves the others alone.
  /// Set state to "done" with an optional resolution line to close a note,
  /// or "open" to reopen it and clear completion. Requires backlog_update_note.
  Future<protectoratev1backlog.UpdateNoteResponse> updateNote(
    protectoratev1backlog.UpdateNoteRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.BacklogService.updateNote,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }

  /// DeleteNote removes a note for good.
  Future<protectoratev1backlog.DeleteNoteResponse> deleteNote(
    protectoratev1backlog.DeleteNoteRequest input, {
    connect.Headers? headers,
    connect.AbortSignal? signal,
    Function(connect.Headers)? onHeader,
    Function(connect.Headers)? onTrailer,
  }) {
    return connect.Client(_transport).unary(
      specs.BacklogService.deleteNote,
      input,
      signal: signal,
      headers: headers,
      onHeader: onHeader,
      onTrailer: onTrailer,
    );
  }
}
