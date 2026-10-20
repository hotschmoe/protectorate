import 'dart:async';

import 'package:connectrpc/connect.dart';
import 'package:connectrpc/test.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:protobuf/well_known_types/google/protobuf/struct.pb.dart';
import 'package:protobuf/well_known_types/google/protobuf/timestamp.pb.dart';
import 'package:protectorate/gen/protectorate/v1/directory.connect.spec.dart';
import 'package:protectorate/gen/protectorate/v1/directory.pb.dart';
import 'package:protectorate/gen/protectorate/v1/events.pb.dart';
import 'package:protectorate/gen/protectorate/v1/message.connect.spec.dart';
import 'package:protectorate/gen/protectorate/v1/message.pb.dart';
import 'package:protectorate/hubs/event_feed.dart';
import 'package:protectorate/hubs/hub_client.dart';
import 'package:protectorate/mail/thread_view.dart';
import 'package:protectorate/screens/messages_tab.dart';
import 'package:protectorate/theme/tokens.dart';

final _t0 = DateTime.utc(2026, 10, 20, 12);
Timestamp _at(int minutesAgo) =>
    Timestamp.fromDateTime(_t0.subtract(Duration(minutes: minutesAgo)));

MailMessage _ask() => MailMessage(
  id: 'm1',
  threadId: 't1',
  from: 'api',
  to: 'owner',
  kind: MessageKind.MESSAGE_KIND_REQUEST,
  ackRequired: true,
  body: 'May I bump the schema?\nIt breaks web.',
  createdAt: _at(10),
  deliveries: [MailDelivery(recipient: 'owner')],
);

MailMessage _fyi() => MailMessage(
  id: 'm2',
  threadId: 't1',
  from: 'api',
  to: 'web',
  body: 'heads up',
  createdAt: _at(8),
  deliveries: [
    MailDelivery(recipient: 'web', deliveredAt: _at(7), ackedAt: _at(6)),
  ],
);

ThreadSummary _summary({int unread = 1}) => ThreadSummary(
  threadId: 't1',
  participants: ['api', 'owner', 'web'],
  messageCount: 2,
  last: _fyi(),
  unread: unread,
  openAcks: 1,
  projects: ['github.com/acme/api'],
);

ThreadSummary _escalated() => ThreadSummary(
  threadId: 't2',
  participants: ['web', 'project:github.com/acme/lib'],
  messageCount: 1,
  last: MailMessage(
    id: 'm3',
    threadId: 't2',
    from: 'web',
    to: 'project:acme/lib',
    body: 'export Foo',
    createdAt: _at(40),
    escalatedAt: _at(5),
  ),
  openAcks: 1,
  escalated: 1,
);

class _Hub {
  final calls = <String>[];
  final listed = <ListThreadsRequest>[];
  final sent = <MessageServiceSendRequest>[];
  final replies = <ReplyRequest>[];
  final inbox = <InboxRequest>[];
  bool owner = true;
  var unread = 1;

  HubClient client() => HubClient.withTransport(
    FakeTransportBuilder()
        .unary(MessageService.listThreads, (ListThreadsRequest r, _) {
          listed.add(r);
          return ListThreadsResponse(
            threads: [
              if (r.project.isEmpty) _summary(unread: unread),
              if (r.sleeve.isEmpty) _escalated(),
            ],
          );
        })
        .unary(MessageService.thread, (ThreadRequest r, _) {
          calls.add('thread ${r.threadId}');
          return ThreadResponse(messages: [_ask(), _fyi()]);
        })
        .unary(MessageService.inbox, (InboxRequest r, _) {
          inbox.add(r);
          unread = 0;
          return InboxResponse();
        })
        .unary(MessageService.ack, (AckRequest r, _) {
          calls.add('ack ${r.id}');
          return AckResponse();
        })
        .unary(MessageService.reply, (ReplyRequest r, _) {
          if (!owner) {
            throw ConnectException(Code.permissionDenied, 'no message_*');
          }
          replies.add(r);
          return ReplyResponse();
        })
        .unary(MessageService.send, (MessageServiceSendRequest r, _) {
          sent.add(r);
          return MessageServiceSendResponse(
            message: MailMessage(id: 'm9', threadId: 't9'),
          );
        })
        .unary(
          DirectoryService.listProjects,
          (ListProjectsRequest r, _) => ListProjectsResponse(
            projects: [
              DirectoryProject(
                id: 'github.com/acme/api',
                display: 'acme/api',
                owners: [ProjectOwner(sleeve: 'api')],
              ),
            ],
          ),
        )
        .build(),
  );
}

