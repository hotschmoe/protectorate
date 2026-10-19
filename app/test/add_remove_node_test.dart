import 'dart:async';

import 'package:connectrpc/connect.dart';
import 'package:connectrpc/test.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:protobuf/well_known_types/google/protobuf/timestamp.pb.dart';
import 'package:protectorate/gen/protectorate/v1/node.connect.spec.dart';
import 'package:protectorate/gen/protectorate/v1/node.pb.dart';
import 'package:protectorate/hubs/hub_client.dart';
import 'package:protectorate/nodes/add_node.dart';
import 'package:protectorate/nodes/remove_node.dart';
import 'package:protectorate/screens/node_detail_screen.dart';
import 'package:protectorate/screens/nodes_tab.dart';
import 'package:protectorate/theme/tokens.dart';
import 'package:protectorate/vfd/lamp.dart';

const _win =
    r"$env:PROTECTORATE_ENROLL_KEY='hskey-1'; $env:PROTECTORATE_NODE_NAME='office-arm'; irm 'http://hub:7470/install.ps1' | iex";
const _sh =
    "curl -fsSL 'http://hub:7470/install.sh' | PROTECTORATE_ENROLL_KEY='hskey-1' sh -s -- --name 'office-arm'";

InviteNodeResponse _invite(String name) => InviteNodeResponse(
  node: NodeInfo(name: name, invited: true),
  key: 'hskey-1',
  controlUrl: 'http://hs:7480',
  installUrl: 'http://hub:7470',
  expiresAt: Timestamp.fromDateTime(DateTime(2026, 10, 20, 14, 32)),
  commands: [
    InstallCommand(
      platform: InstallPlatform.INSTALL_PLATFORM_WINDOWS,
      shell: 'PowerShell',
      command: _win,
      note: 'Paste into a normal PowerShell.',
    ),
    InstallCommand(
      platform: InstallPlatform.INSTALL_PLATFORM_LINUX,
      shell: 'sh',
      command: _sh,
      note: 'Paste into a terminal.',
    ),
    InstallCommand(
      platform: InstallPlatform.INSTALL_PLATFORM_TERMUX,
      shell: 'Termux',
      command: _sh,
      note: 'Paste into Termux.',
    ),
  ],
);

Widget _app(Widget child, {Size size = const Size(1280, 900)}) => MediaQuery(
  data: MediaQueryData(size: size),
  child: MaterialApp(
    theme: appTheme(Brightness.dark),
    home: Scaffold(body: child),
  ),
);

bool _lit(WidgetTester t, String label) =>
    t.widget<Annunciator>(find.widgetWithText(Annunciator, label)).lit;

Tone _stageTone(WidgetTester t, String label) =>
    t.widget<LampLegend>(find.byKey(ValueKey('stage-$label'))).tone;

