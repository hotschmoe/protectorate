import 'dart:io' as io;

import 'package:connectrpc/connect.dart';
import 'package:connectrpc/io.dart' as rpcio;
import 'package:connectrpc/protobuf.dart';
import 'package:connectrpc/protocol/connect.dart' as protocol;
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:protectorate/gen/protectorate/v1/events.pb.dart';
import 'package:protectorate/gen/protectorate/v1/node.pb.dart';
import 'package:protectorate/hubs/connection.dart';
import 'package:protectorate/hubs/event_feed.dart';
import 'package:protectorate/hubs/hub_client.dart';
import 'package:protectorate/hubs/hub_profile.dart';
import 'package:protectorate/screens/node_detail_screen.dart';
import 'package:protectorate/screens/nodes_tab.dart';
import 'package:protectorate/theme/tokens.dart';

void main() {
  const baseUrl = String.fromEnvironment('PROTECTORATE_E2E_HUB_URL');
  const token = String.fromEnvironment('PROTECTORATE_E2E_TOKEN');
  final http = io.HttpClient()..connectionTimeout = const Duration(seconds: 8);

  testWidgets('Nodes tab renders Rust inventory and opens node detail', (
    tester,
  ) async {
    expect(baseUrl, isNotEmpty);
    expect(token, isNotEmpty);
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
    final feed = EventFeed((_) => const Stream<Event>.empty());
    addTearDown(() {
      feed.dispose();
      http.close(force: true);
    });
    await tester.runAsync(() async {
      await client.node.addNode(
        AddNodeRequest(
          node: NodeInfo(
            name: 'ui-offline',
            address: '127.0.0.1:1',
            tier: 'care',
          ),
        ),
      );
    });

    final profile = HubProfile(
      id: 'rust-widget-e2e',
      name: 'Rust fixture',
      baseUrl: baseUrl,
    );
    await tester.pumpWidget(
      MaterialApp(
        theme: appTheme(Brightness.dark),
        home: Scaffold(
          body: NodesTab(hub: profile, client: client, feed: feed),
        ),
      ),
    );

    final card = find.byKey(const ValueKey('node-ui-offline'));
    for (var attempt = 0; attempt < 50 && card.evaluate().isEmpty; attempt++) {
      await tester.runAsync(
        () => Future<void>.delayed(const Duration(milliseconds: 100)),
      );
      await tester.pump(const Duration(milliseconds: 100));
    }
    expect(card, findsOneWidget);
    expect(find.text('ui-offline'), findsOneWidget);
    expect(find.byKey(const ValueKey('tier-care')), findsOneWidget);
    expect(find.text('UNREACHABLE'), findsOneWidget);

    await tester.tap(card);
    await tester.pump(const Duration(milliseconds: 300));
    for (
      var attempt = 0;
      attempt < 20 && find.byType(NodeDetailScreen).evaluate().isEmpty;
      attempt++
    ) {
      await tester.pump(const Duration(milliseconds: 100));
    }
    expect(find.byType(NodeDetailScreen), findsOneWidget);
    for (
      var attempt = 0;
      attempt < 50 && find.byType(NodeDetail).evaluate().isEmpty;
      attempt++
    ) {
      await tester.runAsync(
        () => Future<void>.delayed(const Duration(milliseconds: 100)),
      );
      await tester.pump(const Duration(milliseconds: 100));
    }
    expect(find.byType(NodeDetail), findsOneWidget);
    expect(find.text('ui-offline'), findsWidgets);
    await tester.pumpWidget(const SizedBox.shrink());
    await tester.pump(const Duration(seconds: 16));
  });
}
