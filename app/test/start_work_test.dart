import 'package:connectrpc/test.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:protectorate/gen/protectorate/v1/events.pb.dart';
import 'package:protectorate/gen/protectorate/v1/node.connect.spec.dart';
import 'package:protectorate/gen/protectorate/v1/node.pb.dart';
import 'package:protectorate/hubs/event_feed.dart';
import 'package:protectorate/gen/protectorate/v1/sleeve.connect.spec.dart';
import 'package:protectorate/gen/protectorate/v1/sleeve.pb.dart';
import 'package:protectorate/hubs/hub_client.dart';
import 'package:protectorate/hubs/hub_profile.dart';
import 'package:protectorate/screens/sleeves_screen.dart';
import 'package:protectorate/sleeves/node_fence.dart';
import 'package:protectorate/sleeves/spawn_dialog.dart';

const _hub = HubProfile(id: '1', name: 'home', baseUrl: 'http://h');

final _workspaces = [
  Workspace(
    name: 'app',
    path: '/state/workspaces/app',
    gitBranch: 'main',
    gitRemote: 'https://example.com/org/app.git',
    requiredName: 'app',
  ),
  Workspace(name: 'shared', path: '/srv/shared'),
  Workspace(
    name: 'busy',
    path: '/state/workspaces/busy',
    sleeve: 'busy',
    busy: true,
    requiredName: 'busy',
  ),
];

