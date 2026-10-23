import 'dart:io' as io;

import 'package:connectrpc/io.dart' as rpcio;
import 'package:connectrpc/protobuf.dart';
import 'package:connectrpc/protocol/connect.dart' as protocol;
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:protectorate/gen/protectorate/v1/events.pb.dart';
import 'package:protectorate/gen/protectorate/v1/node.pb.dart';
import 'package:protectorate/hubs/connection.dart';
import 'package:protectorate/hubs/event_feed.dart';
import 'package:protectorate/hubs/hub_client.dart';
import 'package:protectorate/hubs/hub_profile.dart';
import 'package:protectorate/nodes/add_node.dart';
import 'package:protectorate/screens/node_detail_screen.dart';
import 'package:protectorate/screens/nodes_tab.dart';
import 'package:protectorate/theme/tokens.dart';
import 'package:protectorate/vfd/lamp.dart';

// Node invitation against a real Rust hub with a TLS listener: InviteNode
// issues a real single-use join code, the node joins with it over TLS pinned
// to the hub CA, and WatchNodeSetup streams the real setup stages.
void main() {
  const baseUrl = String.fromEnvironment('PROTECTORATE_E2E_HUB_URL');
  const token = String.fromEnvironment('PROTECTORATE_E2E_TOKEN');
  const nodeBinary = String.fromEnvironment('PROTECTORATE_E2E_NODE_BINARY');
  const nodeHome = String.fromEnvironment('PROTECTORATE_E2E_NODE_HOME');
  const tlsUrl = String.fromEnvironment('PROTECTORATE_E2E_TLS_URL');
  const caCert = String.fromEnvironment('PROTECTORATE_E2E_CA_CERT');
  const nodeListen = String.fromEnvironment('PROTECTORATE_E2E_NODE_LISTEN');
  final http = io.HttpClient()..connectionTimeout = const Duration(seconds: 8);

  HubClient hubClient() => HubClient.withTransport(
    protocol.Transport(
      baseUrl: baseUrl,
      codec: const JsonCodec(),
      httpClient: hubHttpClient(baseUrl, client: rpcio.createHttpClient(http)),
      interceptors: [bearerInterceptor(token)],
    ),
  );

  // Real network work only progresses inside runAsync.
  Future<void> wait(
    WidgetTester tester,
    Finder until, {
    int tries = 150,
  }) async {
    for (var i = 0; i < tries && until.evaluate().isEmpty; i++) {
      await tester.runAsync(
        () => Future<void>.delayed(const Duration(milliseconds: 100)),
      );
      await tester.pump(const Duration(milliseconds: 100));
    }
    expect(until, findsWidgets);
  }

  tearDownAll(() => http.close(force: true));

  testWidgets('Nodes tab invites a node through a configured Rust hub', (
    tester,
  ) async {
    expect(baseUrl, isNotEmpty);
    expect(token, isNotEmpty);
    // A desktop view: 1200x800 logical pixels.
    tester.view.physicalSize = const Size(2400, 1600);
    tester.view.devicePixelRatio = 2;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);
    final name = 'ui-invite-${io.pid}';
    final client = hubClient();
    final feed = EventFeed((_) => const Stream<Event>.empty());
    addTearDown(feed.dispose);

    String? clipboard;
    tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
      SystemChannels.platform,
      (call) async {
        if (call.method == 'Clipboard.setData') {
          clipboard =
              (call.arguments as Map<Object?, Object?>?)?['text'] as String?;
        }
        return null;
      },
    );
    addTearDown(
      () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        SystemChannels.platform,
        null,
      ),
    );

    final profile = HubProfile(
      id: 'rust-invite-e2e',
      name: 'Rust invite fixture',
      baseUrl: baseUrl,
      token: token,
    );
    await tester.pumpWidget(
      MaterialApp(
        theme: appTheme(Brightness.dark),
        home: Scaffold(
          body: NodesTab(hub: profile, client: client, feed: feed),
        ),
      ),
    );
    await wait(tester, find.byKey(const ValueKey('add-node')));

    // A name the hub would refuse never reaches the hub: the field blocks it
    // and CREATE INVITE stays disabled.
    await tester.tap(find.byKey(const ValueKey('add-node')));
    await tester.pump(const Duration(milliseconds: 300));
    expect(find.byType(AddNodeDialog), findsOneWidget);
    await tester.enterText(find.byKey(const ValueKey('add-name')), '-bad');
    await tester.pump();
    expect(
      find.text('Lowercase letters, digits and -, not at either end.'),
      findsOneWidget,
    );
    expect(
      tester
          .widget<FilledButton>(find.byKey(const ValueKey('add-create')))
          .onPressed,
      isNull,
    );

    // A name the hub itself refuses shows the hub's reason in the dialog.
    final listed = await tester.runAsync(
      () => client.node.listNodes(ListNodesRequest()),
    );
    expect(listed, isNotNull);
    final hubName = listed!.nodes.firstWhere((n) => n.node.local).node.name;
    await tester.enterText(find.byKey(const ValueKey('add-name')), hubName);
    await tester.pump();
    await tester.tap(find.byKey(const ValueKey('add-create')));
    await wait(tester, find.textContaining("is the hub's own node"));

    await tester.enterText(find.byKey(const ValueKey('add-name')), name);
    await tester.pump();
    await tester.tap(find.byKey(const ValueKey('add-create')));
    await wait(tester, find.byKey(const ValueKey('add-command')));
    await tester.tap(find.byKey(const ValueKey('choice-LINUX')));
    await tester.pump();

    final first = tester
        .widget<SelectableText>(find.byKey(const ValueKey('add-command-text')))
        .data!;
    expect(first, contains(Uri.parse(tlsUrl).host));
    expect(first, contains('ca.pem'));
    expect(first, contains('install.sh'));
    expect(first, contains(name));
    expect(find.textContaining('CODE UNTIL'), findsOneWidget);

    // The hub's setup stream reached the dialog, not just the local guess.
    await wait(tester, find.byKey(const ValueKey('setup-message')));
    expect(
      find.text('waiting for the machine to run the installer'),
      findsOneWidget,
    );
    expect(
      tester.widget<LampLegend>(find.byKey(const ValueKey('stage-INVITED'))).on,
      isTrue,
    );
    expect(
      tester.widget<LampLegend>(find.byKey(const ValueKey('stage-JOINED'))).on,
      isFalse,
    );

    // The same invite carries the installer for every platform.
    await tester.tap(find.byKey(const ValueKey('choice-WINDOWS')));
    await tester.pump();
    final windows = tester
        .widget<SelectableText>(find.byKey(const ValueKey('add-command-text')))
        .data!;
    expect(windows, contains('install.ps1'));
    expect(windows, contains('Get-FileHash'));

    await tester.tap(find.byKey(const ValueKey('choice-TERMUX')));
    await tester.pump();
    final termux = tester
        .widget<SelectableText>(find.byKey(const ValueKey('add-command-text')))
        .data!;
    expect(termux, contains('install-termux.sh'));

    await tester.tap(find.byKey(const ValueKey('add-copy')));
    await tester.pump();
    expect(find.text('COPIED'), findsOneWidget);
    expect(clipboard, termux);

    await tester.tap(find.byKey(const ValueKey('add-close')));
    for (
      var i = 0;
      i < 10 && find.byType(AddNodeDialog).evaluate().isNotEmpty;
      i++
    ) {
      await tester.pump(const Duration(milliseconds: 200));
    }
    expect(find.byType(AddNodeDialog), findsNothing);

    // The invited node shows as pending, without an unreachable alert.
    final card = find.byKey(ValueKey('node-$name'));
    await wait(tester, card);
    expect(
      find.descendant(
        of: card,
        matching: find.byKey(const ValueKey('pending-note')),
      ),
      findsOneWidget,
    );
    expect(
      find.descendant(of: card, matching: find.text('PENDING')),
      findsOneWidget,
    );
    expect(
      find.descendant(of: card, matching: find.text('UNREACHABLE')),
      findsNothing,
    );

    final before = await tester.runAsync(
      () => client.node.getNode(GetNodeRequest(name: name)),
    );
    expect(before, isNotNull);
    expect(before!.node.invited, isTrue);

    // Reissue from the node detail replaces the code on the hub.
    await tester.ensureVisible(card);
    await tester.pump();
    await tester.tap(card);
    await tester.pump(const Duration(milliseconds: 300));
    await wait(tester, find.byType(NodeDetail));
    expect(find.text('PENDING INVITE'), findsOneWidget);
    await tester.tap(find.byKey(const ValueKey('reissue-key')));
    await tester.pump(const Duration(milliseconds: 300));
    expect(find.text('Reissue code'), findsOneWidget);
    await wait(tester, find.byKey(const ValueKey('add-command')));
    await tester.tap(find.byKey(const ValueKey('choice-LINUX')));
    await tester.pump();
    final reissued = tester
        .widget<SelectableText>(find.byKey(const ValueKey('add-command-text')))
        .data!;
    expect(reissued, isNot(first));
    expect(reissued, contains(name));
    expect(reissued, contains('install.sh'));
    await tester.tap(find.byKey(const ValueKey('add-close')));
    await tester.pump(const Duration(milliseconds: 300));

    final after = await tester.runAsync(
      () => client.node.getNode(GetNodeRequest(name: name)),
    );
    expect(after, isNotNull);
    expect(after!.node.invited, isTrue);

    await tester.pumpWidget(const SizedBox.shrink());
    await tester.pump(const Duration(seconds: 16));
  });

  testWidgets('Add node dialog fits a phone width and shows the QR', (
    tester,
  ) async {
    final name = 'ui-phone-${io.pid}';
    final client = hubClient();
    final invited = await tester.runAsync(
      () => client.node.inviteNode(InviteNodeRequest(name: name)),
    );
    expect(invited, isNotNull);
    final invite = invited!;

    // A phone-sized view: 411x891 logical pixels.
    tester.view.physicalSize = const Size(1233, 2673);
    tester.view.devicePixelRatio = 3;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);

    await tester.pumpWidget(
      MaterialApp(
        theme: appTheme(Brightness.dark),
        home: Scaffold(
          body: Center(
            child: Builder(
              builder: (context) => ElevatedButton(
                key: const ValueKey('open-add'),
                onPressed: () => showAddNode(
                  context,
                  client: client,
                  initialName: name,
                  reissue: true,
                  platform: TargetPlatform.linux,
                ),
                child: const Text('ADD'),
              ),
            ),
          ),
        ),
      ),
    );
    await tester.tap(find.byKey(const ValueKey('open-add')));
    await tester.pump(const Duration(milliseconds: 300));
    await wait(tester, find.byKey(const ValueKey('add-command')));
    expect(tester.takeException(), isNull);
    expect(find.byKey(const ValueKey('add-qr')), findsOneWidget);
    expect(
      tester
          .widget<SelectableText>(
            find.byKey(const ValueKey('add-command-text')),
          )
          .data,
      isNot(contains(invite.code)),
    );
    expect(
      tester
          .widget<SelectableText>(
            find.byKey(const ValueKey('add-command-text')),
          )
          .data,
      contains(name),
    );
    final box = tester.getSize(find.byKey(const ValueKey('add-command')));
    expect(box.width, lessThanOrEqualTo(411));
    final dialog = tester.getRect(find.byType(AlertDialog));
    expect(dialog.left, greaterThanOrEqualTo(0));
    expect(dialog.right, lessThanOrEqualTo(411));

    await tester.pumpWidget(const SizedBox.shrink());
    await tester.pump(const Duration(seconds: 16));
  });

  testWidgets('invited Rust node joins, reports metrics, and becomes live', (
    tester,
  ) async {
    expect(nodeBinary, isNotEmpty);
    expect(nodeHome, isNotEmpty);
    expect(tlsUrl, isNotEmpty);
    expect(caCert, isNotEmpty);
    expect(nodeListen, isNotEmpty);
    final name = 'ui-ready-${io.pid}';
    final client = hubClient();
    final feed = EventFeed((_) => const Stream<Event>.empty());
    addTearDown(feed.dispose);
    io.Process? node;
    addTearDown(() async {
      final process = node;
      if (process == null) return;
      process.kill(io.ProcessSignal.sigterm);
      await process.exitCode.timeout(const Duration(seconds: 15));
    });

    await tester.pumpWidget(
      MaterialApp(
        theme: appTheme(Brightness.dark),
        home: Scaffold(
          body: NodesTab(
            hub: HubProfile(
              id: 'rust-ready-e2e',
              name: 'Rust ready fixture',
              baseUrl: baseUrl,
              token: token,
            ),
            client: client,
            feed: feed,
            pollInterval: const Duration(seconds: 1),
          ),
        ),
      ),
    );
    await wait(tester, find.byKey(const ValueKey('add-node')));
    await tester.tap(find.byKey(const ValueKey('add-node')));
    await tester.pump(const Duration(milliseconds: 300));
    await tester.enterText(find.byKey(const ValueKey('add-name')), name);
    await tester.pump();
    await tester.tap(find.byKey(const ValueKey('add-create')));
    await wait(tester, find.byKey(const ValueKey('add-command')));
    await tester.tap(find.byKey(const ValueKey('choice-LINUX')));
    await tester.pump();
    final command = tester
        .widget<SelectableText>(find.byKey(const ValueKey('add-command-text')))
        .data!;
    final code = RegExp(
      "PROTECTORATE_JOIN_CODE='([^']+)'",
    ).firstMatch(command)?.group(1);
    expect(code, isNotNull, reason: 'Linux command has no join code');
    await wait(
      tester,
      find.text('waiting for the machine to run the installer'),
    );
    expect(
      tester.widget<LampLegend>(find.byKey(const ValueKey('stage-INVITED'))).on,
      isTrue,
    );

    // Use the code displayed by Flutter with the current Rust binary. This
    // joins the hub without installing a systemd service on this host.
    final joined = await tester.runAsync(
      () => io.Process.run(
        nodeBinary,
        [
          'node',
          'join',
          '--hub',
          tlsUrl,
          '--ca-cert',
          caCert,
          '--name',
          name,
          '--listen',
          nodeListen,
          '--home',
          nodeHome,
        ],
        environment: {'PROTECTORATE_JOIN_CODE': code!},
      ),
    );
    expect(joined, isNotNull);
    expect(joined!.exitCode, 0, reason: '${joined.stdout}\n${joined.stderr}');
    await wait(tester, find.textContaining('waiting for the hub to reach'));
    expect(
      tester.widget<LampLegend>(find.byKey(const ValueKey('stage-JOINED'))).on,
      isTrue,
    );

    node = await tester.runAsync(
      () => io.Process.start(nodeBinary, ['node', 'serve', '--home', nodeHome]),
    );
    node!.stdout.drain<void>();
    node.stderr.drain<void>();
    await wait(tester, find.text('IN SERVICE'), tries: 300);
    expect(find.text('DONE'), findsOneWidget);
    for (final stage in ['INVITED', 'JOINED', 'READY']) {
      expect(
        tester.widget<LampLegend>(find.byKey(ValueKey('stage-$stage'))).on,
        isTrue,
      );
    }
    final status = await tester.runAsync(
      () => client.node.getNode(GetNodeRequest(name: name)),
    );
    expect(status, isNotNull);
    expect(status!.node.invited, isFalse);
    expect(status.hasMetrics(), isTrue);
    expect(status.freshness, NodeFreshness.NODE_FRESHNESS_FRESH);

    await tester.tap(find.byKey(const ValueKey('add-close')));
    await tester.pump(const Duration(milliseconds: 300));
    final card = find.byKey(ValueKey('node-$name'));
    await wait(tester, find.descendant(of: card, matching: find.text('LIVE')));
    expect(
      find.descendant(of: card, matching: find.byKey(const ValueKey('cpu'))),
      findsOneWidget,
    );
    await tester.pumpWidget(const SizedBox.shrink());
    await tester.pump(const Duration(seconds: 16));
  });
}
