import 'dart:async';

import 'package:connectrpc/connect.dart';
import 'package:connectrpc/test.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:protobuf/well_known_types/google/protobuf/timestamp.pb.dart';
import 'package:protectorate/gen/protectorate/v1/backlog.connect.spec.dart';
import 'package:protectorate/gen/protectorate/v1/backlog.pb.dart';
import 'package:protectorate/gen/protectorate/v1/events.pb.dart';
import 'package:protectorate/hubs/event_feed.dart';
import 'package:protectorate/hubs/hub_client.dart';
import 'package:protectorate/notes/note_draft.dart';
import 'package:protectorate/screens/backlog_tab.dart';
import 'package:protectorate/theme/tokens.dart';

Timestamp _ago(Duration d) =>
    Timestamp.fromDateTime(DateTime.now().toUtc().subtract(d));

// A hub with a notes store in memory; `down` makes every call fail the way an
// unreachable hub does.
class _Notes {
  _Notes(this.notes);

  final List<Note> notes;
  bool down = false;
  int calls = 0;
  final updates = <UpdateNoteRequest>[];
  final creates = <CreateNoteRequest>[];
  final deletes = <String>[];
  Object? reject;

  void _gate() {
    calls++;
    if (down) throw ConnectException(Code.unavailable, 'hub unreachable');
    if (reject != null) throw reject!;
  }

  HubClient client() => HubClient.withTransport(
    FakeTransportBuilder()
        .unary(BacklogService.listNotes, (ListNotesRequest r, ctx) {
          _gate();
          return ListNotesResponse(notes: notes);
        })
        .unary(BacklogService.createNote, (CreateNoteRequest r, ctx) {
          _gate();
          creates.add(r);
          final n = Note(
            id: 'n${notes.length + 10}',
            title: r.title,
            body: r.body,
            project: r.project,
            pinned: r.pinned,
            source: 'owner',
            createdAt: _ago(Duration.zero),
            updatedAt: _ago(Duration.zero),
          );
          notes.add(n);
          return CreateNoteResponse(note: n);
        })
        .unary(BacklogService.updateNote, (UpdateNoteRequest r, ctx) {
          _gate();
          updates.add(r);
          final i = notes.indexWhere((n) => n.id == r.id);
          final n = notes[i].deepCopy();
          if (r.hasTitle()) n.title = r.title;
          if (r.hasBody()) n.body = r.body;
          if (r.hasProject()) n.project = r.project;
          if (r.hasPinned()) n.pinned = r.pinned;
          n.updatedAt = _ago(Duration.zero);
          notes[i] = n;
          return UpdateNoteResponse(note: n);
        })
        .unary(BacklogService.deleteNote, (DeleteNoteRequest r, ctx) {
          _gate();
          deletes.add(r.id);
          notes.removeWhere((n) => n.id == r.id);
          return DeleteNoteResponse();
        })
        .build(),
  );
}

List<Note> _seed() => [
  Note(
    id: 'a1',
    title: 'Older note',
    body: 'first body',
    source: 'owner',
    createdAt: _ago(const Duration(days: 3)),
    updatedAt: _ago(const Duration(days: 3)),
  ),
  Note(
    id: 'a2',
    title: 'Filed by a sleeve',
    body: 'please export Foo',
    project: 'acme/lib',
    source: 'apidev',
    createdAt: _ago(const Duration(hours: 1)),
    updatedAt: _ago(const Duration(hours: 1)),
  ),
  Note(
    id: 'a3',
    title: 'Pinned note',
    body: 'stays on top',
    pinned: true,
    source: 'owner',
    createdAt: _ago(const Duration(days: 9)),
    updatedAt: _ago(const Duration(days: 9)),
  ),
  Note(
    id: 'a4',
    title: 'From the hub',
    body: 'upstream released',
    project: 'acme/api',
    source: 'hub',
    createdAt: _ago(const Duration(minutes: 5)),
    updatedAt: _ago(const Duration(minutes: 5)),
  ),
];

Future<void> _pumpTab(
  WidgetTester tester,
  _Notes hub, {
  double width = 412,
  double height = 900,
}) async {
  tester.view.physicalSize = Size(width, height);
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.reset);
  final feed = EventFeed(() => const Stream<Event>.empty());
  addTearDown(feed.dispose);
  await tester.pumpWidget(
    MaterialApp(
      theme: appTheme(Brightness.dark),
      home: BacklogTab(client: hub.client(), feed: feed),
    ),
  );
  await tester.pump();
  await tester.pump(const Duration(milliseconds: 50));
}

