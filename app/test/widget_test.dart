import 'dart:async';

import 'package:connectrpc/test.dart';
import 'package:flutter/material.dart';
import 'package:protobuf/well_known_types/google/protobuf/struct.pb.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:protectorate/gen/protectorate/v1/backlog.connect.spec.dart';
import 'package:protectorate/gen/protectorate/v1/backlog.pb.dart';
import 'package:protectorate/gen/protectorate/v1/events.connect.spec.dart';
import 'package:protectorate/gen/protectorate/v1/events.pb.dart';
import 'package:protectorate/gen/protectorate/v1/hub.connect.spec.dart';
import 'package:protectorate/gen/protectorate/v1/hub.pb.dart';
import 'package:protectorate/gen/protectorate/v1/quota.connect.spec.dart';
import 'package:protectorate/gen/protectorate/v1/quota.pb.dart';
import 'package:protectorate/gen/protectorate/v1/session.connect.spec.dart';
import 'package:protectorate/gen/protectorate/v1/session.pb.dart';
import 'package:protectorate/screens/quota_tab.dart';
import 'package:protectorate/hubs/event_feed.dart';
import 'package:protectorate/hubs/hub_client.dart';
import 'package:protectorate/hubs/hub_profile.dart';
import 'package:protectorate/main.dart';
import 'package:protectorate/terminal/extra_keys.dart';
import 'package:protectorate/terminal/terminal_protocol.dart';
import 'package:shared_preferences/shared_preferences.dart';

const _hub = HubProfile(
  id: '1',
  name: 'home',
  baseUrl: 'https://hub.example:7470',
  token: 'secret',
);

HubClient _fakeClient(HubProfile p, {void Function(String?)? onAuth}) {
  final transport = FakeTransportBuilder()
      .unary(
        BacklogService.listItems,
        (ListItemsRequest r, ctx) => ListItemsResponse(
          items: [
            Item(
              id: 'a1',
              project: 'acme/app',
              title: 'fix login',
              priority: 2,
              state: ItemState.ITEM_STATE_TODO,
            ),
          ],
        ),
      )
      .unary(HubService.getStatus, (GetStatusRequest r, ctx) {
        onAuth?.call(ctx.requestHeaders.get('authorization')?.first);
        return GetStatusResponse(
          hostname: 'box',
          os: 'linux',
          arch: 'arm64',
          version: 'v1',
        );
      })
      .unary(
        QuotaService.getQuota,
        (GetQuotaRequest r, ctx) => GetQuotaResponse(
          quotas: [
            ProviderQuota(
              provider: 'claude',
              freshness: Freshness.FRESHNESS_FRESH,
              windows: [QuotaWindow(name: '5h', usedPercent: 40)],
            ),
            ProviderQuota(
              provider: 'gemini',
              freshness: Freshness.FRESHNESS_UNKNOWN,
            ),
          ],
        ),
      )
      .unary(
        SessionService.listSessions,
        (ListSessionsRequest r, ctx) => ListSessionsResponse(
          sessions: [Session(name: 'work', cwd: '/src')],
        ),
      )
      .server(EventService.subscribe, (SubscribeRequest r, ctx) async* {
        yield Event(kind: 'session.started', subject: 'alice');
        await Completer<void>().future;
      })
      .build(interceptors: [bearerInterceptor(p.token)]);
  return HubClient.withTransport(transport);
}

