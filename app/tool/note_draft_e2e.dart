import 'dart:io' as io;

import 'package:connectrpc/connect.dart';
import 'package:connectrpc/io.dart' as rpcio;
import 'package:connectrpc/protobuf.dart';
import 'package:connectrpc/protocol/connect.dart' as protocol;
// Flutter's test runner already supplies test_api for this test-only script.
// ignore: depend_on_referenced_packages
import 'package:test_api/scaffolding.dart' show test;

import 'package:protectorate/gen/protectorate/v1/backlog.pb.dart';
import 'package:protectorate/hubs/connection.dart';
import 'package:protectorate/hubs/hub_client.dart';
import 'package:protectorate/notes/note_draft.dart';

void main() {
  test('NoteDraft autosaves to the Rust hub', () async {
    const baseUrl = String.fromEnvironment('PROTECTORATE_E2E_HUB_URL');
    const token = String.fromEnvironment('PROTECTORATE_E2E_TOKEN');
    check(baseUrl.isNotEmpty && token.isNotEmpty, 'missing Rust hub settings');
    await runScenario(baseUrl, token);
  });
}

Future<void> runScenario(String baseUrl, String token) async {
  final http = io.HttpClient()..connectionTimeout = const Duration(seconds: 8);
  final transport = protocol.Transport(
    baseUrl: baseUrl,
    codec: const JsonCodec(),
    httpClient: hubHttpClient(baseUrl, client: rpcio.createHttpClient(http)),
    interceptors: [
      <I extends Object, O extends Object>(AnyFn<I, O> next) =>
          (Request<I, O> request) {
            request.headers.set('authorization', ['Bearer $token']);
            return next(request);
          },
    ],
  );
  final client = HubClient.withTransport(transport);
  final draft = NoteDraft(
    client,
    debounce: const Duration(milliseconds: 50),
    retry: const Duration(milliseconds: 100),
  );
  try {
    draft.edit(
      title: 'Autosaved note',
      body: 'First body',
      project: 'draft/e2e',
    );
    await waitFor(
      () => draft.id != null && draft.state == SaveState.saved,
      'create',
    );
    final id = draft.id!;
    var saved = (await client.backlog.getNote(GetNoteRequest(id: id))).note;
    check(
      saved.title == 'Autosaved note' && saved.body == 'First body',
      'debounced autosave did not create the note',
    );
    check(saved.project == 'draft/e2e', 'autosave lost project');

    draft.edit(body: 'Second body');
    draft.edit(body: 'Last body', project: '', pinned: true);
    await waitFor(
      () => draft.state == SaveState.saved && !draft.hasUnsaved,
      'edit',
    );
    saved = (await client.backlog.getNote(GetNoteRequest(id: id))).note;
    check(
      saved.title == 'Autosaved note' && saved.body == 'Last body',
      'autosave lost a field or kept an intermediate edit',
    );
    check(
      saved.project.isEmpty && saved.pinned,
      'autosave did not clear project or pin the note',
    );
    check(
      (await client.backlog.listNotes(
            ListNotesRequest(state: 'all'),
          )).notes.length ==
          1,
      'autosave created a duplicate note',
    );

    draft.edit(body: 'Editor closed before debounce');
    await draft.flush();
    await waitFor(
      () => draft.state == SaveState.saved && !draft.hasUnsaved,
      'flush',
    );
    saved = (await client.backlog.getNote(GetNoteRequest(id: id))).note;
    check(
      saved.body == 'Editor closed before debounce',
      'explicit close flush did not save the latest edit',
    );
    io.stdout.writeln(
      'NoteDraft debounced create, edits and close flush passed.',
    );
  } finally {
    draft.dispose();
    http.close(force: true);
  }
}

void check(bool condition, String message) {
  if (!condition) throw StateError(message);
}

Future<void> waitFor(bool Function() condition, String step) async {
  final deadline = DateTime.now().add(const Duration(seconds: 20));
  while (!condition()) {
    if (DateTime.now().isAfter(deadline)) {
      throw StateError('timed out waiting for NoteDraft $step');
    }
    await Future<void>.delayed(const Duration(milliseconds: 20));
  }
}
