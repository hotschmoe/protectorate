import 'dart:async';

import 'package:connectrpc/test.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:protectorate/gen/protectorate/v1/chat.connect.spec.dart';
import 'package:protectorate/gen/protectorate/v1/chat.pb.dart';
import 'package:protectorate/gen/protectorate/v1/message.connect.spec.dart';
import 'package:protectorate/gen/protectorate/v1/message.pb.dart';
import 'package:protectorate/gen/protectorate/v1/sleeve.connect.spec.dart';
import 'package:protectorate/gen/protectorate/v1/sleeve.pb.dart';
import 'package:protectorate/hubs/event_feed.dart';
import 'package:protectorate/hubs/hub_client.dart';
import 'package:protectorate/screens/messages_tab.dart';
import 'package:protectorate/screens/orchestrator_tab.dart';

Sleeve _sleeve(String name, SleeveMode mode) => Sleeve(
  name: name,
  harness: Harness.HARNESS_CLAUDE,
  mode: mode,
  chat: ChatStatus(model: 'm-$name'),
);

void main() {
  late List<Sleeve> sleeves;
  late List<String> watched;
  var unread = 0;

  setUp(() {
    sleeves = [];
    watched = [];
    unread = 0;
  });

  Future<void> pump(WidgetTester tester, {String selected = 'poe'}) async {
    final transport = FakeTransportBuilder()
        .unary(
          SleeveService.listSleeves,
          (ListSleevesRequest r, ctx) => ListSleevesResponse(sleeves: sleeves),
        )
        .unary(
          MessageService.listThreads,
          (ListThreadsRequest r, ctx) => ListThreadsResponse(
            threads: [ThreadSummary(threadId: 't1', unread: unread)],
          ),
        )
        .server(ChatService.watch, (WatchRequest r, ctx) async* {
          yield ChatEvent();
          watched.add(r.sleeve);
          await Completer<void>().future;
        })
        .build();
    final feed = EventFeed(() => const Stream.empty());
    var current = selected;
    var mail = false;
    await tester.pumpWidget(
      MaterialApp(
        home: StatefulBuilder(
          builder: (context, setState) => Scaffold(
            body: OrchestratorTab(
              client: HubClient.withTransport(transport),
              feed: feed,
              selected: current,
              onSelect: (n) => setState(() => current = n),
              mail: mail,
              onMail: (m) => setState(() => mail = m),
            ),
          ),
        ),
      ),
    );
    await tester.pump();
    await tester.pump();
  }

  testWidgets('switches between chat and mail and badges unread mail', (
    tester,
  ) async {
    sleeves = [_sleeve('poe', SleeveMode.SLEEVE_MODE_CHAT)];
    unread = 3;
    await pump(tester);
    expect(find.byKey(const ValueKey('sleeve-selector')), findsOneWidget);
    expect(find.byKey(const ValueKey('mail-unread')), findsOneWidget);
    expect(find.text('3'), findsOneWidget);

    await tester.tap(find.byKey(const ValueKey('mode-mail')));
    await tester.pump();
    await tester.pump();
    expect(find.byType(MessagesTab), findsOneWidget);
    expect(find.byKey(const ValueKey('sleeve-selector')), findsNothing);

    await tester.tap(find.byKey(const ValueKey('mode-chat')));
    await tester.pump();
    expect(find.byType(MessagesTab), findsNothing);
    expect(find.byKey(const ValueKey('sleeve-selector')), findsOneWidget);
  });

  testWidgets('offers the spawn command when poe is not running', (
    tester,
  ) async {
    sleeves = [_sleeve('worker', SleeveMode.SLEEVE_MODE_CHAT)];
    await pump(tester);
    expect(find.byKey(const ValueKey('orchestrator-empty')), findsOneWidget);
    expect(find.text('protectorate orchestrator spawn'), findsOneWidget);
    expect(watched, isEmpty);
  });

  testWidgets('talks to poe and lists only chat sleeves, poe first', (
    tester,
  ) async {
    sleeves = [
      _sleeve('zed', SleeveMode.SLEEVE_MODE_CHAT),
      _sleeve('shell1', SleeveMode.SLEEVE_MODE_TERMINAL),
      _sleeve('poe', SleeveMode.SLEEVE_MODE_CHAT),
    ];
    await pump(tester);
    expect(watched, ['poe']);
    expect(find.byKey(const ValueKey('orchestrator-empty')), findsNothing);

    await tester.tap(find.byKey(const ValueKey('sleeve-selector')));
    await tester.pumpAndSettle();
    expect(find.byKey(const ValueKey('sleeve-option-shell1')), findsNothing);
    final poe = tester.getTopLeft(
      find.byKey(const ValueKey('sleeve-option-poe')).last,
    );
    final zed = tester.getTopLeft(
      find.byKey(const ValueKey('sleeve-option-zed')).last,
    );
    expect(poe.dy, lessThan(zed.dy));

    await tester.tap(find.byKey(const ValueKey('sleeve-option-zed')).last);
    await tester.pumpAndSettle();
    expect(watched, ['poe', 'zed']);
  });
}