Future<void> _type(WidgetTester tester, String key, String text) async {
  await tester.enterText(find.byKey(ValueKey(key)), text);
  await tester.pump();
}

double _top(WidgetTester tester, String key) =>
    tester.getTopLeft(find.byKey(ValueKey(key))).dy;

String _status(WidgetTester tester) => tester
    .widget<Text>(
      find.descendant(
        of: find.byKey(const ValueKey('note-status')),
        matching: find.byType(Text),
      ),
    )
    .data!;

void main() {
  group('NoteDraft', () {
    testWidgets('waits out the pause and sends only what changed', (
      tester,
    ) async {
      final hub = _Notes(_seed());
      final d = NoteDraft(hub.client(), note: hub.notes[0]);
      addTearDown(d.dispose);
      d.edit(body: 'first body, more');
      await tester.pump(const Duration(milliseconds: 700));
      expect(hub.updates, isEmpty);
      expect(d.state, SaveState.edited);
      d.edit(body: 'first body, more still');
      await tester.pump(const Duration(milliseconds: 700));
      expect(hub.updates, isEmpty, reason: 'typing restarts the pause');
      await tester.pump(const Duration(milliseconds: 200));
      expect(hub.updates, hasLength(1));
      final sent = hub.updates.single;
      expect(sent.id, 'a1');
      expect(sent.body, 'first body, more still');
      expect(sent.hasTitle(), isFalse);
      expect(sent.hasPinned(), isFalse);
      expect(d.state, SaveState.saved);
    });

    testWidgets('creates on the first text and updates after', (tester) async {
      final hub = _Notes([]);
      final d = NoteDraft(hub.client());
      addTearDown(d.dispose);
      d.edit(title: '   ');
      await tester.pump(const Duration(seconds: 1));
      expect(hub.calls, 0, reason: 'a blank note is not stored');
      d.edit(title: 'Groceries');
      await tester.pump(const Duration(seconds: 1));
      expect(hub.creates.single.title, 'Groceries');
      expect(d.id, isNotNull);
      d.edit(body: 'milk');
      await tester.pump(const Duration(seconds: 1));
      expect(hub.creates, hasLength(1));
      expect(hub.updates.single.id, d.id);
      expect(hub.updates.single.body, 'milk');
    });

    testWidgets('stays dirty while the hub is down and retries', (
      tester,
    ) async {
      final hub = _Notes(_seed())..down = true;
      final d = NoteDraft(hub.client(), note: hub.notes[0]);
      addTearDown(d.dispose);
      d.edit(body: 'typed offline');
      await tester.pump(const Duration(seconds: 1));
      expect(d.state, SaveState.offline);
      expect(d.hasUnsaved, isTrue);
      await tester.pump(const Duration(seconds: 2));
      expect(d.state, SaveState.offline, reason: 'still down');
      final tries = hub.calls;
      hub.down = false;
      await tester.pump(const Duration(seconds: 5));
      expect(hub.calls, greaterThan(tries));
      expect(d.state, SaveState.saved);
      expect(hub.notes[0].body, 'typed offline');
    });

    testWidgets('a rejected save is not retried', (tester) async {
      final hub = _Notes(_seed())
        ..reject = ConnectException(Code.invalidArgument, 'title too long');
      final d = NoteDraft(hub.client(), note: hub.notes[0]);
      addTearDown(d.dispose);
      d.edit(title: 'x');
      await tester.pump(const Duration(seconds: 1));
      final tries = hub.calls;
      await tester.pump(const Duration(seconds: 60));
      expect(d.state, SaveState.rejected);
      expect(d.error, 'title too long');
      expect(hub.calls, tries);
    });

    testWidgets('local edits win over the hub copy', (tester) async {
      final hub = _Notes(_seed())..down = true;
      final d = NoteDraft(hub.client(), note: hub.notes[0]);
      final remote = hub.notes[0].deepCopy()
        ..body = 'edited by the orchestrator'
        ..updatedAt = _ago(Duration.zero);
      d.adopt(remote);
      expect(d.body, 'edited by the orchestrator');
      d.edit(body: 'mine');
      d.adopt(remote.deepCopy()..updatedAt = _ago(const Duration(minutes: -1)));
      expect(d.body, 'mine');
      d.dispose();
    });
  });

  group('notes tab', () {
    testWidgets('lists pinned first, then newest, with tags and sources', (
      tester,
    ) async {
      await _pumpTab(tester, _Notes(_seed()));
      final order = ['note-a3', 'note-a4', 'note-a2', 'note-a1'];
      for (var i = 1; i < order.length; i++) {
        expect(
          _top(tester, order[i - 1]),
          lessThan(_top(tester, order[i])),
          reason: '${order[i - 1]} before ${order[i]}',
        );
      }
      expect(find.text('from sleeve apidev'), findsOneWidget);
      expect(find.text('from hub'), findsOneWidget);
      expect(find.text('from owner'), findsNothing);
      expect(find.byKey(const ValueKey('tag-acme/lib')), findsOneWidget);
      expect(find.byIcon(Icons.push_pin), findsOneWidget);
    });

    testWidgets('a tag chip filters the list', (tester) async {
      await _pumpTab(tester, _Notes(_seed()));
      await tester.tap(find.byKey(const ValueKey('tag-acme/lib')));
      await tester.pump();
      expect(find.byKey(const ValueKey('note-a2')), findsOneWidget);
      expect(find.byKey(const ValueKey('note-a1')), findsNothing);
      await tester.tap(find.byKey(const ValueKey('tag-all')));
      await tester.pump();
      expect(find.byKey(const ValueKey('note-a1')), findsOneWidget);
    });

    testWidgets('phone: the editor replaces the list and autosaves', (
      tester,
    ) async {
      final hub = _Notes(_seed());
      await _pumpTab(tester, hub);
      await tester.tap(find.byKey(const ValueKey('note-a1')));
      await tester.pump();
      expect(find.byKey(const ValueKey('note-title')), findsOneWidget);
      expect(find.byKey(const ValueKey('note-a2')), findsNothing);
      expect(_status(tester), 'SAVED');

      await _type(tester, 'note-body', 'first body\nsecond line');
      expect(_status(tester), 'EDITED');
      await tester.pump(const Duration(seconds: 1));
      await tester.pump();
      expect(_status(tester), 'SAVED');
      expect(hub.updates.single.body, 'first body\nsecond line');

      await tester.tap(find.byKey(const ValueKey('note-back')));
      await tester.pump();
      expect(find.byKey(const ValueKey('note-a2')), findsOneWidget);
      expect(find.byKey(const ValueKey('note-title')), findsNothing);
    });

    testWidgets('a tag can be picked from the existing ones', (tester) async {
      final hub = _Notes(_seed());
      await _pumpTab(tester, hub);
      await tester.tap(find.byKey(const ValueKey('note-a1')));
      await tester.pump();
      await tester.ensureVisible(
        find.byKey(const ValueKey('suggest-acme/api')),
      );
      await tester.tap(find.byKey(const ValueKey('suggest-acme/api')));
      await tester.pump(const Duration(seconds: 1));
      expect(hub.updates.single.project, 'acme/api');
      expect(hub.updates.single.hasBody(), isFalse);
    });

    testWidgets('closing the editor does not drop an unsaved edit', (
      tester,
    ) async {
      final hub = _Notes(_seed());
      await _pumpTab(tester, hub);
      await tester.tap(find.byKey(const ValueKey('note-a1')));
      await tester.pump();
      await _type(tester, 'note-title', 'Renamed quickly');
      await tester.tap(find.byKey(const ValueKey('note-back')));
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 100));
      expect(hub.updates.single.title, 'Renamed quickly');
      expect(find.text('Renamed quickly'), findsOneWidget);
    });

    testWidgets('a new note is created once it has text', (tester) async {
      final hub = _Notes(_seed());
      await _pumpTab(tester, hub);
      await tester.tap(find.byKey(const ValueKey('add-note')));
      await tester.pump();
      await tester.pump(const Duration(seconds: 1));
      expect(hub.creates, isEmpty);
      await _type(tester, 'note-title', 'Buy cables');
      await tester.pump(const Duration(seconds: 1));
      await tester.pump();
      expect(hub.creates.single.title, 'Buy cables');
      expect(_status(tester), 'SAVED');
      await tester.tap(find.byKey(const ValueKey('note-back')));
      await tester.pump();
      expect(find.text('Buy cables'), findsOneWidget);
    });

    testWidgets('pinning saves right away', (tester) async {
      final hub = _Notes(_seed());
      await _pumpTab(tester, hub);
      await tester.tap(find.byKey(const ValueKey('note-a1')));
      await tester.pump();
      await tester.tap(find.byKey(const ValueKey('note-pin')));
      await tester.pump(const Duration(milliseconds: 10));
      await tester.pump(const Duration(milliseconds: 10));
      expect(hub.updates.single.pinned, isTrue);
      await tester.tap(find.byKey(const ValueKey('note-back')));
      await tester.pump();
      expect(_top(tester, 'note-a1'), lessThan(_top(tester, 'note-a4')));
    });

    testWidgets('offline edits show and retry until they land', (tester) async {
      final hub = _Notes(_seed());
      await _pumpTab(tester, hub);
      await tester.tap(find.byKey(const ValueKey('note-a1')));
      await tester.pump();
      hub.down = true;
      await _type(tester, 'note-body', 'written on the train');
      await tester.pump(const Duration(seconds: 1));
      await tester.pump();
      expect(_status(tester), 'OFFLINE, RETRYING');
      hub.down = false;
      await tester.pump(const Duration(seconds: 3));
      await tester.pump();
      expect(_status(tester), 'SAVED');
      expect(hub.notes[0].body, 'written on the train');
    });

    testWidgets('delete asks first', (tester) async {
      final hub = _Notes(_seed());
      await _pumpTab(tester, hub);
      await tester.tap(find.byKey(const ValueKey('note-a1')));
      await tester.pump();
      await tester.tap(find.byKey(const ValueKey('note-delete')));
      await tester.pumpAndSettle();
      expect(find.text('Delete this note?'), findsOneWidget);
      await tester.tap(find.byKey(const ValueKey('note-delete-cancel')));
      await tester.pumpAndSettle();
      expect(hub.deletes, isEmpty);
      expect(find.byKey(const ValueKey('note-title')), findsOneWidget);

      await tester.tap(find.byKey(const ValueKey('note-delete')));
      await tester.pumpAndSettle();
      await tester.tap(find.byKey(const ValueKey('note-delete-confirm')));
      await tester.pumpAndSettle();
      expect(hub.deletes, ['a1']);
      expect(find.byKey(const ValueKey('note-a1')), findsNothing);
      expect(find.byKey(const ValueKey('note-a2')), findsOneWidget);
    });

    testWidgets('desktop: list and editor sit side by side', (tester) async {
      final hub = _Notes(_seed());
      await _pumpTab(tester, hub, width: 1200, height: 800);
      expect(find.text('Pick a note or start a new one.'), findsOneWidget);
      await tester.tap(find.byKey(const ValueKey('note-a2')));
      await tester.pump();
      expect(find.byKey(const ValueKey('note-a1')), findsOneWidget);
      expect(find.byKey(const ValueKey('note-title')), findsOneWidget);
      expect(find.byKey(const ValueKey('note-back')), findsNothing);
      expect(
        tester.getTopLeft(find.byKey(const ValueKey('note-title'))).dx,
        greaterThan(
          tester.getTopRight(find.byKey(const ValueKey('note-a1'))).dx - 1,
        ),
      );
      expect(find.text('please export Foo'), findsWidgets);
      expect(find.text('NOTE a2'), findsOneWidget);
    });

    testWidgets('a note the orchestrator changed shows up in an open editor', (
      tester,
    ) async {
      final hub = _Notes(_seed());
      final events = StreamController<Event>.broadcast();
      addTearDown(events.close);
      tester.view.physicalSize = const Size(1200, 800);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);
      final feed = EventFeed(() => events.stream)..start();
      addTearDown(feed.dispose);
      await tester.pumpWidget(
        MaterialApp(
          theme: appTheme(Brightness.dark),
          home: BacklogTab(client: hub.client(), feed: feed),
        ),
      );
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 50));
      await tester.tap(find.byKey(const ValueKey('note-a1')));
      await tester.pump();
      expect(find.text('first body'), findsWidgets);
      hub.notes[0] = hub.notes[0].deepCopy()
        ..body = 'rewritten by envoy'
        ..updatedAt = _ago(Duration.zero);
      events.add(Event(kind: 'note.updated', subject: 'a1'));
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 50));
      expect(find.text('rewritten by envoy'), findsWidgets);
      expect(hub.updates, isEmpty);
    });
  });
}
