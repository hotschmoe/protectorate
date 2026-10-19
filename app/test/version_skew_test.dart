import 'dart:async';

import 'package:connectrpc/connect.dart';
import 'package:connectrpc/test.dart';
import 'package:fixnum/fixnum.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:protectorate/gen/protectorate/v1/events.connect.spec.dart';
import 'package:protectorate/gen/protectorate/v1/events.pb.dart';
import 'package:protectorate/gen/protectorate/v1/hub.connect.spec.dart';
import 'package:protectorate/gen/protectorate/v1/hub.pb.dart';
import 'package:protectorate/gen/protectorate/v1/node.connect.spec.dart';
import 'package:protectorate/gen/protectorate/v1/node.pb.dart';
import 'package:protectorate/gen/protectorate/v1/quota.connect.spec.dart';
import 'package:protectorate/gen/protectorate/v1/quota.pb.dart';
import 'package:protectorate/hubs/event_feed.dart';
import 'package:protectorate/hubs/hub_client.dart';
import 'package:protectorate/hubs/hub_profile.dart';
import 'package:protectorate/nodes/node_update.dart';
import 'package:protectorate/nodes/version_skew.dart';
import 'package:protectorate/screens/node_detail_screen.dart';
import 'package:protectorate/screens/nodes_tab.dart';
import 'package:protectorate/screens/overview_tab.dart';
import 'package:protectorate/theme/tokens.dart';
import 'package:protectorate/vfd/lamp.dart';

const _hub = HubProfile(id: '1', name: 'home', baseUrl: 'http://h');

NodeStatus _node(
  String name,
  String version, {
  NodeFreshness f = NodeFreshness.NODE_FRESHNESS_FRESH,
  bool local = false,
}) => NodeStatus(
  node: NodeInfo(name: name, address: '$name:7475', local: local),
  freshness: f,
  metrics: version.isEmpty
      ? null
      : NodeMetrics(
          os: 'linux',
          arch: 'amd64',
          agentVersion: version,
          cpu: CpuMetrics(logicalCores: 4, usagePercent: 10),
          memory: MemoryMetrics(totalBytes: Int64(8 << 30), usedPercent: 20),
        ),
);

Annunciator _skewLamp(WidgetTester t) =>
    t.widget<Annunciator>(find.byKey(const ValueKey('skew-annunciator')));

Widget _overview(
  List<NodeStatus> nodes,
  Versions v,
  StreamController<Event> e,
) {
  final transport = FakeTransportBuilder()
      .server(EventService.subscribe, (SubscribeRequest r, ctx) => e.stream)
      .unary(
        HubService.getStatus,
        (GetStatusRequest r, ctx) =>
            GetStatusResponse(version: v.hub, hostname: 'forge'),
      )
      .unary(
        NodeService.listNodes,
        (ListNodesRequest r, ctx) => ListNodesResponse(nodes: nodes),
      )
      .unary(
        QuotaService.getQuota,
        (GetQuotaRequest r, ctx) => GetQuotaResponse(),
      )
      .build();
  final feed = EventFeed(() => e.stream)..start();
  addTearDown(feed.dispose);
  return MaterialApp(
    theme: appTheme(Brightness.dark),
    home: Scaffold(
      body: OverviewTab(
        hub: _hub,
        client: HubClient.withTransport(transport),
        feed: feed,
        versions: v,
      ),
    ),
  );
}

