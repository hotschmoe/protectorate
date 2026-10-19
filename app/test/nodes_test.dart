import 'dart:async';

import 'package:connectrpc/test.dart';
import 'package:fixnum/fixnum.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:protectorate/gen/protectorate/v1/events.connect.spec.dart';
import 'package:protectorate/gen/protectorate/v1/events.pb.dart';
import 'package:protectorate/gen/protectorate/v1/node.connect.spec.dart';
import 'package:protectorate/gen/protectorate/v1/node.pb.dart';
import 'package:protectorate/hubs/event_feed.dart';
import 'package:protectorate/gen/protectorate/v1/sleeve.connect.spec.dart';
import 'package:protectorate/gen/protectorate/v1/sleeve.pb.dart';
import 'package:protectorate/hubs/hub_client.dart';
import 'package:protectorate/hubs/hub_profile.dart';
import 'package:protectorate/screens/sleeves_screen.dart';
import 'package:protectorate/nodes/node_view.dart';
import 'package:protectorate/screens/node_detail_screen.dart';
import 'package:protectorate/screens/nodes_tab.dart';

const _hub = HubProfile(id: '1', name: 'home', baseUrl: 'http://h');

NodeMetrics _metrics({double disk = 40, double mem = 30, bool gpu = false}) =>
    NodeMetrics(
      os: 'linux',
      arch: 'arm64',
      platform: 'ubuntu 24.04',
      cpu: CpuMetrics(logicalCores: 8, usagePercent: 12.5),
      memory: MemoryMetrics(
        totalBytes: Int64(16 << 30),
        usedBytes: Int64(5 << 30),
        usedPercent: mem,
      ),
      filesystems: [
        FilesystemMetrics(
          mount: '/',
          totalBytes: Int64(100 << 30),
          usedBytes: Int64(40 << 30),
          usedPercent: 20,
        ),
        FilesystemMetrics(
          mount: '/data',
          primary: true,
          totalBytes: Int64(1000 << 30),
          usedBytes: Int64(900 << 30),
          usedPercent: disk,
        ),
      ],
      gpus: gpu
          ? [
              GpuMetrics(
                name: 'RTX',
                utilizationPercent: 55,
                memoryUsedBytes: Int64(8 << 30),
                memoryTotalBytes: Int64(24 << 30),
                temperatureCelsius: -1,
              ),
            ]
          : [],
      engine: ContainerEngine(
        kind: 'docker',
        containers: [
          ContainerMetrics(name: 'a', writableLayerBytes: Int64(5 << 30)),
          ContainerMetrics(name: 'b', writableLayerBytes: Int64(50 << 30)),
        ],
      ),
    );

NodeStatus _status(
  String name,
  NodeFreshness f, {
  NodeMetrics? metrics,
  List<NodeAlert> alerts = const [],
  String error = '',
  String tier = 'shared',
}) => NodeStatus(
  node: NodeInfo(name: name, address: '$name:7475', tier: tier),
  metrics: metrics,
  freshness: f,
  alerts: alerts,
  error: error,
);

Widget _host(Widget child) => MaterialApp(
  home: Scaffold(body: SingleChildScrollView(child: child)),
);