Future<EventFeed> _pump(
  WidgetTester t,
  _Hub hub, {
  StreamController<Event>? events,
  double width = 412,
}) async {
  t.view.physicalSize = Size(width, 1200);
  t.view.devicePixelRatio = 1;
  addTearDown(t.view.reset);
  final stream = events ?? StreamController<Event>.broadcast();
  if (events == null) addTearDown(stream.close);
  final feed = EventFeed((_) => stream.stream)..start();
  addTearDown(feed.dispose);
  await t.pumpWidget(
    MaterialApp(
      theme: appTheme(Brightness.dark),
      home: Scaffold(
        body: MessagesTab(client: hub.client(), feed: feed, now: () => _t0),
      ),
    ),
  );
  await t.pump();
  await t.pump();
  return feed;
}

void main() {
  group('helpers', () {
    test('delivery states', () {
      final cases = [
        (
          _ask(),
          MailDelivery(recipient: 'owner'),
          ('QUEUED, ACK DUE', Tone.warn),
        ),
        (
          _ask(),
          MailDelivery(recipient: 'owner', readAt: _at(1)),
          ('READ, ACK DUE', Tone.warn),
        ),
        (_fyi(), _fyi().deliveries.first, ('ACKED', Tone.ok)),
        (
          _fyi(),
          MailDelivery(recipient: 'web', deliveredAt: _at(1)),
          ('DELIVERED', Tone.lit),
        ),
        (_fyi(), MailDelivery(recipient: 'web'), ('QUEUED', Tone.dim)),
      ];
      for (final (m, d, want) in cases) {
        expect(deliveryState(m, d), want, reason: '${m.id} ${d.recipient}');
      }
    });

    test('targets, labels and permissions', () {
      final orch = MailMessage(
        from: 'hub',
        to: 'envoy',
        deliveries: [MailDelivery(recipient: 'envoy')],
      );
      final waiting = MailMessage(from: 'web', to: 'project:acme/lib');
      expect(threadTargets([_ask(), _fyi(), orch, waiting]), [
        'api',
        'web',
        'orchestrator',
        'project:acme/lib',
      ]);
      for (final (input, want) in [
        ('github.com/acme/lib', 'acme/lib'),
        ('path:/home/me/src/tool', 'tool'),
        ('lib', 'lib'),
      ]) {
        expect(shortProject(input), want);
      }
      expect(partyLabel('project:github.com/acme/lib'), 'project:acme/lib');
      expect(replyCandidates([_ask(), _fyi()]).map((m) => m.id), ['m1', 'm2']);
      expect(replyCandidates([_fyi()]).map((m) => m.id), ['m2']);
      expect(canReply(_ask()), isTrue);
      expect(canReply(orch), isFalse);
      expect(ownerCanAck(_ask()), isTrue);
      expect(ownerCanAck(_fyi()), isFalse);
      expect(unreadByOwner(_ask()), isTrue);
      expect(
        mailFailure(ConnectException(Code.permissionDenied, 'x')),
        contains('owner token'),
      );
    });
  });

  testWidgets('threads show participants, last word and their lamps', (
    t,
  ) async {
    final hub = _Hub();
    await _pump(t, hub);
    expect(find.text('api, owner, web'), findsOneWidget);
    expect(find.text('web, project:acme/lib'), findsOneWidget);
    expect(find.text('1 UNREAD'), findsNWidgets(2));
    expect(find.byKey(const ValueKey('acks-t1')), findsOneWidget);
    expect(find.byKey(const ValueKey('escalated-t2')), findsOneWidget);
    expect(find.byKey(const ValueKey('escalated-t1')), findsNothing);
    expect(find.textContaining('heads up'), findsOneWidget);
    expect(find.text('8m ago'), findsOneWidget);
  });

  testWidgets('opening a thread shows deliveries and marks it read', (t) async {
    final hub = _Hub();
    await _pump(t, hub);
    await t.tap(find.byKey(const ValueKey('thread-row-t1')));
    await t.pumpAndSettle();
    expect(hub.calls, contains('thread t1'));
    expect(hub.inbox.single.threadId, 't1');
    expect(hub.inbox.single.peek, isFalse);
    expect(find.text('THREAD t1  2 MESSAGES'), findsOneWidget);
    expect(find.text('owner QUEUED, ACK DUE'), findsOneWidget);
    expect(find.text('web ACKED'), findsOneWidget);
    expect(find.text('REQUEST'), findsOneWidget);
    expect(find.byKey(const ValueKey('ack-m1')), findsOneWidget);
    expect(find.byKey(const ValueKey('ack-m2')), findsNothing);

    await t.tap(find.byKey(const ValueKey('ack-m1')));
    await t.pumpAndSettle();
    expect(hub.calls, contains('ack m1'));

    await t.enterText(find.byKey(const ValueKey('mail-compose')), 'go ahead');
    await t.tap(find.byKey(const ValueKey('mail-send')));
    await t.pumpAndSettle();
    expect(hub.replies.single.id, 'm1');
    expect(hub.replies.single.body, 'go ahead');
    expect(hub.replies.single.requestId, startsWith('app-'));

    await t.tap(find.byKey(const ValueKey('choice-TO web')));
    await t.pump();
    await t.enterText(find.byKey(const ValueKey('mail-compose')), 'thanks');
    await t.tap(find.byKey(const ValueKey('mail-send')));
    await t.pumpAndSettle();
    expect(hub.sent.single.to, 'web');
    expect(hub.sent.single.threadId, 't1');

    await t.tap(find.byKey(const ValueKey('reply-m2')));
    await t.pump();
    await t.enterText(find.byKey(const ValueKey('mail-compose')), 'later');
    await t.tap(find.byKey(const ValueKey('mail-send')));
    await t.pumpAndSettle();
    expect(hub.replies.last.id, 'm2');
  });

  testWidgets('a token without message scopes is told what it needs', (
    t,
  ) async {
    final hub = _Hub()..owner = false;
    await _pump(t, hub);
    await t.tap(find.byKey(const ValueKey('thread-row-t1')));
    await t.pumpAndSettle();
    await t.enterText(find.byKey(const ValueKey('mail-compose')), 'hello');
    await t.tap(find.byKey(const ValueKey('mail-send')));
    await t.pumpAndSettle();
    expect(find.textContaining('need the owner token'), findsOneWidget);
    expect(find.text('hello'), findsOneWidget);
  });

  testWidgets('wide layout keeps the list beside the open thread', (t) async {
    final hub = _Hub();
    await _pump(t, hub, width: 1280);
    expect(find.text('SELECT A THREAD'), findsOneWidget);
    await t.tap(find.byKey(const ValueKey('thread-row-t2')));
    await t.pumpAndSettle();
    expect(find.text('THREAD t2  2 MESSAGES'), findsOneWidget);
    expect(find.byKey(const ValueKey('thread-row-t1')), findsOneWidget);
  });

  testWidgets('filters go to the hub and mail events refresh the list', (
    t,
  ) async {
    final hub = _Hub();
    final events = StreamController<Event>.broadcast();
    addTearDown(events.close);
    await _pump(t, hub, events: events);
    await t.tap(find.byKey(const ValueKey('filter-sleeve')));
    await t.pumpAndSettle();
    await t.tap(find.byKey(const ValueKey('filter-SLEEVE-api')).last);
    await t.pumpAndSettle();
    expect(hub.listed.last.sleeve, 'api');
    expect(find.text('web, project:acme/lib'), findsNothing);

    await t.tap(find.byKey(const ValueKey('filter-project')));
    await t.pumpAndSettle();
    await t.tap(find.byKey(const ValueKey('filter-PROJECT-acme/api')).last);
    await t.pumpAndSettle();
    expect(hub.listed.last.project, 'github.com/acme/api');
    expect(find.textContaining('No threads match'), findsOneWidget);

    final before = hub.listed.length;
    events.add(
      Event(
        kind: 'msg.sent',
        subject: 'm4',
        payload: Struct()..mergeFromProto3Json({'thread': 't1'}),
      ),
    );
    await t.pump(const Duration(milliseconds: 400));
    await t.pump();
    expect(hub.listed.length, before + 1);
  });

  testWidgets('a new message goes out with its kind and opens its thread', (
    t,
  ) async {
    final hub = _Hub();
    await _pump(t, hub);
    await t.tap(find.byKey(const ValueKey('mail-new')));
    await t.pumpAndSettle();
    await t.tap(find.byKey(const ValueKey('choice-project:acme/api')));
    await t.tap(find.byKey(const ValueKey('choice-REQUEST')));
    await t.enterText(find.byKey(const ValueKey('compose-body')), 'export Foo');
    await t.tap(find.byKey(const ValueKey('compose-send')));
    await t.pumpAndSettle();
    final req = hub.sent.single;
    expect(req.to, 'project:acme/api');
    expect(req.kind, MessageKind.MESSAGE_KIND_REQUEST);
    expect(req.body, 'export Foo');
    expect(hub.calls, contains('thread t9'));
  });
}