void main() {
  group('suggestSleeveName', () {
    for (final (input, want) in [
      ('https://github.com/org/My_Repo.git', 'my-repo'),
      ('git@github.com:org/app.git', 'app'),
      ('git@host:repo', 'repo'),
      ('/home/me/src/Proj.X/', 'proj-x'),
      ('/', ''),
      ('', ''),
      ('https://h/${'a' * 60}', 'a' * 40),
    ]) {
      test(input, () => expect(suggestSleeveName(input), want));
    }
  });

  group('spawn dialog', () {
    testWidgets('picks an existing workspace and names the sleeve after it', (
      t,
    ) async {
      SpawnSleeveRequest? got;
      await t.pumpWidget(
        MaterialApp(
          home: Builder(
            builder: (context) => TextButton(
              onPressed: () async => got = await showDialog<SpawnSleeveRequest>(
                context: context,
                builder: (_) => SpawnSleeveDialog(workspaces: _workspaces),
              ),
              child: const Text('open'),
            ),
          ),
        ),
      );
      await t.tap(find.text('open'));
      await t.pumpAndSettle();

      await t.tap(find.byKey(const ValueKey('sleeve-spawn')));
      await t.pumpAndSettle();
      expect(find.text('pick a workspace'), findsOneWidget);

      await t.tap(find.byKey(const ValueKey('sleeve-existing')));
      await t.pumpAndSettle();
      expect(find.text('busy (in use)'), findsOneWidget);
      await t.tap(find.text('shared').last);
      await t.pumpAndSettle();
      expect(find.textContaining('/srv/shared'), findsWidgets);
      expect(
        t
            .widget<TextFormField>(find.byKey(const ValueKey('sleeve-name')))
            .controller!
            .text,
        'shared',
      );

      await t.tap(find.byKey(const ValueKey('sleeve-spawn')));
      await t.pumpAndSettle();
      expect(got?.workspace, '/srv/shared');
      expect(got?.name, 'shared');
    });

    testWidgets('a hub-owned workspace fixes the sleeve name', (t) async {
      await t.pumpWidget(
        MaterialApp(home: SpawnSleeveDialog(workspaces: _workspaces)),
      );
      await t.tap(find.byKey(const ValueKey('sleeve-existing')));
      await t.pumpAndSettle();
      await t.tap(find.text('app').last);
      await t.pumpAndSettle();
      final name = t.widget<TextFormField>(
        find.byKey(const ValueKey('sleeve-name')),
      );
      expect(name.controller!.text, 'app');
      expect(find.textContaining('branch main'), findsOneWidget);
      expect(find.textContaining('example.com/org/app.git'), findsOneWidget);
      expect(
        find.text('This workspace belongs to a sleeve of this name'),
        findsOneWidget,
      );
    });

    testWidgets('clone suggests a name from the URL', (t) async {
      SpawnSleeveRequest? got;
      await t.pumpWidget(
        MaterialApp(
          home: Builder(
            builder: (context) => TextButton(
              onPressed: () async => got = await showDialog<SpawnSleeveRequest>(
                context: context,
                builder: (_) => const SpawnSleeveDialog(),
              ),
              child: const Text('open'),
            ),
          ),
        ),
      );
      await t.tap(find.text('open'));
      await t.pumpAndSettle();
      expect(find.text('EXISTING'), findsNothing);

      await t.enterText(
        find.byKey(const ValueKey('sleeve-workspace')),
        'https://github.com/org/Tool.git',
      );
      await t.pump();
      expect(
        t
            .widget<TextFormField>(find.byKey(const ValueKey('sleeve-name')))
            .controller!
            .text,
        'tool',
      );
      await t.enterText(find.byKey(const ValueKey('sleeve-name')), 'mine');
      await t.enterText(
        find.byKey(const ValueKey('sleeve-workspace')),
        'https://github.com/org/Other.git',
      );
      await t.tap(find.byKey(const ValueKey('sleeve-spawn')));
      await t.pumpAndSettle();
      expect(got?.workspace, 'https://github.com/org/Other.git');
      expect(got?.name, 'mine');
    });

    testWidgets('path must be absolute and clone must not be a path', (
      t,
    ) async {
      await t.pumpWidget(const MaterialApp(home: SpawnSleeveDialog()));
      await t.tap(find.text('PATH'));
      await t.pumpAndSettle();
      await t.enterText(find.byKey(const ValueKey('sleeve-workspace')), 'src');
      await t.tap(find.byKey(const ValueKey('sleeve-spawn')));
      await t.pumpAndSettle();
      expect(find.text('an absolute path on the hub'), findsOneWidget);

      await t.tap(find.text('CLONE'));
      await t.pumpAndSettle();
      await t.enterText(
        find.byKey(const ValueKey('sleeve-workspace')),
        '/home/me/src',
      );
      await t.tap(find.byKey(const ValueKey('sleeve-spawn')));
      await t.pumpAndSettle();
      expect(find.textContaining('use Path for a directory'), findsOneWidget);
    });
  });

  group('sleeves tab', () {
    final nodes = [
      NodeStatus(
        node: NodeInfo(name: 'zeta', local: false),
        freshness: NodeFreshness.NODE_FRESHNESS_UNREACHABLE,
        metrics: NodeMetrics(sleeves: [Sleeve(name: 'remote-one')]),
      ),
      NodeStatus(
        node: NodeInfo(name: 'forge', local: true),
        freshness: NodeFreshness.NODE_FRESHNESS_FRESH,
      ),
      NodeStatus(
        node: NodeInfo(name: 'idle-box'),
        freshness: NodeFreshness.NODE_FRESHNESS_FRESH,
      ),
    ];

    test('the hub node leads, nodes without sleeves get no fence', () {
      final g = groupSleeves(nodes, [Sleeve(name: 'alice')], 'home');
      expect(g.map((x) => x.name), ['forge', 'zeta']);
      expect(g.first.sleeves.single.name, 'alice');
      expect(g.last.sleeves.single.name, 'remote-one');
      expect(groupSleeves(const [], const [], 'home').single.name, 'home');
    });

    Future<List<SpawnSleeveRequest>> pump(WidgetTester t) async {
      final spawned = <SpawnSleeveRequest>[];
      final transport = FakeTransportBuilder()
          .unary(
            NodeService.listNodes,
            (ListNodesRequest r, ctx) => ListNodesResponse(nodes: nodes),
          )
          .unary(
            SleeveService.listSleeves,
            (ListSleevesRequest r, ctx) => ListSleevesResponse(
              sleeves: [
                Sleeve(
                  name: 'alice',
                  harness: Harness.HARNESS_CODEX,
                  workspace: '/w/a',
                  state: SleeveState.SLEEVE_STATE_RUNNING,
                ),
                Sleeve(
                  name: 'bob',
                  workspace: '/w/b',
                  state: SleeveState.SLEEVE_STATE_EXITED,
                ),
              ],
            ),
          )
          .unary(
            SleeveService.listWorkspaces,
            (ListWorkspacesRequest r, ctx) =>
                ListWorkspacesResponse(workspaces: _workspaces),
          )
          .unary(SleeveService.spawnSleeve, (SpawnSleeveRequest r, ctx) {
            spawned.add(r);
            return SpawnSleeveResponse(
              sleeve: Sleeve(name: r.name, session: 'sleeve-${r.name}'),
            );
          })
          .build();
      final feed = EventFeed(() => const Stream<Event>.empty());
      addTearDown(feed.dispose);
      await t.pumpWidget(
        MaterialApp(
          home: SleevesScreen(
            hub: _hub,
            client: HubClient.withTransport(transport),
            feed: feed,
            embedded: true,
          ),
        ),
      );
      await t.pumpAndSettle();
      return spawned;
    }

    testWidgets('each node has a fence holding its own sleeves', (t) async {
      await pump(t);
      Finder inFence(String node, String sleeve) => find.descendant(
        of: find.byKey(ValueKey('fence-$node')),
        matching: find.byKey(ValueKey('sleeve-$sleeve')),
      );
      expect(inFence('forge', 'alice'), findsOneWidget);
      expect(inFence('forge', 'bob'), findsOneWidget);
      expect(inFence('zeta', 'remote-one'), findsOneWidget);
      expect(inFence('forge', 'remote-one'), findsNothing);
      expect(find.byKey(const ValueKey('fence-idle-box')), findsNothing);
      expect(find.text('2 SLEEVES, 1 RUNNING'), findsOneWidget);
      expect(find.text('1 SLEEVES, 0 RUNNING'), findsOneWidget);
      expect(
        t.widget<Opacity>(find.byKey(const ValueKey('fence-zeta'))).opacity,
        lessThan(1),
      );
      expect(
        t.widget<Opacity>(find.byKey(const ValueKey('fence-forge'))).opacity,
        1,
      );
      expect(find.byKey(const ValueKey('fence-new-forge')), findsOneWidget);
      expect(find.byKey(const ValueKey('fence-new-zeta')), findsNothing);
      expect(find.text('NEW SLEEVE'), findsOneWidget);
      expect(find.text('RAW SESSIONS'), findsOneWidget);
    });

    testWidgets('the fence plus starts a sleeve on the picked workspace', (
      t,
    ) async {
      final spawned = await pump(t);
      await t.tap(find.byKey(const ValueKey('fence-new-forge')));
      await t.pumpAndSettle();
      expect(find.text('Start a sleeve'), findsOneWidget);
      await t.tap(find.byKey(const ValueKey('sleeve-existing')));
      await t.pumpAndSettle();
      await t.tap(find.text('shared').last);
      await t.pumpAndSettle();
      await t.tap(find.byKey(const ValueKey('sleeve-spawn')));
      await t.pump();
      await t.pump(const Duration(milliseconds: 100));
      expect(spawned.single.workspace, '/srv/shared');
      expect(spawned.single.name, 'shared');
    });
  });
}
