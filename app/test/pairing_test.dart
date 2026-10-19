import 'package:connectrpc/test.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:protectorate/hubs/hub_client.dart';
import 'package:protectorate/hubs/hub_profile.dart';
import 'package:protectorate/hubs/pairing.dart';
import 'package:protectorate/main.dart';
import 'package:protectorate/screens/hub_shell.dart';
import 'package:shared_preferences/shared_preferences.dart';

void main() {
  group('parsePairing', () {
    test('web fragment link', () {
      final r = parsePairing(
        Uri.parse(
          'https://hub.example:7470/#/pair?name=my%20hub%26&token=t0k&tab=quota',
        ),
      )!;
      expect(r.baseUrl, 'https://hub.example:7470');
      expect(r.name, 'my hub&');
      expect(r.token, 't0k');
      expect(r.tab, 'quota');
      expect(r.autoConfirm, isFalse);
    });

    test('custom scheme link', () {
      final r = parsePairing(
        Uri.parse(
          'protectorate://pair?url=http%3A%2F%2F10.0.0.2%3A7470%2F&name=lab&token=abc&confirm=auto',
        ),
      )!;
      expect(r.baseUrl, 'http://10.0.0.2:7470');
      expect(r.name, 'lab');
      expect(r.token, 'abc');
      expect(r.autoConfirm, isTrue);
    });

    test('name defaults to host', () {
      final r = parsePairing(Uri.parse('http://box:7470/#/pair?token=x'))!;
      expect(r.name, 'box');
    });

    test('rejects other links', () {
      for (final u in [
        'http://box:7470/',
        'http://box:7470/#/other?token=x',
        'protectorate://other?url=http://x',
        'protectorate://pair',
        'protectorate://pair?url=ftp%3A%2F%2Fx',
        'mailto:a@b.c',
      ]) {
        expect(parsePairing(Uri.parse(u)), isNull, reason: u);
      }
    });
  });

  group('pairing flow', () {
    final link = Uri.parse(
      'protectorate://pair?url=http%3A%2F%2Fhub%3A7470&name=lab&token=abc',
    );

    Future<HubStore> start(WidgetTester tester) async {
      SharedPreferences.setMockInitialValues({});
      final store = HubStore(await SharedPreferences.getInstance());
      await tester.pumpWidget(
        ProtectorateApp(
          store: store,
          links: DeepLinks(initial: link),
          clientFactory: (p) =>
              HubClient.withTransport(FakeTransportBuilder().build()),
        ),
      );
      await tester.pumpAndSettle();
      return store;
    }

    testWidgets('confirm saves the hub and opens it', (tester) async {
      final store = await start(tester);
      expect(find.text('Add hub?'), findsOneWidget);
      expect(find.text('http://hub:7470'), findsOneWidget);
      expect(store.hubs, isEmpty);
      await tester.tap(find.byKey(const ValueKey('pair-confirm')));
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 500));
      expect(store.hubs.single.token, 'abc');
      expect(store.hubs.single.name, 'lab');
      expect(find.byType(HubShell), findsOneWidget);
      await tester.pumpWidget(const SizedBox());
    });

    testWidgets('cancel saves nothing', (tester) async {
      final store = await start(tester);
      await tester.tap(find.byKey(const ValueKey('pair-cancel')));
      await tester.pumpAndSettle();
      expect(store.hubs, isEmpty);
      expect(find.byType(HubShell), findsNothing);
    });
  });
}
