import 'dart:async';

import 'package:connectrpc/test.dart';
import 'package:fixnum/fixnum.dart';
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
import 'package:protectorate/chat/badges.dart';
import 'package:protobuf/well_known_types/google/protobuf/timestamp.pb.dart';
import 'package:protectorate/screens/messages_tab.dart';
import 'package:protectorate/screens/orchestrator_tab.dart';
import 'package:protectorate/sleeves/node_fence.dart';
import 'package:protectorate/sleeves/sleeve_card.dart';

Sleeve _sleeve(String name, SleeveMode mode) => Sleeve(
  name: name,
  harness: Harness.HARNESS_CLAUDE,
  mode: mode,
  chat: ChatStatus(model: 'm-$name'),
);

void main() {
  late List<Sleeve> sleeves;
  late List<String> watched;
  late List<SpawnEnvoyRequest> spawned;
  late List<ResleeveSleeveRequest> resleeved;
  var unread = 0;

  setUp(() {
    sleeves = [];
    watched = [];
    spawned = [];
    resleeved = [];
    unread = 0;
  });

  Future<void> pump(WidgetTester tester, {String selected = 'envoy'}) async {
    final transport = FakeTransportBuilder()
        .unary(
          SleeveService.listSleeves,
          (ListSleevesRequest r, ctx) => ListSleevesResponse(sleeves: sleeves),
        )
        .unary(SleeveService.spawnEnvoy, (SpawnEnvoyRequest r, ctx) {
          spawned.add(r);
          sleeves = [_sleeve('envoy', SleeveMode.SLEEVE_MODE_CHAT)];
          return SpawnEnvoyResponse();
        })
        .unary(SleeveService.resleeveSleeve, (ResleeveSleeveRequest r, ctx) {
          resleeved.add(r);
          return ResleeveSleeveResponse();
        })
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
    final feed = EventFeed((_) => const Stream.empty());
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
    sleeves = [_sleeve('envoy', SleeveMode.SLEEVE_MODE_CHAT)];
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

  testWidgets('starts envoy from the empty state, on auto or a harness', (
    tester,
  ) async {
    sleeves = [_sleeve('worker', SleeveMode.SLEEVE_MODE_CHAT)];
    await pump(tester);
    expect(find.byKey(const ValueKey('orchestrator-empty')), findsOneWidget);
    expect(watched, isEmpty);

    await tester.tap(find.byKey(const ValueKey('start-envoy')));
    await tester.pump();
    await tester.pump();
    await tester.pump();
    expect(spawned.single.harness, Harness.HARNESS_UNSPECIFIED);
    expect(find.byKey(const ValueKey('orchestrator-empty')), findsNothing);
    expect(watched, ['envoy']);
  });

  testWidgets('start can pick a harness', (tester) async {
    await pump(tester);
    await tester.tap(find.byKey(const ValueKey('start-harness')));
    await tester.pumpAndSettle();
    await tester.tap(find.byKey(const ValueKey('harness-option-codex')).last);
    await tester.pumpAndSettle();
    await tester.tap(find.byKey(const ValueKey('start-envoy')));
    await tester.pump();
    expect(spawned.single.harness, Harness.HARNESS_CODEX);
  });

  testWidgets('respawn asks first and warns about the cache', (tester) async {
    sleeves = [_sleeve('envoy', SleeveMode.SLEEVE_MODE_CHAT)];
    await pump(tester);
    await tester.tap(find.byKey(const ValueKey('chat-menu')));
    await tester.pumpAndSettle();
    await tester.tap(find.byKey(const ValueKey('menu-respawn')));
    await tester.pumpAndSettle();
    expect(
      find.textContaining('prompt cache has to be rebuilt'),
      findsOneWidget,
    );
    await tester.tap(find.byKey(const ValueKey('respawn-cancel')));
    await tester.pumpAndSettle();
    expect(spawned, isEmpty);

    await tester.tap(find.byKey(const ValueKey('chat-menu')));
    await tester.pumpAndSettle();
    await tester.tap(find.byKey(const ValueKey('menu-respawn')));
    await tester.pumpAndSettle();
    await tester.tap(find.byKey(const ValueKey('respawn-confirm')));
    await tester.pumpAndSettle();
    expect(spawned.single.harness, Harness.HARNESS_CLAUDE);
  });

  testWidgets('resleeve moves envoy to the chosen harness', (tester) async {
    sleeves = [_sleeve('envoy', SleeveMode.SLEEVE_MODE_CHAT)];
    await pump(tester);
    await tester.tap(find.byKey(const ValueKey('chat-menu')));
    await tester.pumpAndSettle();
    await tester.tap(find.byKey(const ValueKey('menu-resleeve')));
    await tester.pumpAndSettle();
    await tester.tap(find.byKey(const ValueKey('resleeve-harness')));
    await tester.pumpAndSettle();
    await tester.tap(find.byKey(const ValueKey('harness-option-pi')).last);
    await tester.pumpAndSettle();
    await tester.tap(find.byKey(const ValueKey('resleeve-confirm')));
    await tester.pumpAndSettle();
    expect(resleeved.single.name, 'envoy');
    expect(resleeved.single.to, Harness.HARNESS_PI);
  });

  testWidgets('other sleeves get no envoy actions', (tester) async {
    sleeves = [_sleeve('zed', SleeveMode.SLEEVE_MODE_CHAT)];
    await pump(tester, selected: 'zed');
    await tester.tap(find.byKey(const ValueKey('chat-menu')));
    await tester.pumpAndSettle();
    expect(find.byKey(const ValueKey('menu-compact')), findsOneWidget);
    expect(find.byKey(const ValueKey('menu-respawn')), findsNothing);
  });

  group('cache lamp', () {
    final now = DateTime.utc(2026, 10, 20, 12);
    Future<void> lamp(WidgetTester tester, DateTime? expires) =>
        tester.pumpWidget(
          MaterialApp(
            home: Scaffold(
              body: CacheLamp(expires: expires, clock: () => now),
            ),
          ),
        );

    for (final c in [
      (
        'warm with minutes left',
        Duration(minutes: 3, seconds: 30),
        'CACHE WARM 3m',
      ),
      ('about to run out', Duration(seconds: 20), 'CACHE WARM <1m'),
      ('run out', Duration(seconds: -5), 'CACHE COLD'),
    ]) {
      testWidgets(c.$1, (tester) async {
        await lamp(tester, now.add(c.$2));
        expect(find.text(c.$3), findsOneWidget);
      });
    }

    testWidgets('unknown expiry is cold', (tester) async {
      await lamp(tester, null);
      expect(find.text('CACHE COLD'), findsOneWidget);
    });

    testWidgets('shows next to the context meter for a claude sleeve', (
      tester,
    ) async {
      final soon = DateTime.now().toUtc().add(
        const Duration(minutes: 4, seconds: 30),
      );
      sleeves = [
        Sleeve(
          name: 'envoy',
          harness: Harness.HARNESS_CLAUDE,
          mode: SleeveMode.SLEEVE_MODE_CHAT,
          chat: ChatStatus(
            contextTokens: Int64(1000),
            contextWindow: Int64(10000),
            cacheExpiresAt: Timestamp.fromDateTime(soon),
          ),
        ),
      ];
      await pump(tester);
      expect(find.text('CACHE WARM 4m'), findsOneWidget);
    });
  });

  testWidgets('talks to envoy and lists only chat sleeves, envoy first', (
    tester,
  ) async {
    sleeves = [
      _sleeve('zed', SleeveMode.SLEEVE_MODE_CHAT),
      _sleeve('shell1', SleeveMode.SLEEVE_MODE_TERMINAL),
      _sleeve('envoy', SleeveMode.SLEEVE_MODE_CHAT),
    ];
    await pump(tester);
    expect(watched, ['envoy']);
    expect(find.byKey(const ValueKey('orchestrator-empty')), findsNothing);

    await tester.tap(find.byKey(const ValueKey('sleeve-selector')));
    await tester.pumpAndSettle();
    expect(find.byKey(const ValueKey('sleeve-option-shell1')), findsNothing);
    final envoy = tester.getTopLeft(
      find.byKey(const ValueKey('sleeve-option-envoy')).last,
    );
    final zed = tester.getTopLeft(
      find.byKey(const ValueKey('sleeve-option-zed')).last,
    );
    expect(envoy.dy, lessThan(zed.dy));

    await tester.tap(find.byKey(const ValueKey('sleeve-option-zed')).last);
    await tester.pumpAndSettle();
    expect(watched, ['envoy', 'zed']);
  });

  test('envoy is pinned first in the hub fence', () {
    final g = groupSleeves(const [], [
      Sleeve(name: 'alice'),
      Sleeve(name: 'zed'),
      Sleeve(name: 'envoy'),
    ], 'home');
    expect(g.single.sleeves.map((x) => x.name), ['envoy', 'alice', 'zed']);
  });

  testWidgets('the envoy card carries an ENVOY badge', (tester) async {
    for (final name in ['envoy', 'alice']) {
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: SleeveCard(
              sleeve: _sleeve(name, SleeveMode.SLEEVE_MODE_CHAT),
              now: DateTime.utc(2026),
              onOpen: () {},
              onKill: () {},
            ),
          ),
        ),
      );
      expect(
        find.byKey(const ValueKey('envoy-badge')),
        name == 'envoy' ? findsOneWidget : findsNothing,
      );
    }
  });
}