void main() {
  group('node card', () {
    testWidgets('fresh node shows live numbers and no alerts', (t) async {
      await t.pumpWidget(
        _host(
          NodeCard(
            status: _status(
              'alpha',
              NodeFreshness.NODE_FRESHNESS_FRESH,
              metrics: _metrics(gpu: true),
            ),
            age: const Duration(seconds: 3),
            onTap: () {},
          ),
        ),
      );
      expect(find.text('LIVE'), findsOneWidget);
      expect(find.text('linux/arm64  ubuntu 24.04  up 0m'), findsOneWidget);
      expect(find.text('12.5%  8c'), findsOneWidget);
      expect(find.text('40.0%  /data'), findsOneWidget);
      expect(find.text('55.0%'), findsOneWidget);
      expect(find.text('8.00 GB / 24.0 GB'), findsOneWidget);
      expect(find.text('docker: 2 containers'), findsOneWidget);
      expect(find.textContaining('DISK'), findsOneWidget);
      expect(find.byKey(const ValueKey('alert-DISK 40% /data')), findsNothing);
    });

    testWidgets('tier badge marks care nodes only', (t) async {
      for (final (tier, want) in [
        ('care', findsOneWidget),
        ('shared', findsNothing),
      ]) {
        await t.pumpWidget(
          _host(
            NodeCard(
              status: _status(
                'kidsbox',
                NodeFreshness.NODE_FRESHNESS_FRESH,
                metrics: _metrics(),
                tier: tier,
              ),
              age: const Duration(seconds: 3),
              onTap: () {},
            ),
          ),
        );
        expect(find.byKey(const ValueKey('tier-care')), want, reason: tier);
      }
    });

    testWidgets('stale node is labeled with its age', (t) async {
      await t.pumpWidget(
        _host(
          NodeCard(
            status: _status(
              'beta',
              NodeFreshness.NODE_FRESHNESS_STALE,
              metrics: _metrics(),
            ),
            age: const Duration(minutes: 2),
            onTap: () {},
          ),
        ),
      );
      expect(find.text('STALE 2m ago'), findsOneWidget);
      expect(find.text('LIVE'), findsNothing);
    });

    testWidgets('unreachable node hides numbers', (t) async {
      await t.pumpWidget(
        _host(
          NodeCard(
            status: _status(
              'ghost',
              NodeFreshness.NODE_FRESHNESS_UNREACHABLE,
              metrics: _metrics(),
              error: 'dial refused',
              alerts: [NodeAlert(kind: AlertKind.ALERT_KIND_UNREACHABLE)],
            ),
            age: const Duration(hours: 3),
            onTap: () {},
          ),
        ),
      );
      expect(find.text('UNREACHABLE'), findsOneWidget);
      expect(find.text('DOWN'), findsOneWidget);
      expect(find.byKey(const ValueKey('cpu')), findsNothing);
      expect(find.byKey(const ValueKey('disk')), findsNothing);
      expect(find.textContaining('dial refused'), findsOneWidget);
      expect(find.textContaining('3h ago'), findsOneWidget);
    });

    testWidgets('alert badges for disk and memory', (t) async {
      await t.pumpWidget(
        _host(
          NodeCard(
            status: _status(
              'full',
              NodeFreshness.NODE_FRESHNESS_FRESH,
              metrics: _metrics(disk: 95, mem: 92),
              alerts: [
                NodeAlert(
                  kind: AlertKind.ALERT_KIND_DISK_PERCENT,
                  subject: '/data',
                  value: 95,
                ),
                NodeAlert(kind: AlertKind.ALERT_KIND_MEM_PRESSURE, value: 92),
              ],
            ),
            age: Duration.zero,
            onTap: () {},
          ),
        ),
      );
      expect(find.text('DISK 95% /data'), findsOneWidget);
      expect(find.text('MEM 92%'), findsOneWidget);
    });
  });

  group('nodes tab', () {
    testWidgets('lists worst first and refreshes on node events', (t) async {
      var calls = 0;
      final events = StreamController<Event>();
      final transport = FakeTransportBuilder()
          .unary(NodeService.listNodes, (ListNodesRequest r, ctx) {
            calls++;
            return ListNodesResponse(
              nodes: [
                _status(
                  'alpha',
                  NodeFreshness.NODE_FRESHNESS_FRESH,
                  metrics: _metrics(),
                ),
                _status(
                  'ghost',
                  NodeFreshness.NODE_FRESHNESS_UNREACHABLE,
                  error: 'refused',
                ),
              ],
            );
          })
          .server(
            EventService.subscribe,
            (SubscribeRequest r, ctx) => events.stream,
          )
          .build();
      final client = HubClient.withTransport(transport);
      final feed = EventFeed(() => events.stream)..start();
      addTearDown(() {
        feed.dispose();
        events.close();
      });
      await t.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: NodesTab(hub: _hub, client: client, feed: feed),
          ),
        ),
      );
      await t.pumpAndSettle();
      expect(calls, 1);
      expect(
        t.getTopLeft(find.byKey(const ValueKey('node-ghost'))).dy,
        lessThan(t.getTopLeft(find.byKey(const ValueKey('node-alpha'))).dy),
      );
      events.add(Event(kind: 'node.metrics', subject: 'alpha'));
      await t.pump(const Duration(milliseconds: 50));
      await t.pump(const Duration(milliseconds: 400));
      expect(calls, 2);
      events.add(Event(kind: 'quota.updated', subject: 'x'));
      await t.pump(const Duration(milliseconds: 500));
      expect(calls, 2);
    });
  });

  group('top offenders', () {
    test('ranks containers, engine buckets and watched dirs together', () {
      final m = _metrics()
        ..engine.disk = EngineDiskUsage(
          images: DiskBucket(
            count: Int64(3),
            sizeBytes: Int64(20 << 30),
            reclaimableBytes: Int64(10 << 30),
          ),
          buildCache: DiskBucket(sizeBytes: Int64(80 << 30)),
          containers: DiskBucket(sizeBytes: Int64(55 << 30)),
          volumes: DiskBucket(),
        )
        ..watchDirs.add(
          DirSize(path: '/home/x/.cargo', sizeBytes: Int64(30 << 30)),
        );
      final top = topOffenders(m);
      expect(top.map((o) => o.label), [
        'build cache',
        'b',
        '/home/x/.cargo',
        'images',
        'a',
      ]);
      expect(top.first.kind, OffenderKind.engine);
      expect(top.map((o) => o.label), isNot(contains('containers')));
    });

    test('engine containers bucket counts when no per-container data', () {
      final m = NodeMetrics(
        engine: ContainerEngine(
          disk: EngineDiskUsage(containers: DiskBucket(sizeBytes: Int64(9))),
        ),
      );
      expect(topOffenders(m).single.label, 'containers');
      expect(topOffenders(NodeMetrics()), isEmpty);
    });

    test('limit and ordering', () {
      final m = NodeMetrics();
      for (var i = 1; i <= 12; i++) {
        m.watchDirs.add(DirSize(path: 'd$i', sizeBytes: Int64(i)));
      }
      final top = topOffenders(m, limit: 3);
      expect(top.map((o) => o.label), ['d12', 'd11', 'd10']);
    });
  });

  testWidgets('detail shows sections and a disabled cleanup note', (t) async {
    t.view.physicalSize = const Size(500, 3000);
    t.view.devicePixelRatio = 1;
    addTearDown(t.view.reset);
    final m = _metrics(disk: 93, gpu: true)
      ..watchDirs.add(DirSize(path: '/cache', sizeBytes: Int64(2 << 30)));
    await t.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: NodeDetail(
            status: _status(
              'alpha',
              NodeFreshness.NODE_FRESHNESS_FRESH,
              metrics: m,
            ),
            age: Duration.zero,
            now: DateTime(2026),
          ),
        ),
      ),
    );
    expect(find.byKey(const ValueKey('fs-/data')), findsOneWidget);
    expect(find.byKey(const ValueKey('watch-/cache')), findsOneWidget);
    expect(find.byKey(const ValueKey('offender-0')), findsOneWidget);
    expect(find.byKey(const ValueKey('cleanup-note')), findsOneWidget);
    final rows = ['container-b', 'container-a'];
    expect(
      t.getTopLeft(find.byKey(ValueKey(rows[0]))).dy,
      lessThan(t.getTopLeft(find.byKey(ValueKey(rows[1]))).dy),
    );
  });

  test('sleeve workspaces count as offenders', () {
    final m = NodeMetrics(
      sleeves: [
        Sleeve(
          name: 'alice',
          usage: SleeveUsage(workspaceDiskBytes: Int64(40 << 30)),
        ),
        Sleeve(name: 'idle'),
      ],
      watchDirs: [DirSize(path: '/c', sizeBytes: Int64(1 << 30))],
    );
    final top = topOffenders(m);
    expect(top.first.kind, OffenderKind.sleeve);
    expect(top.first.label, 'alice');
    expect(top.length, 2);
  });

  testWidgets('sleeves screen lists and kills only after confirm', (t) async {
    final killed = <String>[];
    final transport = FakeTransportBuilder()
        .unary(
          SleeveService.listSleeves,
          (ListSleevesRequest r, ctx) => ListSleevesResponse(
            sleeves: [
              Sleeve(
                name: 'alice',
                harness: Harness.HARNESS_CLAUDE,
                workspace: '/w/a',
                session: 'sleeve-alice',
                state: SleeveState.SLEEVE_STATE_RUNNING,
                limits: SleeveLimits(
                  cpuPercent: 200,
                  memoryBytes: Int64(4 << 30),
                ),
                usage: SleeveUsage(
                  cpuPercent: 50,
                  memoryCurrentBytes: Int64(1 << 30),
                  workspaceDiskBytes: Int64(3 << 30),
                ),
              ),
            ],
          ),
        )
        .unary(SleeveService.killSleeve, (KillSleeveRequest r, ctx) {
          killed.add(r.name);
          return KillSleeveResponse();
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
        ),
      ),
    );
    await t.pumpAndSettle();
    expect(find.byKey(const ValueKey('sleeve-alice')), findsOneWidget);
    expect(find.text('1.00 GB / 4.00 GB'), findsOneWidget);
    await t.tap(find.byKey(const ValueKey('kill-alice')));
    await t.pumpAndSettle();
    await t.tap(find.text('CANCEL'));
    await t.pumpAndSettle();
    expect(killed, isEmpty);
    await t.tap(find.byKey(const ValueKey('kill-alice')));
    await t.pumpAndSettle();
    await t.tap(find.byKey(const ValueKey('confirm-kill')));
    await t.pumpAndSettle();
    expect(killed, ['alice']);
  });

  group('accelerators', () {
    final snapdragon = NodeMetrics(
      os: 'windows',
      arch: 'arm64',
      cpu: CpuMetrics(logicalCores: 18, usagePercent: 3),
      memory: MemoryMetrics(totalBytes: Int64(32 << 30), usedPercent: 27),
      gpus: [
        GpuMetrics(
          vendor: 'qualcomm',
          name: 'Qualcomm Adreno X2-90 GPU',
          driver: 'QCDX 32.0.172.1',
          integrated: true,
          utilizationPercent: -1,
          temperatureCelsius: -1,
        ),
      ],
      npus: [
        AcceleratorMetrics(
          kind: 'npu',
          vendor: 'qualcomm',
          name: 'Qualcomm Hexagon NPU',
          driver: 'qcnspmcdm 30.0.228.10000',
          utilizationPercent: 12,
        ),
      ],
    );

    testWidgets('card lists integrated GPU and NPU without fake meters', (
      t,
    ) async {
      await t.pumpWidget(
        _host(
          NodeCard(
            status: _status(
              'snapdragon-laptop',
              NodeFreshness.NODE_FRESHNESS_FRESH,
              metrics: snapdragon,
            ),
            age: Duration.zero,
            onTap: () {},
          ),
        ),
      );
      expect(find.textContaining('Qualcomm Adreno X2-90 GPU'), findsOneWidget);
      expect(find.textContaining('INTEGRATED'), findsOneWidget);
      expect(find.textContaining('QCDX 32.0.172.1'), findsOneWidget);
      expect(find.byKey(const ValueKey('gpu-util-0')), findsNothing);
      expect(find.byKey(const ValueKey('gpu-vram-0')), findsNothing);
      expect(find.textContaining('Qualcomm Hexagon NPU'), findsOneWidget);
      expect(find.byKey(const ValueKey('npu-util-0')), findsOneWidget);
      expect(find.text('12.0%'), findsOneWidget);
    });

    testWidgets('detail shows unknown VRAM use as dashes, not spare', (
      t,
    ) async {
      t.view.physicalSize = const Size(500, 3000);
      t.view.devicePixelRatio = 1;
      addTearDown(t.view.reset);
      final m = NodeMetrics(
        os: 'linux',
        arch: 'amd64',
        cpu: CpuMetrics(logicalCores: 32),
        memory: MemoryMetrics(totalBytes: Int64(64 << 30)),
        gpus: [
          GpuMetrics(
            index: 0,
            vendor: 'intel',
            name: 'Battlemage G31 [Intel Graphics]',
            driver: 'xe',
            utilizationPercent: 0,
            memoryTotalBytes: Int64(32 << 30),
            temperatureCelsius: 55,
          ),
        ],
      );
      await t.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: NodeDetail(
              status: _status(
                'b70-nas-server',
                NodeFreshness.NODE_FRESHNESS_FRESH,
                metrics: m,
              ),
              age: Duration.zero,
              now: DateTime(2026),
            ),
          ),
        ),
      );
      expect(find.textContaining('DISCRETE'), findsOneWidget);
      expect(find.byKey(const ValueKey('gpu-util-0')), findsOneWidget);
      expect(find.text('? / 32.0 GB'), findsOneWidget);
      expect(find.text('NPUS'), findsNothing);
    });

    test('summary uses the first GPU and never invents VRAM use', () {
      final b50 = GpuMetrics(
        name: 'Intel Arc Pro B50 Graphics',
        utilizationPercent: 2,
        memoryUsedBytes: Int64(3 << 30),
        memoryTotalBytes: Int64(16 << 30),
      );
      final igpu = GpuMetrics(
        name: 'Intel Graphics',
        integrated: true,
        utilizationPercent: 40,
      );
      final office = summarizeGpus(NodeMetrics(gpus: [b50, igpu]))!;
      expect(office.utilPercent, 2);
      expect(office.vramPercent, closeTo(18.75, 0.01));
      final soc = summarizeGpus(snapdragon)!;
      expect(soc.utilPercent, isNull);
      expect(soc.vramPercent, isNull);
      final xe = summarizeGpus(
        NodeMetrics(
          gpus: [
            GpuMetrics(
              utilizationPercent: -1,
              memoryTotalBytes: Int64(32 << 30),
            ),
          ],
        ),
      )!;
      expect(xe.vramPercent, isNull);
      expect(summarizeGpus(NodeMetrics()), isNull);
    });
  });
}
