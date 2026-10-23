import 'dart:io' as io;

import 'package:connectrpc/connect.dart';
import 'package:connectrpc/io.dart' as rpcio;
import 'package:connectrpc/protobuf.dart';
import 'package:connectrpc/protocol/connect.dart' as protocol;

import 'package:protectorate/gen/protectorate/v1/backlog.connect.client.dart';
import 'package:protectorate/gen/protectorate/v1/backlog.pb.dart';
import 'package:protectorate/hubs/connection.dart';

// Uses the same generated Connect client as BacklogTab and NoteDraft. The
// Rust integration test supplies a disposable hub and owner credential.
Future<void> main(List<String> args) async {
  if (args.length != 2) throw ArgumentError('expected hub URL and owner token');
  final http = io.HttpClient()..connectionTimeout = const Duration(seconds: 8);
  try {
    final transport = protocol.Transport(
      baseUrl: args[0],
      codec: const JsonCodec(),
      httpClient: hubHttpClient(args[0], client: rpcio.createHttpClient(http)),
      interceptors: [
        <I extends Object, O extends Object>(AnyFn<I, O> next) =>
            (Request<I, O> request) {
              request.headers.set('authorization', ['Bearer ${args[1]}']);
              return next(request);
            },
      ],
    );
    final backlog = BacklogServiceClient(transport);
    void check(bool condition, String message) {
      if (!condition) throw StateError(message);
    }

    check(
      (await backlog.listNotes(ListNotesRequest(state: 'all'))).notes.isEmpty,
      'new hub backlog was not empty',
    );
    final created = (await backlog.createNote(
      CreateNoteRequest(
        title: 'Rust Flutter backlog trial',
        body: 'First draft',
        project: 'e2e/flutter',
        pinned: true,
      ),
    )).note;
    check(created.id.isNotEmpty && created.source == 'owner', 'create failed');
    check(created.state == 'open' && created.pinned, 'create lost fields');
    check(created.hasCreatedAt() && created.hasUpdatedAt(), 'missing dates');

    final open = await backlog.listNotes(ListNotesRequest());
    check(
      open.notes.length == 1 && open.notes.single.id == created.id,
      'open list missed the new note',
    );
    final byProject = await backlog.listNotes(
      ListNotesRequest(project: 'e2e/flutter', query: 'FIRST DRAFT'),
    );
    check(byProject.notes.length == 1, 'project or text filter failed');
    final otherProject = await backlog.listNotes(
      ListNotesRequest(project: 'e2e/other'),
    );
    check(otherProject.notes.isEmpty, 'project filter leaked a note');

    // NoteDraft sends only edited fields; false and empty string are edits.
    final edited = (await backlog.updateNote(
      UpdateNoteRequest(
        id: created.id,
        body: 'Second draft',
        pinned: false,
        project: '',
      ),
    )).note;
    check(
      edited.title == created.title &&
          edited.body == 'Second draft' &&
          !edited.pinned &&
          edited.project.isEmpty,
      'partial edit lost a field or did not clear a field',
    );
    final fetched = (await backlog.getNote(
      GetNoteRequest(id: created.id),
    )).note;
    check(fetched.body == edited.body, 'get did not reflect edit');

    final done = (await backlog.updateNote(
      UpdateNoteRequest(
        id: created.id,
        state: 'done',
        resolution: 'Verified with the Dart client',
      ),
    )).note;
    check(
      done.state == 'done' &&
          done.hasCompletion() &&
          done.completion.doneBy == 'owner' &&
          done.completion.resolution == 'Verified with the Dart client',
      'completion data was lost',
    );
    check(
      (await backlog.listNotes(ListNotesRequest())).notes.isEmpty,
      'done note remained in the default open list',
    );
    check(
      (await backlog.listNotes(
            ListNotesRequest(state: 'done'),
          )).notes.single.id ==
          created.id,
      'done list missed completed note',
    );

    final reopened = (await backlog.updateNote(
      UpdateNoteRequest(id: created.id, state: 'open'),
    )).note;
    check(
      reopened.state == 'open' && !reopened.hasCompletion(),
      'reopen retained completion',
    );
    await backlog.deleteNote(DeleteNoteRequest(id: created.id));
    check(
      (await backlog.listNotes(ListNotesRequest(state: 'all'))).notes.isEmpty,
      'deleted note remained visible',
    );
    io.stdout.writeln(
      'Dart backlog create, list, filter, partial edit, complete, reopen and delete passed.',
    );
  } finally {
    http.close(force: true);
  }
}