void main() {
  test('the platform follows the machine', () {
    const cases = {
      TargetPlatform.windows: InstallPlatform.INSTALL_PLATFORM_WINDOWS,
      TargetPlatform.linux: InstallPlatform.INSTALL_PLATFORM_LINUX,
      TargetPlatform.android: InstallPlatform.INSTALL_PLATFORM_TERMUX,
      TargetPlatform.macOS: InstallPlatform.INSTALL_PLATFORM_LINUX,
      TargetPlatform.iOS: InstallPlatform.INSTALL_PLATFORM_LINUX,
    };
    cases.forEach((p, want) => expect(platformFor(p), want, reason: '$p'));
  });

  test('node names follow the hub rule', () {
    const cases = {
      '': null,
      'office-arm': null,
      'a': null,
      'pc2': null,
      'a_b': 'Lowercase letters, digits and -, not at either end.',
      'a.b': 'Lowercase letters, digits and -, not at either end.',
      '-x': 'Lowercase letters, digits and -, not at either end.',
      'x-': 'Lowercase letters, digits and -, not at either end.',
      "a'b": 'Lowercase letters, digits and -, not at either end.',
      r'$(id)': 'Lowercase letters, digits and -, not at either end.',
    };
    cases.forEach((n, want) => expect(nodeNameError(n), want, reason: n));
    expect(nodeNameError('a' * 64), 'At most 63 characters.');
  });

  group('add node', () {
    testWidgets('invites, shows the line for the machine and follows setup', (
      t,
    ) async {
      final setup = StreamController<NodeSetupProgress>();
      InviteNodeRequest? asked;
      WatchNodeSetupRequest? watched;
      final transport = FakeTransportBuilder()
          .unary(NodeService.inviteNode, (InviteNodeRequest r, _) async {
            asked = r;
            return _invite(r.name);
          })
          .server(NodeService.watchNodeSetup, (WatchNodeSetupRequest r, _) {
            watched = r;
            return setup.stream;
          })
          .build();
      final copied = <String>[];
      t.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        SystemChannels.platform,
        (call) async {
          if (call.method == 'Clipboard.setData') {
            copied.add((call.arguments as Map)['text'] as String);
          }
          return null;
        },
      );
      var changed = 0;
      await t.pumpWidget(
        _app(
          AddNodeDialog(
            client: HubClient.withTransport(transport),
            platform: TargetPlatform.windows,
            onChanged: () => changed++,
          ),
        ),
      );
      expect(_lit(t, 'WINDOWS'), isTrue);
      expect(_lit(t, 'LINUX'), isFalse);
      final create = find.byKey(const ValueKey('add-create'));
      expect(t.widget<FilledButton>(create).onPressed, isNull);

      await t.enterText(find.byKey(const ValueKey('add-name')), 'Bad_Name');
      await t.pump();
      expect(find.textContaining('Lowercase letters'), findsOneWidget);
      expect(t.widget<FilledButton>(create).onPressed, isNull);

      await t.enterText(find.byKey(const ValueKey('add-name')), 'office-arm');
      await t.pump();
      await t.tap(create);
      await t.pump();
      await t.pump();
      expect(asked?.name, 'office-arm');
      expect(watched?.name, 'office-arm');
      expect(changed, 1);
      expect(find.text(_win), findsOneWidget);
      expect(find.text('PASTE INTO POWERSHELL'), findsOneWidget);
      expect(find.text('KEY UNTIL 14:32'), findsOneWidget);
      expect(find.byKey(const ValueKey('add-qr')), findsNothing);

      await t.tap(find.byKey(const ValueKey('add-copy')));
      await t.pump();
      expect(copied, [_win]);
      expect(find.text('COPIED'), findsOneWidget);
      await t.pump(const Duration(seconds: 3));

      await t.tap(find.byKey(const ValueKey('choice-LINUX')));
      await t.pump();
      expect(find.text(_sh), findsOneWidget);
      expect(find.text('PASTE INTO SH'), findsOneWidget);

      setup.add(
        NodeSetupProgress(
          stage: NodeSetupStage.NODE_SETUP_STAGE_INVITED,
          message: 'waiting for the machine to run the installer',
          status: NodeStatus(node: NodeInfo(name: 'office-arm', invited: true)),
        ),
      );
      await t.pump();
      expect(_stageTone(t, 'INVITED'), Tone.info);
      expect(_stageTone(t, 'JOINED'), Tone.ghost);

      setup.add(
        NodeSetupProgress(
          stage: NodeSetupStage.NODE_SETUP_STAGE_ATTENTION,
          problems: ['the agent runs v0.9 and the hub v1.0: update the node'],
          status: NodeStatus(
            node: NodeInfo(name: 'office-arm'),
            metrics: NodeMetrics(os: 'windows'),
          ),
        ),
      );
      await t.pump();
      expect(_stageTone(t, 'JOINED'), Tone.ok);
      expect(_stageTone(t, 'FIRST METRICS'), Tone.warn);
      expect(find.textContaining('update the node'), findsOneWidget);

      setup.add(
        NodeSetupProgress(
          stage: NodeSetupStage.NODE_SETUP_STAGE_READY,
          message: 'windows/arm64, agent v1.0',
          status: NodeStatus(
            node: NodeInfo(name: 'office-arm'),
            metrics: NodeMetrics(os: 'windows'),
          ),
        ),
      );
      await t.pump();
      expect(_stageTone(t, 'READY'), Tone.ok);
      expect(find.text('IN SERVICE'), findsOneWidget);
      expect(find.text('DONE'), findsOneWidget);
      expect(changed, 2);
      await setup.close();
    });

    testWidgets('a phone shows the line as a QR code too', (t) async {
      t.view.physicalSize = const Size(412, 900);
      t.view.devicePixelRatio = 1;
      addTearDown(t.view.reset);
      final transport = FakeTransportBuilder()
          .unary(
            NodeService.inviteNode,
            (InviteNodeRequest r, _) async => _invite(r.name),
          )
          .server(
            NodeService.watchNodeSetup,
            (WatchNodeSetupRequest r, _) =>
                const Stream<NodeSetupProgress>.empty(),
          )
          .build();
      await t.pumpWidget(
        MaterialApp(
          theme: appTheme(Brightness.light),
          home: Scaffold(
            body: AddNodeDialog(
              client: HubClient.withTransport(transport),
              platform: TargetPlatform.android,
              initialName: 'phone',
              reissue: true,
            ),
          ),
        ),
      );
      await t.pump();
      await t.pump();
      expect(find.text('Reissue key'), findsOneWidget);
      expect(find.text('PASTE INTO TERMUX'), findsOneWidget);
      expect(find.byKey(const ValueKey('add-qr')), findsOneWidget);
    });

    testWidgets('a token that is not the owner is told what to do', (t) async {
      final transport = FakeTransportBuilder()
          .unary(
            NodeService.inviteNode,
            (InviteNodeRequest r, _) async =>
                throw ConnectException(Code.permissionDenied, 'owner-only'),
          )
          .build();
      await t.pumpWidget(
        _app(
          AddNodeDialog(
            client: HubClient.withTransport(transport),
            initialName: 'box',
          ),
        ),
      );
      await t.tap(find.byKey(const ValueKey('add-create')));
      await t.runAsync(
        () => Future<void>.delayed(const Duration(milliseconds: 20)),
      );
      await t.pump();
      expect(find.textContaining('owner token'), findsOneWidget);
      expect(find.byKey(const ValueKey('add-name')), findsOneWidget);
    });
  });

  group('remove node', () {
    NodeStatus status({
      bool invited = false,
      NodeFreshness f = NodeFreshness.NODE_FRESHNESS_FRESH,
    }) => NodeStatus(
      node: NodeInfo(name: 'box', invited: invited),
      freshness: f,
    );

    testWidgets('needs the name typed, then drains, uninstalls and removes', (
      t,
    ) async {
      final steps = StreamController<RemoveNodeProgress>();
      RemoveNodeRequest? asked;
      final transport = FakeTransportBuilder().server(NodeService.removeNode, (
        RemoveNodeRequest r,
        _,
      ) {
        asked = r;
        return steps.stream;
      }).build();
      bool? result;
      await t.pumpWidget(
        _app(
          Builder(
            builder: (context) => TextButton(
              onPressed: () async => result = await showRemoveNode(
                context,
                client: HubClient.withTransport(transport),
                status: status(),
              ),
              child: const Text('open'),
            ),
          ),
        ),
      );
      await t.tap(find.text('open'));
      await t.pumpAndSettle();
      final go = find.byKey(const ValueKey('remove-go'));
      expect(t.widget<FilledButton>(go).onPressed, isNull);
      expect(_lit(t, 'LET THEM FINISH'), isTrue);
      expect(_lit(t, 'ALSO UNINSTALL'), isTrue);

      await t.enterText(find.byKey(const ValueKey('remove-confirm')), 'bo');
      await t.pump();
      expect(t.widget<FilledButton>(go).onPressed, isNull);
      await t.enterText(find.byKey(const ValueKey('remove-confirm')), 'box');
      await t.tap(find.byKey(const ValueKey('choice-CANCEL THEM')));
      await t.pump();
      await t.tap(go);
      await t.pump();
      expect(asked?.name, 'box');
      expect(asked?.jobs, RemoveJobs.REMOVE_JOBS_CANCEL);
      expect(asked?.uninstall, isTrue);

      steps.add(
        RemoveNodeProgress(
          phase: RemovePhase.REMOVE_PHASE_DRAINING,
          runningJobs: 1,
          message: 'cancelling j1',
        ),
      );
      await t.pump();
      expect(find.text('cancelling j1 (1 running)'), findsOneWidget);
      expect(
        t
            .widget<TextButton>(find.byKey(const ValueKey('remove-close')))
            .onPressed,
        isNull,
      );
      steps.add(
        RemoveNodeProgress(
          phase: RemovePhase.REMOVE_PHASE_REMOVED,
          message: 'removed box from the inventory',
        ),
      );
      await steps.close();
      await t.pump();
      await t.pump();
      expect(find.text('REMOVED'), findsOneWidget);
      await t.tap(find.text('DONE'));
      await t.pumpAndSettle();
      expect(result, isTrue);
    });

    testWidgets('an offline node cannot be asked to uninstall', (t) async {
      RemoveNodeRequest? asked;
      final transport = FakeTransportBuilder().server(NodeService.removeNode, (
        RemoveNodeRequest r,
        _,
      ) {
        asked = r;
        return Stream.value(
          RemoveNodeProgress(phase: RemovePhase.REMOVE_PHASE_REMOVED),
        );
      }).build();
      await t.pumpWidget(
        _app(
          RemoveNodeDialog(
            client: HubClient.withTransport(transport),
            status: status(f: NodeFreshness.NODE_FRESHNESS_UNREACHABLE),
          ),
        ),
      );
      expect(_lit(t, 'ALSO UNINSTALL'), isFalse);
      expect(find.textContaining('is offline'), findsOneWidget);
      await t.enterText(find.byKey(const ValueKey('remove-confirm')), 'box');
      await t.pump();
      await t.tap(find.byKey(const ValueKey('remove-go')));
      await t.pump();
      expect(asked?.uninstall, isFalse);
      expect(asked?.jobs, RemoveJobs.REMOVE_JOBS_WAIT);
    });

    testWidgets('a pending invite is cancelled without typing', (t) async {
      RemoveNodeRequest? asked;
      final transport = FakeTransportBuilder().server(NodeService.removeNode, (
        RemoveNodeRequest r,
        _,
      ) {
        asked = r;
        return Stream.value(
          RemoveNodeProgress(phase: RemovePhase.REMOVE_PHASE_REMOVED),
        );
      }).build();
      await t.pumpWidget(
        _app(
          RemoveNodeDialog(
            client: HubClient.withTransport(transport),
            status: status(
              invited: true,
              f: NodeFreshness.NODE_FRESHNESS_INVITED,
            ),
          ),
        ),
      );
      expect(find.byKey(const ValueKey('remove-confirm')), findsNothing);
      await t.tap(find.text('CANCEL INVITE'));
      await t.pump();
      expect(asked?.name, 'box');
      expect(asked?.uninstall, isFalse);
    });

    testWidgets('a refusal is shown and can be retried', (t) async {
      final transport = FakeTransportBuilder()
          .server(
            NodeService.removeNode,
            (RemoveNodeRequest r, _) => Stream<RemoveNodeProgress>.error(
              ConnectException(
                Code.failedPrecondition,
                '1 job(s) running on box',
              ),
            ),
          )
          .build();
      await t.pumpWidget(
        _app(
          RemoveNodeDialog(
            client: HubClient.withTransport(transport),
            status: status(),
          ),
        ),
      );
      await t.enterText(find.byKey(const ValueKey('remove-confirm')), 'box');
      await t.pump();
      await t.tap(find.byKey(const ValueKey('remove-go')));
      await t.runAsync(
        () => Future<void>.delayed(const Duration(milliseconds: 20)),
      );
      await t.pump();
      expect(find.textContaining('1 job(s) running'), findsOneWidget);
      expect(find.text('RETRY'), findsOneWidget);
    });
  });

  group('inventory actions', () {
    testWidgets('a pending node offers reissue and cancel', (t) async {
      var reissued = 0, removed = 0;
      await t.pumpWidget(
        _app(
          NodeDetail(
            status: NodeStatus(
              node: NodeInfo(name: 'box', invited: true),
              freshness: NodeFreshness.NODE_FRESHNESS_INVITED,
              error: 'invited, not yet joined',
            ),
            age: Duration.zero,
            now: DateTime(2026),
            onReissue: () => reissued++,
            onRemove: () => removed++,
          ),
        ),
      );
      expect(find.text('PENDING INVITE'), findsOneWidget);
      await t.tap(find.byKey(const ValueKey('reissue-key')));
      await t.tap(find.text('CANCEL INVITE'));
      expect((reissued, removed), (1, 1));
    });

    testWidgets('a pending card reads PENDING', (t) async {
      await t.pumpWidget(
        _app(
          NodeCard(
            status: NodeStatus(
              node: NodeInfo(name: 'box', invited: true),
              freshness: NodeFreshness.NODE_FRESHNESS_INVITED,
            ),
            age: Duration.zero,
            onTap: () {},
          ),
        ),
      );
      expect(find.text('PENDING'), findsOneWidget);
      expect(find.byKey(const ValueKey('pending-note')), findsOneWidget);
    });
  });
}