void main() {
  group('skew detection', () {
    test('versionsDiffer ignores unknown versions', () {
      final cases = [
        ('v1', 'v1', false),
        ('v1', 'v2', true),
        ('', 'v2', false),
        ('v1', '', false),
        ('', '', false),
        ('v1.0.0-3-gabc', 'v1.0.0-3-gabc-dirty', true),
      ];
      for (final (a, b, want) in cases) {
        expect(versionsDiffer(a, b), want, reason: '$a vs $b');
      }
    });

    test('versionSkew', () {
      final cases =
          <(String, String, String, List<NodeStatus>, bool, List<String>)>[
            (
              'all match',
              'v2',
              'v2',
              [_node('a', 'v2'), _node('b', 'v2')],
              false,
              [],
            ),
            (
              'a node behind',
              'v2',
              'v2',
              [_node('a', 'v1'), _node('b', 'v2')],
              false,
              ['a'],
            ),
            ('app differs', 'v2', 'v1', [_node('a', 'v2')], true, []),
            ('both', 'v2', 'v1', [_node('a', 'v3')], true, ['a']),
            ('hub version unknown', '', 'v1', [_node('a', 'v1')], false, []),
            ('node without metrics', 'v2', 'v2', [_node('a', '')], false, []),
            (
              'invited node',
              'v2',
              'v2',
              [_node('a', 'v1', f: NodeFreshness.NODE_FRESHNESS_INVITED)],
              false,
              [],
            ),
            (
              'unreachable node keeps its last version',
              'v2',
              'v2',
              [_node('a', 'v1', f: NodeFreshness.NODE_FRESHNESS_UNREACHABLE)],
              false,
              ['a'],
            ),
          ];
      for (final (name, hub, app, nodes, wantApp, wantNodes) in cases) {
        final s = versionSkew(hubVersion: hub, appVersion: app, nodes: nodes);
        expect(s.app, wantApp, reason: name);
        expect(s.nodes, wantNodes, reason: name);
        expect(s.any, wantApp || wantNodes.isNotEmpty, reason: name);
      }
    });
  });

  group('overview', () {
    Future<void> pump(
      WidgetTester t,
      List<NodeStatus> nodes,
      Versions v,
    ) async {
      t.view.physicalSize = const Size(900, 1600);
      t.view.devicePixelRatio = 1;
      addTearDown(t.view.reset);
      final e = StreamController<Event>.broadcast();
      addTearDown(e.close);
      await t.pumpWidget(_overview(nodes, v, e));
      await t.pump(const Duration(milliseconds: 100));
      await t.pump(const Duration(milliseconds: 600));
    }

    testWidgets('annunciator is dark when everything matches', (t) async {
      await pump(t, [_node('a', 'v2')], const Versions(hub: 'v2', app: 'v2'));
      expect(_skewLamp(t).lit, isFalse);
      expect(find.byKey(const ValueKey('skew-a')), findsNothing);
    });

    testWidgets('a node behind lights it and badges its row', (t) async {
      await pump(t, [
        _node('a', 'v1'),
        _node('b', 'v2'),
      ], const Versions(hub: 'v2', app: 'v2'));
      final lamp = _skewLamp(t);
      expect(lamp.lit, isTrue);
      expect(lamp.tone, Tone.warn);
      expect(find.text('SKEW v1 / HUB v2'), findsOneWidget);
      expect(find.byKey(const ValueKey('skew-a')), findsOneWidget);
      expect(find.byKey(const ValueKey('skew-b')), findsNothing);
    });

    testWidgets('an app that differs from the hub lights it alone', (t) async {
      await pump(t, [_node('a', 'v2')], const Versions(hub: 'v2', app: 'v1'));
      expect(_skewLamp(t).lit, isTrue);
      expect(find.byKey(const ValueKey('skew-a')), findsNothing);
      expect(find.text('app v1'), findsOneWidget);
    });
  });

  group('nodes', () {
    testWidgets('card badge carries both versions', (t) async {
      Future<void> card(String hub) => t.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: NodeCard(
              status: _node('a', 'v1'),
              age: Duration.zero,
              onTap: () {},
              hubVersion: hub,
            ),
          ),
        ),
      );
      await card('v2');
      expect(find.text('SKEW v1 / HUB v2'), findsOneWidget);
      await card('v1');
      expect(find.byKey(const ValueKey('skew')), findsNothing);
      await card('');
      expect(find.byKey(const ValueKey('skew')), findsNothing);
    });

    testWidgets('detail shows both versions and offers the update', (t) async {
      var updates = 0;
      Future<void> detail({VoidCallback? onUpdate, NodeStatus? s}) =>
          t.pumpWidget(
            MaterialApp(
              home: Scaffold(
                body: NodeDetail(
                  status: s ?? _node('a', 'v1'),
                  age: Duration.zero,
                  now: DateTime(2026),
                  versions: const Versions(hub: 'v2', app: 'v2'),
                  onUpdate: onUpdate,
                ),
              ),
            ),
          );
      await detail(onUpdate: () => updates++);
      expect(find.text('v1'), findsOneWidget);
      expect(find.text('v2'), findsOneWidget);
      expect(find.text('SKEW'), findsOneWidget);
      await t.tap(find.byKey(const ValueKey('update-node')));
      expect(updates, 1);

      await detail();
      expect(find.byKey(const ValueKey('update-node')), findsNothing);

      await detail(
        onUpdate: () {},
        s: _node('a', 'v1', f: NodeFreshness.NODE_FRESHNESS_UNREACHABLE),
      );
      expect(find.byKey(const ValueKey('update-node')), findsNothing);
    });
  });

  group('update progress', () {
    test('labels and fractions', () {
      final cases = [
        (
          UpdateNodeProgress(
            phase: UpdatePhase.UPDATE_PHASE_SENDING,
            sentBytes: Int64(1 << 20),
            totalBytes: Int64(4 << 20),
          ),
          'SENDING  1.00 MB / 4.00 MB',
          25.0,
          false,
        ),
        (
          UpdateNodeProgress(phase: UpdatePhase.UPDATE_PHASE_VERIFYING),
          'VERIFYING',
          null,
          false,
        ),
        (
          UpdateNodeProgress(
            phase: UpdatePhase.UPDATE_PHASE_SWAPPING,
            previousVersion: 'v1',
            version: 'v2',
          ),
          'SWAPPED  v1 > v2',
          null,
          false,
        ),
        (
          UpdateNodeProgress(
            phase: UpdatePhase.UPDATE_PHASE_CONFIRMED,
            version: 'v2',
          ),
          'CONFIRMED  v2',
          null,
          true,
        ),
        (
          UpdateNodeProgress(phase: UpdatePhase.UPDATE_PHASE_ROLLED_BACK),
          'ROLLED BACK',
          null,
          true,
        ),
      ];
      for (final (p, label, pct, final_) in cases) {
        expect(updatePhaseLabel(p), label);
        expect(updatePercent(p), pct);
        expect(updateFinal(p.phase), final_);
      }
    });

    testWidgets('dialog follows the stream to confirmation', (t) async {
      final progress = StreamController<UpdateNodeProgress>();
      UpdateNodeRequest? seen;
      final transport = FakeTransportBuilder().server(NodeService.updateNode, (
        UpdateNodeRequest r,
        ctx,
      ) {
        seen = r;
        return progress.stream;
      }).build();
      var finished = 0;
      await t.pumpWidget(
        MaterialApp(
          theme: appTheme(Brightness.dark),
          home: Scaffold(
            body: NodeUpdateDialog(
              client: HubClient.withTransport(transport),
              node: 'pi',
              nodeVersion: 'v1',
              hubVersion: 'v2',
              onFinished: () => finished++,
            ),
          ),
        ),
      );
      await t.tap(find.byKey(const ValueKey('update-start')));
      await t.pump();
      progress.add(
        UpdateNodeProgress(
          phase: UpdatePhase.UPDATE_PHASE_SENDING,
          sentBytes: Int64(1 << 20),
          totalBytes: Int64(2 << 20),
        ),
      );
      await t.pump(const Duration(milliseconds: 50));
      expect(seen?.name, 'pi');
      expect(find.text('SENDING  1.00 MB / 2.00 MB'), findsOneWidget);
      final close = find.byKey(const ValueKey('update-close'));
      expect(t.widget<TextButton>(close).onPressed, isNull);

      progress.add(
        UpdateNodeProgress(
          phase: UpdatePhase.UPDATE_PHASE_CONFIRMED,
          version: 'v2',
        ),
      );
      unawaited(progress.close());
      await t.pump(const Duration(milliseconds: 50));
      expect(find.text('CONFIRMED  v2'), findsOneWidget);
      expect(finished, 1);
      expect(t.widget<TextButton>(close).onPressed, isNotNull);
    });

    testWidgets('dialog shows a refusal', (t) async {
      final transport = FakeTransportBuilder()
          .server(
            NodeService.updateNode,
            (UpdateNodeRequest r, ctx) => Stream<UpdateNodeProgress>.error(
              ConnectException(Code.permissionDenied, 'owner token required'),
            ),
          )
          .build();
      await t.pumpWidget(
        MaterialApp(
          theme: appTheme(Brightness.dark),
          home: Scaffold(
            body: NodeUpdateDialog(
              client: HubClient.withTransport(transport),
              node: 'pi',
              nodeVersion: 'v1',
              hubVersion: 'v2',
            ),
          ),
        ),
      );
      await t.tap(find.byKey(const ValueKey('update-start')));
      await t.runAsync(
        () => Future<void>.delayed(const Duration(milliseconds: 50)),
      );
      await t.pump();
      expect(find.textContaining('owner token required'), findsOneWidget);
      expect(find.text('RETRY'), findsOneWidget);
    });
  });
}
