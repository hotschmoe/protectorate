import 'package:connectrpc/test.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:protectorate/gen/protectorate/v1/session.connect.spec.dart';
import 'package:protectorate/gen/protectorate/v1/session.pb.dart';
import 'package:protectorate/gen/protectorate/v1/sleeve.connect.spec.dart';
import 'package:protectorate/gen/protectorate/v1/sleeve.pb.dart';
import 'package:protectorate/hubs/hub_client.dart';
import 'package:protectorate/hubs/hub_profile.dart';
import 'package:protectorate/screens/sessions_tab.dart';
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

  testWidgets('sessions tab starts a sleeve from START WORK', (t) async {
    final spawned = <SpawnSleeveRequest>[];
    final transport = FakeTransportBuilder()
        .unary(
          SessionService.listSessions,
          (ListSessionsRequest r, ctx) => ListSessionsResponse(),
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
    await t.pumpWidget(
      MaterialApp(
        home: SessionsTab(
          hub: _hub,
          client: HubClient.withTransport(transport),
        ),
      ),
    );
    await t.pumpAndSettle();
    expect(find.text('START WORK'), findsOneWidget);
    expect(find.text('NEW RAW SESSION'), findsOneWidget);
    expect(find.byKey(const ValueKey('sleeve-alice')), findsOneWidget);

    await t.tap(find.byKey(const ValueKey('start-work')));
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
}