void main() {
  group('hub profile persistence', () {
    test('round trips multiple hubs through shared_preferences', () async {
      SharedPreferences.setMockInitialValues({});
      final prefs = await SharedPreferences.getInstance();
      final store = HubStore(prefs);
      await store.upsert(_hub);
      await store.upsert(
        const HubProfile(
          id: '2',
          name: 'office',
          baseUrl: 'http://10.0.0.2:7470',
        ),
      );
      await store.upsert(
        const HubProfile(id: '1', name: 'home2', baseUrl: 'http://h'),
      );

      final reloaded = HubStore(prefs);
      expect(reloaded.hubs.map((h) => h.name), ['home2', 'office']);

      await reloaded.remove('2');
      expect(HubStore(prefs).hubs.single.id, '1');
    });

    test('terminal uri maps scheme and path', () {
      expect(
        _hub.terminalUri('alice').toString(),
        'wss://hub.example:7470/ws/terminal/alice',
      );
      expect(
        _hub.terminalUri('alice', readOnly: true).toString(),
        'wss://hub.example:7470/ws/terminal/alice?readonly=1',
      );
    });

    test('url validation', () {
      expect(validateBaseUrl('http://x:1'), isNull);
      expect(validateBaseUrl('x:1'), isNotNull);
    });
  });

  group('shell', () {
    Future<void> openHub(WidgetTester tester) async {
      SharedPreferences.setMockInitialValues({});
      final store = HubStore(await SharedPreferences.getInstance());
      await tester.pumpWidget(
        ProtectorateApp(store: store, clientFactory: _fakeClient),
      );
      await tester.tap(find.byKey(const ValueKey('add-hub')));
      await tester.pumpAndSettle();
      await tester.enterText(find.byKey(const ValueKey('hub-name')), 'home');
      await tester.enterText(
        find.byKey(const ValueKey('hub-url')),
        'http://hub:7470',
      );
      await tester.tap(find.byKey(const ValueKey('hub-save')));
      await tester.pumpAndSettle();
      expect(store.hubs.single.name, 'home');
      await tester.tap(find.text('home'));
      await tester.pumpAndSettle();
    }

    testWidgets('phone width uses bottom navigation', (tester) async {
      tester.view.physicalSize = const Size(400, 800);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);
      await openHub(tester);
      expect(find.byType(NavigationBar), findsOneWidget);
      expect(find.byType(NavigationRail), findsNothing);
      expect(find.text('Events: live'), findsOneWidget);
      expect(find.text('box (linux/arm64)'), findsOneWidget);
      await tester.tap(find.text('Sessions'));
      await tester.pumpAndSettle();
      expect(find.byKey(const ValueKey('session-work')), findsOneWidget);
      await tester.tap(find.text('Quota'));
      await tester.pumpAndSettle();
      expect(find.text('5h: 40% used'), findsOneWidget);
      expect(find.text('No data. This is not spare capacity.'), findsOneWidget);
      expect(find.byKey(const ValueKey('bar-5h')), findsOneWidget);
    });

    testWidgets('desktop width uses navigation rail', (tester) async {
      tester.view.physicalSize = const Size(1200, 800);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);
      await openHub(tester);
      expect(find.byType(NavigationRail), findsOneWidget);
      expect(find.byType(NavigationBar), findsNothing);
      await tester.tap(find.text('Backlog'));
      await tester.pumpAndSettle();
      expect(find.text('acme/app'), findsOneWidget);
      expect(find.text('fix login'), findsOneWidget);
    });
  });

  test('bearer interceptor sends the token', () async {
    String? auth;
    final client = _fakeClient(_hub, onAuth: (a) => auth = a);
    await client.hub.getStatus(GetStatusRequest());
    expect(auth, 'Bearer secret');
  });

  group('event feed', () {
    test('reconnects with backoff after the stream ends', () async {
      var opens = 0;
      final feed = EventFeed(() {
        opens++;
        return Stream<Event>.fromIterable([Event(kind: 'k$opens')]);
      }, initialDelay: const Duration(milliseconds: 10))..start();
      await Future<void>.delayed(const Duration(milliseconds: 200));
      feed.dispose();
      expect(opens, greaterThan(1));
      expect(feed.events.first.kind, startsWith('k'));
      expect(feed.backoff(0), const Duration(milliseconds: 10));
      expect(feed.backoff(3), const Duration(milliseconds: 80));
      expect(feed.backoff(30), const Duration(seconds: 30));
    });
  });

  test('quota event payload decodes', () {
    final q = ProviderQuota(
      provider: 'claude',
      plan: 'max',
      freshness: Freshness.FRESHNESS_STALE,
    );
    final e = Event(kind: 'quota.updated', subject: 'claude');
    e.payload = Struct()..mergeFromProto3Json(q.toProto3Json());
    final back = quotaFromEvent(e)!;
    expect(back.plan, 'max');
    expect(back.freshness, Freshness.FRESHNESS_STALE);
    expect(quotaFromEvent(Event(kind: 'other')), isNull);
  });

  group('extra keys', () {
    test('sticky ctrl + c is 0x03 and is consumed', () {
      final m = ModifierState();
      m.toggleCtrl();
      expect(m.applyToText('c').codeUnits, [0x03]);
      expect(m.ctrl, isFalse);
      expect(m.applyToText('c'), 'c');
    });

    test('ctrl with a bar key sends the plain sequence and clears', () {
      final m = ModifierState()..toggleCtrl();
      expect(m.applyToKey(ExtraKey.esc), '\x1b');
      expect(m.ctrl, isFalse);
      expect(ExtraKey.up.sequence, '\x1b[A');
      expect(ExtraKey.tab.sequence, '\t');
    });

    testWidgets('bar buttons emit keys; Ctrl button is sticky', (tester) async {
      final m = ModifierState();
      final sent = <String>[];
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: ExtraKeysBar(modifiers: m, onInput: sent.add),
          ),
        ),
      );
      await tester.tap(find.byKey(const ValueKey('key-Esc')));
      await tester.tap(find.byKey(const ValueKey('key-|')));
      await tester.tap(find.byKey(const ValueKey('key-Ctrl')));
      await tester.pump();
      expect(m.ctrl, isTrue);
      expect(sent, ['\x1b', '|']);
    });
  });

  test('terminal framing', () {
    expect(encodeData('ls').toList(), [0x30, 0x6c, 0x73]);
    final resize = encodeResize(80, 24);
    expect(resize[0], 0x31);
    expect(String.fromCharCodes(resize.sublist(1)), '{"cols":80,"rows":24}');
    expect(decodeData([0x30, 0x41])!.toList(), [0x41]);
    expect(decodeData([0x31, 0x41]), isNull);
    expect(terminalSubprotocols('t'), ['protectorate.v1', 'token.t']);
  });
}
