// Renders every main screen to PNG with realistic fake data, phone and
// desktop, both themes. Skipped unless SCREENSHOT_DIR is set.
import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:math' as math;
import 'dart:ui' as ui;

import 'package:connectrpc/connect.dart';
import 'package:connectrpc/test.dart';
import 'package:fixnum/fixnum.dart';
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:protobuf/well_known_types/google/protobuf/duration.pb.dart'
    as pb;
import 'package:protobuf/well_known_types/google/protobuf/timestamp.pb.dart';
import 'package:protectorate/gen/protectorate/v1/backlog.connect.spec.dart';
import 'package:protectorate/gen/protectorate/v1/backlog.pb.dart';
import 'package:protectorate/gen/protectorate/v1/events.connect.spec.dart';
import 'package:protectorate/gen/protectorate/v1/hub.connect.spec.dart';
import 'package:protectorate/gen/protectorate/v1/hub.pb.dart';
import 'package:protectorate/gen/protectorate/v1/node.connect.spec.dart';
import 'package:protectorate/gen/protectorate/v1/node.pb.dart';
import 'package:protectorate/gen/protectorate/v1/quota.connect.spec.dart';
import 'package:protectorate/gen/protectorate/v1/quota.pb.dart';
import 'package:protectorate/gen/protectorate/v1/session.connect.spec.dart';
import 'package:protectorate/gen/protectorate/v1/session.pb.dart';
import 'package:protectorate/screens/history_screen.dart';
import 'package:protectorate/screens/hub_shell.dart';
import 'package:protectorate/screens/hubs_screen.dart';
import 'package:protectorate/screens/node_detail_screen.dart';
import 'package:shared_preferences/shared_preferences.dart';
import 'package:protectorate/gen/protectorate/v1/chat.connect.spec.dart';
import 'package:protectorate/gen/protectorate/v1/chat.pb.dart';
import 'package:protectorate/gen/protectorate/v1/sleeve.connect.spec.dart';
import 'package:protectorate/gen/protectorate/v1/sleeve.pb.dart';
import 'package:protectorate/hubs/event_feed.dart';
import 'package:protectorate/hubs/hub_client.dart';
import 'package:protectorate/hubs/hub_profile.dart';
import 'package:protectorate/gen/protectorate/v1/events.pb.dart';
import 'package:protectorate/nodes/add_node.dart';
import 'package:protectorate/nodes/remove_node.dart';
import 'package:protectorate/nodes/version_skew.dart';
import 'package:protectorate/screens/chat_screen.dart';
import 'package:protectorate/screens/sleeves_screen.dart';
import 'package:protectorate/theme/tokens.dart';

const _hub = HubProfile(id: '1', name: 'home', baseUrl: 'http://hub:7470');
final _dir = Platform.environment['SCREENSHOT_DIR'];
const _fonts =
    '/home/hotschmoe/development/flutter/bin/cache/artifacts/material_fonts';

ChatEvent _ev(int seq, void Function(ChatEvent) f) {
  final e = ChatEvent(seq: Int64(seq));
  f(e);
  return e;
}

ChatEvent _text(int seq, String id, String text, {bool thinking = false}) =>
    _ev(
      seq,
      (e) => e.text = AssistantText(
        messageId: id,
        delta: text,
        thinking: thinking,
      ),
    );

const _diff = '''--- internal/auth/validate.go
+++ internal/auth/validate.go
@@ -12,6 +12,9 @@ func Validate(u, p string) error {
 	if u == "" {
 		return ErrNoUser
 	}
+	if len(p) < 8 {
+		return ErrWeakPassword
+	}
 	return nil
 }''';

Stream<ChatEvent> _transcript() async* {
  yield _ev(
    1,
    (e) => e.state = StateChange(state: ChatState.CHAT_STATE_WORKING),
  );
  yield _ev(
    2,
    (e) => e.user = UserMessage(
      text:
          'The login form lets an empty password through. '
          'Can you fix it and run the tests?',
      from: 'owner',
    ),
  );
  yield _text(
    3,
    'm0',
    'The validator probably only checks the username.',
    thinking: true,
  );
  yield _text(4, 'm1', 'I will start with the validator.');
  yield _ev(
    5,
    (e) => e.toolCall = ToolCall(
      id: 'c1',
      name: 'Read',
      inputJson: '{"file_path":"internal/auth/validate.go"}',
    ),
  );
  yield _ev(
    6,
    (e) => e.toolResult = ToolResult(
      toolCallId: 'c1',
      output:
          'func Validate(u, p string) error {\n\tif u == "" {\n\t\treturn ErrNoUser\n\t}\n\treturn nil\n}',
    ),
  );
  yield _text(
    7,
    'm2',
    'Found it: `Validate` never looks at the password. I will require at '
        'least 8 characters:\n```go\nif len(p) < 8 {\n\treturn ErrWeakPassword\n}\n```',
  );
  yield _ev(
    8,
    (e) => e.toolCall = ToolCall(
      id: 'c2',
      name: 'Edit',
      inputJson:
          '{"file_path":"internal/auth/validate.go","old_string":"return nil","new_string":"if len(p) < 8 {...}"}',
    ),
  );
  yield _ev(
    9,
    (e) => e.toolResult = ToolResult(toolCallId: 'c2', output: _diff),
  );
  yield _ev(
    10,
    (e) => e.compaction = Compaction(
      tokensBefore: Int64(148000),
      tokensAfter: Int64(21000),
      native: true,
    ),
  );
  yield _ev(
    11,
    (e) => e.usage = UsageUpdate(
      contextTokens: Int64(24000),
      contextWindow: Int64(200000),
    ),
  );
  yield _ev(
    12,
    (e) => e.approval = ApprovalRequest(
      id: 'ap1',
      kind: 'exec',
      summary: 'go test ./internal/auth/...',
    ),
  );
  await Completer<void>().future;
}

Future<void> _loadFonts() async {
  Future<ByteData> read(String p) async =>
      ByteData.sublistView(await File(p).readAsBytes());
  final icons = FontLoader('MaterialIcons')
    ..addFont(read('$_fonts/MaterialIcons-Regular.otf'));
  await icons.load();
  final mono = FontLoader(monoFamily)
    ..addFont(read('assets/fonts/JetBrainsMonoNL-Regular.ttf'))
    ..addFont(read('assets/fonts/JetBrainsMonoNL-Medium.ttf'))
    ..addFont(read('assets/fonts/JetBrainsMonoNL-Bold.ttf'))
    ..addFont(read('assets/fonts/JetBrainsMonoNL-Italic.ttf'));
  await mono.load();
  final seg = FontLoader(segmentFamily)
    ..addFont(read('assets/fonts/DSEG7Classic-Regular.ttf'))
    ..addFont(read('assets/fonts/DSEG7Classic-Bold.ttf'));
  await seg.load();
}

Future<void> _shoot(WidgetTester tester, GlobalKey key, String name) async {
  await tester.runAsync(() async {
    final boundary =
        key.currentContext!.findRenderObject()! as RenderRepaintBoundary;
    final image = await boundary.toImage(pixelRatio: 2);
    final data = await image.toByteData(format: ui.ImageByteFormat.png);
    await File('$_dir/$name.png').writeAsBytes(data!.buffer.asUint8List());
  });
}

Sleeve _sleeve(
  String name,
  Harness h,
  String model,
  ChatState st, {
  int ctx = 0,
  int win = 0,
  SleeveMode mode = SleeveMode.SLEEVE_MODE_CHAT,
  double cpu = 30,
  int memMb = 900,
  String provider = '',
  bool local = false,
}) => Sleeve(
  name: name,
  harness: h,
  workspace: '/home/me/src/$name',
  session: 'sleeve-$name',
  state: SleeveState.SLEEVE_STATE_RUNNING,
  mode: mode,
  createdAt: Timestamp.fromDateTime(
    DateTime.now().toUtc().subtract(const Duration(hours: 3, minutes: 12)),
  ),
  limits: SleeveLimits(cpuPercent: 200, memoryBytes: Int64(4 << 30)),
  usage: SleeveUsage(cpuPercent: cpu, memoryCurrentBytes: Int64(memMb << 20)),
  chat: st == ChatState.CHAT_STATE_UNSPECIFIED
      ? null
      : ChatStatus(
          state: st,
          model: model,
          provider: provider,
          localModel: local,
          contextTokens: Int64(ctx),
          contextWindow: Int64(win),
        ),
);

Timestamp _ago(Duration d) =>
    Timestamp.fromDateTime(DateTime.now().toUtc().subtract(d));
Timestamp _in(Duration d) =>
    Timestamp.fromDateTime(DateTime.now().toUtc().add(d));
Int64 _gb(num g) => Int64((g * (1 << 30)).round());
pb.Duration _dur(Duration d) => pb.Duration(seconds: Int64(d.inSeconds));

double _wave(int i, double base, double amp) =>
    (base + amp * math.sin(i / 2.7) + ((i * 37) % 11) - 5).clamp(1.0, 99.0);

NodeStatus _forge(int i) => NodeStatus(
  node: NodeInfo(
    name: 'forge',
    address: '100.64.0.1:7475',
    tier: 'workstation',
    tags: ['gpu', 'docker'],
    local: true,
  ),
  freshness: NodeFreshness.NODE_FRESHNESS_FRESH,
  alerts: [
    NodeAlert(
      kind: AlertKind.ALERT_KIND_DISK_PERCENT,
      subject: '/data',
      value: 82,
    ),
  ],
  metrics: NodeMetrics(
    hostname: 'forge',
    os: 'linux',
    arch: 'amd64',
    platform: 'ubuntu 24.04',
    agentVersion: 'v0.9.2',
    uptimeSeconds: Int64(9 * 86400 + 4 * 3600),
    collectedAt: _ago(Duration(seconds: (200 - i) * 30)),
    cpu: CpuMetrics(
      logicalCores: 16,
      usagePercent: _wave(i, 38, 22),
      load1: 3.21,
      load5: 2.84,
      load15: 2.10,
    ),
    memory: MemoryMetrics(
      totalBytes: _gb(64),
      usedBytes: _gb(37.4),
      usedPercent: 58.4,
      swapTotalBytes: _gb(8),
      swapUsedBytes: _gb(0.4),
      swapUsedPercent: 5,
    ),
    filesystems: [
      FilesystemMetrics(
        mount: '/',
        device: '/dev/nvme0n1p2',
        fstype: 'ext4',
        totalBytes: _gb(930),
        usedBytes: _gb(577),
        usedPercent: 62.0,
        primary: true,
      ),
      FilesystemMetrics(
        mount: '/data',
        device: '/dev/md0',
        fstype: 'xfs',
        totalBytes: _gb(7450),
        usedBytes: _gb(6110),
        usedPercent: 82.0,
      ),
      FilesystemMetrics(
        mount: '/boot/efi',
        device: '/dev/nvme0n1p1',
        fstype: 'vfat',
        totalBytes: _gb(1),
        usedBytes: _gb(0.31),
        usedPercent: 31.0,
      ),
    ],
    gpus: [
      GpuMetrics(
        index: 0,
        vendor: 'nvidia',
        name: 'RTX 4090',
        utilizationPercent: _wave(i + 5, 66, 25),
        memoryUsedBytes: _gb(18.2),
        memoryTotalBytes: _gb(24),
        temperatureCelsius: 66,
      ),
    ],
    engine: ContainerEngine(
      kind: 'docker',
      version: '27.3.1',
      diskMeasuredAt: _ago(const Duration(minutes: 4)),
      disk: EngineDiskUsage(
        images: DiskBucket(
          count: Int64(41),
          sizeBytes: _gb(96),
          reclaimableBytes: _gb(52),
        ),
        buildCache: DiskBucket(
          count: Int64(318),
          sizeBytes: _gb(141),
          reclaimableBytes: _gb(141),
        ),
        volumes: DiskBucket(
          count: Int64(12),
          sizeBytes: _gb(38),
          reclaimableBytes: _gb(4),
        ),
        containers: DiskBucket(count: Int64(5), sizeBytes: _gb(9)),
      ),
      containers: [
        ContainerMetrics(
          name: 'postgres',
          state: 'running',
          cpuPercent: 3.4,
          memoryUsedBytes: _gb(1.2),
          writableLayerBytes: _gb(0.4),
          createdAt: _ago(const Duration(days: 12)),
        ),
        ContainerMetrics(
          name: 'buildkitd',
          state: 'running',
          cpuPercent: 41.0,
          memoryUsedBytes: _gb(3.1),
          writableLayerBytes: _gb(6.8),
          createdAt: _ago(const Duration(days: 3)),
        ),
        ContainerMetrics(
          name: 'redis',
          state: 'running',
          cpuPercent: 0.6,
          memoryUsedBytes: _gb(0.2),
          writableLayerBytes: _gb(0.01),
          createdAt: _ago(const Duration(days: 12)),
        ),
        ContainerMetrics(
          name: 'grafana',
          state: 'exited',
          writableLayerBytes: _gb(1.4),
          createdAt: _ago(const Duration(days: 40)),
        ),
      ],
    ),
    watchDirs: [
      DirSize(
        path: '/home/me/.cache/bazel',
        sizeBytes: _gb(84),
        complete: true,
        measuredAt: _ago(const Duration(minutes: 9)),
      ),
      DirSize(
        path: '/home/me/.cargo',
        sizeBytes: _gb(22),
        measuredAt: _ago(const Duration(minutes: 9)),
      ),
    ],
    sleeves: [
      Sleeve(
        name: 'auth-fix',
        harness: Harness.HARNESS_CLAUDE,
        state: SleeveState.SLEEVE_STATE_RUNNING,
        limits: SleeveLimits(cpuPercent: 200, memoryBytes: _gb(4)),
        usage: SleeveUsage(
          cpuPercent: 62,
          memoryCurrentBytes: _gb(1.66),
          memoryPeakBytes: _gb(2.1),
          pids: 38,
          workspaceDiskBytes: _gb(3.4),
        ),
      ),
      Sleeve(
        name: 'refactor',
        harness: Harness.HARNESS_CODEX,
        state: SleeveState.SLEEVE_STATE_RUNNING,
        limits: SleeveLimits(cpuPercent: 200, memoryBytes: _gb(4)),
        usage: SleeveUsage(
          cpuPercent: 181,
          memoryCurrentBytes: _gb(3.7),
          memoryPeakBytes: _gb(3.9),
          pids: 112,
          oomKills: Int64(1),
          workspaceDiskBytes: _gb(11.2),
        ),
      ),
    ],
  ),
);

List<NodeStatus> _fleet(int i) => [
  _forge(i),
  NodeStatus(
    node: NodeInfo(name: 'arc', address: '100.64.0.7:7475', tier: 'arm'),
    freshness: NodeFreshness.NODE_FRESHNESS_FRESH,
    alerts: [
      NodeAlert(
        kind: AlertKind.ALERT_KIND_DISK_PERCENT,
        subject: '/',
        value: 93,
      ),
    ],
    metrics: NodeMetrics(
      os: 'linux',
      arch: 'arm64',
      platform: 'debian 13',
      uptimeSeconds: Int64(41 * 86400),
      collectedAt: _ago(Duration(seconds: (200 - i) * 30)),
      cpu: CpuMetrics(logicalCores: 8, usagePercent: _wave(i + 9, 14, 8)),
      memory: MemoryMetrics(
        totalBytes: _gb(16),
        usedBytes: _gb(6.6),
        usedPercent: 41.2,
      ),
      filesystems: [
        FilesystemMetrics(
          mount: '/',
          totalBytes: _gb(256),
          usedBytes: _gb(239),
          usedPercent: 93.4,
          primary: true,
        ),
      ],
      engine: ContainerEngine(kind: 'podman', version: '5.2'),
    ),
  ),
  NodeStatus(
    node: NodeInfo(name: 'win-desk', address: '100.64.0.4:7475'),
    freshness: NodeFreshness.NODE_FRESHNESS_STALE,
    age: _dur(const Duration(minutes: 2)),
    metrics: NodeMetrics(
      os: 'windows',
      arch: 'amd64',
      platform: 'windows 11 pro',
      agentVersion: 'v0.9.0',
      uptimeSeconds: Int64(2 * 86400),
      cpu: CpuMetrics(logicalCores: 24, usagePercent: 6.2),
      memory: MemoryMetrics(
        totalBytes: _gb(32),
        usedBytes: _gb(21.1),
        usedPercent: 66.0,
      ),
      filesystems: [
        FilesystemMetrics(
          mount: 'C:',
          totalBytes: _gb(1900),
          usedBytes: _gb(1010),
          usedPercent: 53.1,
          primary: true,
        ),
      ],
      gpus: [
        GpuMetrics(
          name: 'Arc A770',
          vendor: 'intel',
          utilizationPercent: 3,
          memoryUsedBytes: _gb(2),
          memoryTotalBytes: _gb(16),
          temperatureCelsius: -1,
        ),
      ],
    ),
  ),
  NodeStatus(
    node: NodeInfo(
      name: 'pi-lab',
      address: '100.64.0.9:7475',
      os: 'linux',
      arch: 'arm64',
    ),
    freshness: NodeFreshness.NODE_FRESHNESS_UNREACHABLE,
    age: _dur(const Duration(hours: 3)),
    error: 'dial tcp 100.64.0.9:7475: connect: no route to host',
    alerts: [NodeAlert(kind: AlertKind.ALERT_KIND_UNREACHABLE)],
    metrics: NodeMetrics(
      os: 'linux',
      arch: 'arm64',
      cpu: CpuMetrics(logicalCores: 4, usagePercent: 9),
      memory: MemoryMetrics(totalBytes: _gb(8), usedPercent: 30),
    ),
  ),
];

List<ProviderQuota> _quotas() => [
  ProviderQuota(
    provider: 'claude',
    plan: 'max',
    freshness: Freshness.FRESHNESS_FRESH,
    windows: [
      QuotaWindow(
        name: '5h',
        usedPercent: 64,
        resetsAt: _in(const Duration(hours: 2, minutes: 14)),
        estimatedUnusedPercentAtReset: 9,
      ),
      QuotaWindow(
        name: '7d',
        usedPercent: 38,
        resetsAt: _in(const Duration(days: 3, hours: 4)),
        estimatedUnusedPercentAtReset: 22,
      ),
      QuotaWindow(
        name: '7d fable',
        usedPercent: 71,
        windowSeconds: Int64(7 * 24 * 3600),
        resetsAt: _in(const Duration(hours: 18, minutes: 5)),
      ),
    ],
    resets: [
      ResetCredit(
        kind: 'weekly reset',
        count: 1,
        expiresAt: _in(const Duration(days: 9)),
      ),
    ],
  ),
  ProviderQuota(
    provider: 'codex',
    plan: 'pro',
    freshness: Freshness.FRESHNESS_FRESH,
    windows: [
      QuotaWindow(
        name: '5h',
        usedPercent: 91,
        resetsAt: _in(const Duration(minutes: 41)),
      ),
      QuotaWindow(
        name: 'week',
        usedPercent: 47,
        resetsAt: _in(const Duration(days: 4, hours: 19)),
      ),
    ],
  ),
  ProviderQuota(
    provider: 'grok',
    plan: 'supergrok',
    freshness: Freshness.FRESHNESS_STALE,
    age: _dur(const Duration(minutes: 25)),
    windows: [
      QuotaWindow(
        name: '2h',
        usedPercent: 30,
        resetsAt: _in(const Duration(minutes: 77)),
      ),
    ],
  ),
  ProviderQuota(
    provider: 'antigravity',
    freshness: Freshness.FRESHNESS_UNKNOWN,
    error: 'token expired; run protectorate login antigravity',
  ),
];

final _logEvents = [
  ('session.started', 'work', 52),
  ('sleeve.spawned', 'research', 40),
  ('chat.approval', 'auth-fix', 31),
  ('note.created', 'a1', 29),
  ('quota.refreshed', 'claude', 18),
  ('node.unreachable', 'pi-lab', 11),
  ('lease.acquired', 'desk:win-desk', 6),
  ('msg.delivered', 'refactor', 2),
];

// A hub with every service the shell reads; each node poll moves the
// readings so the history graphs fill.
Transport _hubTransport(StreamController<Event> events) {
  var polls = 0;
  return FakeTransportBuilder()
      .server(
        EventService.subscribe,
        (SubscribeRequest r, ctx) => events.stream,
      )
      .unary(
        HubService.getStatus,
        (GetStatusRequest r, ctx) => GetStatusResponse(
          hostname: 'forge',
          os: 'linux',
          arch: 'amd64',
          version: 'v0.9.2',
          uptimeSeconds: Int64(9 * 86400 + 4 * 3600),
        ),
      )
      .unary(
        NodeService.listNodes,
        (ListNodesRequest r, ctx) => ListNodesResponse(nodes: _fleet(polls++)),
      )
      .unary(
        NodeService.getNode,
        (GetNodeRequest r, ctx) => r.name == 'forge'
            ? _forge(polls++)
            : _fleet(0).firstWhere((n) => n.node.name == r.name),
      )
      .unary(
        QuotaService.getQuota,
        (GetQuotaRequest r, ctx) => GetQuotaResponse(quotas: _quotas()),
      )
      .unary(
        SessionService.listSessions,
        (ListSessionsRequest r, ctx) => ListSessionsResponse(
          sessions: [
            Session(
              name: 'work',
              cwd: '/home/me/src/protectorate',
              attachedClients: 1,
            ),
            Session(name: 'logs', cwd: '/var/log'),
            Session(
              name: 'build-arm',
              cwd: '/home/me/src/protectorate',
              attachedClients: 2,
            ),
          ],
        ),
      )
      .unary(
        SessionService.capturePane,
        (CapturePaneRequest r, ctx) => CapturePaneResponse(text: _pane),
      )
      .unary(
        BacklogService.listNotes,
        (ListNotesRequest r, ctx) => ListNotesResponse(notes: _notes()),
      )
      .build();
}

List<Note> _notes() {
  Timestamp ago(Duration d) =>
      Timestamp.fromDateTime(DateTime.now().toUtc().subtract(d));
  return [
    Note(
      id: 'a1',
      title: 'Gap 2: hub as a real service',
      body:
          'Run the hub as a systemd service with its own home.\nTLS via Traefik, owner token never over plain http.',
      project: 'hotschmoe/protectorate',
      source: 'owner',
      pinned: true,
      updatedAt: ago(const Duration(hours: 2)),
    ),
    Note(
      id: 'a2',
      title: 'acme/lib asks: export Foo',
      body:
          'apidev asks: please export Foo from the lib package.\n\nFrom apidev, thread t7.',
      project: 'acme/lib',
      source: 'apidev',
      updatedAt: ago(const Duration(minutes: 14)),
    ),
    Note(
      id: 'a3',
      title: 'acme/lib released v0.3.0: review changes',
      body: 'Upstream release: acme/lib tag v0.3.0\nCommits (2): add Qux',
      project: 'acme/api',
      source: 'hub',
      updatedAt: ago(const Duration(days: 1, hours: 3)),
    ),
    Note(
      id: 'a4',
      body: 'Ideas for the demo: record the pairing flow on the phone.',
      source: 'owner',
      updatedAt: ago(const Duration(days: 4)),
    ),
  ];
}

const _pane =
    '\x1b[38;5;214mme@forge\x1b[0m:\x1b[34m~/src/protectorate\x1b[0m\$ git status -sb\r\n'
    '## pivot-city...origin/pivot-city\r\n'
    ' \x1b[31mM\x1b[0m app/lib/theme/tokens.dart\r\n'
    '\x1b[32m??\x1b[0m app/lib/vfd/\r\n'
    '\x1b[38;5;214mme@forge\x1b[0m:\x1b[34m~/src/protectorate\x1b[0m\$ make test\r\n'
    'go vet ./...\r\n'
    'go test -race ./...\r\n'
    'ok  \tgithub.com/hotschmoe/protectorate/internal/hub\t4.112s\r\n'
    'ok  \tgithub.com/hotschmoe/protectorate/internal/node\t2.380s\r\n'
    '\x1b[31mFAIL\x1b[0m\tgithub.com/hotschmoe/protectorate/internal/auth\t0.041s\r\n'
    '\x1b[33mwarning:\x1b[0m 1 package failed\r\n'
    '\x1b[38;5;214mme@forge\x1b[0m:\x1b[34m~/src/protectorate\x1b[0m\$ ';

Future<void> _pumpShell(
  WidgetTester tester,
  GlobalKey key,
  Brightness b,
  String tab,
  StreamController<Event> events,
) async {
  final transport = _hubTransport(events);
  await tester.pumpWidget(
    RepaintBoundary(
      key: key,
      child: MaterialApp(
        debugShowCheckedModeBanner: false,
        theme: appTheme(b),
        home: HubShell(
          hub: _hub,
          clientFactory: (_) => HubClient.withTransport(transport),
          initialTab: tab,
          appVersion: 'v0.9.2',
        ),
      ),
    ),
  );
  await tester.pump();
  for (final (kind, subject, minutes) in _logEvents) {
    events.add(
      Event(
        kind: kind,
        subject: subject,
        time: _ago(Duration(minutes: minutes)),
      ),
    );
  }
  for (var i = 0; i < 40; i++) {
    events.add(
      Event(kind: 'node.metrics', subject: 'forge', time: _ago(Duration.zero)),
    );
    await tester.pump(const Duration(milliseconds: 350));
  }
  await tester.pump(const Duration(milliseconds: 600));
}

void main() {
  if (_dir == null) {
    test('screenshots skipped without SCREENSHOT_DIR', () {}, skip: true);
    return;
  }
  setUpAll(_loadFonts);

  for (final dark in [false, true]) {
    for (final w in [412, 1280]) {
      final tag = '$w${dark ? 'dark' : ''}';
      final b = dark ? Brightness.dark : Brightness.light;
      testWidgets('chat $tag', (tester) async {
        tester.view.physicalSize = Size(w.toDouble(), w == 412 ? 1280 : 900);
        tester.view.devicePixelRatio = 1;
        addTearDown(tester.view.reset);
        final key = GlobalKey();
        final transport = FakeTransportBuilder()
            .server(ChatService.watch, (WatchRequest r, ctx) => _transcript())
            .build();
        await tester.pumpWidget(
          RepaintBoundary(
            key: key,
            child: MaterialApp(
              debugShowCheckedModeBanner: false,
              theme: appTheme(b),
              home: ChatScreen(
                hub: _hub,
                client: HubClient.withTransport(transport),
                sleeve: _sleeve(
                  'auth-fix',
                  Harness.HARNESS_CLAUDE,
                  'claude-opus-5',
                  ChatState.CHAT_STATE_WORKING,
                ),
              ),
            ),
          ),
        );
        await tester.pump();
        await tester.pump();
        await tester.tap(find.byKey(const ValueKey('tool-c2')));
        await tester.pump(const Duration(milliseconds: 100));
        await _shoot(tester, key, 'chat-$tag');
      });

      testWidgets('sleeves $tag', (tester) async {
        tester.view.physicalSize = Size(w.toDouble(), w == 412 ? 1100 : 800);
        tester.view.devicePixelRatio = 1;
        addTearDown(tester.view.reset);
        final key = GlobalKey();
        final transport = FakeTransportBuilder()
            .server(ChatService.watch, (WatchRequest r, ctx) => _transcript())
            .unary(
              SleeveService.listSleeves,
              (ListSleevesRequest r, ctx) => ListSleevesResponse(
                sleeves: [
                  _sleeve(
                    'auth-fix',
                    Harness.HARNESS_CLAUDE,
                    'claude-opus-5',
                    ChatState.CHAT_STATE_AWAITING_APPROVAL,
                    ctx: 24000,
                    win: 200000,
                    cpu: 62,
                    memMb: 1700,
                  ),
                  _sleeve(
                    'refactor',
                    Harness.HARNESS_CODEX,
                    'gpt-5-codex',
                    ChatState.CHAT_STATE_WORKING,
                    ctx: 91000,
                    win: 272000,
                    cpu: 120,
                    memMb: 2400,
                  ),
                  _sleeve(
                    'scratch',
                    Harness.HARNESS_PI,
                    'qwen3:8b',
                    ChatState.CHAT_STATE_IDLE,
                    ctx: 7000,
                    win: 32000,
                    cpu: 8,
                    memMb: 600,
                    provider: 'vllm',
                    local: true,
                  ),
                  _sleeve(
                    'research',
                    Harness.HARNESS_PI,
                    'deepseek-v4-pro',
                    ChatState.CHAT_STATE_WORKING,
                    ctx: 42000,
                    win: 128000,
                    cpu: 15,
                    memMb: 700,
                    provider: 'deepseek',
                  ),
                  _sleeve(
                    'ops',
                    Harness.HARNESS_SHELL,
                    '',
                    ChatState.CHAT_STATE_UNSPECIFIED,
                    mode: SleeveMode.SLEEVE_MODE_TERMINAL,
                    cpu: 1,
                    memMb: 90,
                  ),
                ],
              ),
            )
            .build();
        final feed = EventFeed(() => const Stream<Event>.empty());
        addTearDown(feed.dispose);
        await tester.pumpWidget(
          RepaintBoundary(
            key: key,
            child: MaterialApp(
              debugShowCheckedModeBanner: false,
              theme: appTheme(b),
              home: SleevesScreen(
                hub: _hub,
                client: HubClient.withTransport(transport),
                feed: feed,
              ),
            ),
          ),
        );
        await tester.pump();
        await tester.pump(const Duration(milliseconds: 100));
        await _shoot(tester, key, 'sleeves-$tag');
        await tester.pumpWidget(const SizedBox());
      });

      final phone = w == 412;
      final size = Size(w.toDouble(), phone ? 1200 : 860);

      for (final tab in ['overview', 'nodes', 'quota', 'sessions', 'backlog']) {
        testWidgets('$tab $tag', (tester) async {
          tester.view.physicalSize = size;
          tester.view.devicePixelRatio = 1;
          addTearDown(tester.view.reset);
          final events = StreamController<Event>.broadcast();
          addTearDown(events.close);
          final key = GlobalKey();
          await _pumpShell(tester, key, b, tab, events);
          await _shoot(tester, key, '$tab-$tag');
          await tester.pumpWidget(const SizedBox());
        });
      }

      testWidgets('notes-editor $tag', (tester) async {
        tester.view.physicalSize = size;
        tester.view.devicePixelRatio = 1;
        addTearDown(tester.view.reset);
        final events = StreamController<Event>.broadcast();
        addTearDown(events.close);
        final key = GlobalKey();
        await _pumpShell(tester, key, b, 'backlog', events);
        await tester.tap(find.byKey(const ValueKey('note-a2')));
        await tester.pump(const Duration(milliseconds: 100));
        await tester.pump(const Duration(milliseconds: 800));
        await _shoot(tester, key, 'notes-editor-$tag');
        await tester.pumpWidget(const SizedBox());
      });

      testWidgets('add-node $tag', (tester) async {
        tester.view.physicalSize = Size(w.toDouble(), phone ? 1500 : 900);
        tester.view.devicePixelRatio = 1;
        addTearDown(tester.view.reset);
        final events = StreamController<Event>.broadcast();
        addTearDown(events.close);
        final key = GlobalKey();
        await _pumpShell(tester, key, b, 'nodes', events);
        final hub = phone ? 'http://192.168.1.20:7470' : 'http://hub.lan:7470';
        const key0 = 'hskey-auth-7f3a9c2e41b8-5d0e6a';
        final client = HubClient.withTransport(
          FakeTransportBuilder()
              .unary(
                NodeService.inviteNode,
                (InviteNodeRequest r, ctx) async => InviteNodeResponse(
                  node: NodeInfo(name: r.name, invited: true),
                  key: key0,
                  installUrl: hub,
                  expiresAt: Timestamp.fromDateTime(
                    DateTime.now().add(const Duration(hours: 1)),
                  ),
                  commands: [
                    InstallCommand(
                      platform: InstallPlatform.INSTALL_PLATFORM_WINDOWS,
                      shell: 'PowerShell',
                      command:
                          "\$env:PROTECTORATE_ENROLL_KEY='$key0'; \$env:PROTECTORATE_NODE_NAME='${r.name}'; irm '$hub/install.ps1' | iex",
                      note:
                          'Paste into a normal PowerShell (not Run as administrator) as the user who stays logged in. It installs a logon task for that user, so the agent runs in their desktop session without administrator rights. x64 and ARM64.',
                    ),
                    InstallCommand(
                      platform: InstallPlatform.INSTALL_PLATFORM_TERMUX,
                      shell: 'Termux',
                      command:
                          "curl -fsSL '$hub/install.sh' | PROTECTORATE_ENROLL_KEY='$key0' sh -s -- --name '${r.name}'",
                      note:
                          'Paste into Termux. For start at boot, first pkg install termux-services (then restart Termux) and install the Termux:Boot app.',
                    ),
                  ],
                ),
              )
              .server(
                NodeService.watchNodeSetup,
                (WatchNodeSetupRequest r, ctx) => Stream.value(
                  NodeSetupProgress(
                    stage: NodeSetupStage.NODE_SETUP_STAGE_JOINED,
                    message:
                        'enrolled in the tailnet; waiting for the hub to reach the agent',
                    status: NodeStatus(
                      node: NodeInfo(name: r.name, invited: true),
                    ),
                  ),
                ),
              )
              .build(),
        );
        unawaited(
          showAddNode(
            tester.element(find.byType(Scaffold).first),
            client: client,
            initialName: phone ? 'pixel' : 'office-arm',
            platform: phone ? TargetPlatform.android : TargetPlatform.windows,
          ),
        );
        await tester.pump();
        await tester.pump(const Duration(milliseconds: 300));
        await _shoot(tester, key, 'add-node-form-$tag');
        await tester.tap(find.byKey(const ValueKey('add-create')));
        await tester.pump();
        await tester.pump(const Duration(milliseconds: 300));
        if (phone) {
          await tester.tap(find.byKey(const ValueKey('choice-TERMUX')));
        } else {
          await tester.tap(find.byKey(const ValueKey('choice-WINDOWS')));
        }
        await tester.pump(const Duration(milliseconds: 300));
        await _shoot(tester, key, 'add-node-$tag');
        await tester.pumpWidget(const SizedBox());
      });

      testWidgets('remove-node $tag', (tester) async {
        tester.view.physicalSize = Size(w.toDouble(), phone ? 1100 : 900);
        tester.view.devicePixelRatio = 1;
        addTearDown(tester.view.reset);
        final events = StreamController<Event>.broadcast();
        addTearDown(events.close);
        final key = GlobalKey();
        await _pumpShell(tester, key, b, 'nodes', events);
        final progress = StreamController<RemoveNodeProgress>();
        addTearDown(progress.close);
        final client = HubClient.withTransport(
          FakeTransportBuilder()
              .server(
                NodeService.removeNode,
                (RemoveNodeRequest r, ctx) => progress.stream,
              )
              .build(),
        );
        unawaited(
          showRemoveNode(
            tester.element(find.byType(Scaffold).first),
            client: client,
            status: NodeStatus(
              node: NodeInfo(name: 'win-desk'),
              freshness: NodeFreshness.NODE_FRESHNESS_FRESH,
            ),
          ),
        );
        await tester.pump();
        await tester.pump(const Duration(milliseconds: 300));
        await tester.enterText(
          find.byKey(const ValueKey('remove-confirm')),
          'win-desk',
        );
        for (var i = 0; i < 5; i++) {
          await tester.pump(const Duration(milliseconds: 100));
        }
        await _shoot(tester, key, 'remove-node-$tag');
        await tester.tap(find.byKey(const ValueKey('remove-go')));
        await tester.pump();
        progress.add(
          RemoveNodeProgress(
            phase: RemovePhase.REMOVE_PHASE_DRAINING,
            runningJobs: 1,
            message: 'waiting for 1 job(s) to finish',
          ),
        );
        progress.add(
          RemoveNodeProgress(
            phase: RemovePhase.REMOVE_PHASE_UNINSTALLING,
            message:
                'the agent unregisters its startup and stops: unregister the logon task; stop the agent. Nothing is deleted.',
          ),
        );
        await tester.pump(const Duration(milliseconds: 300));
        await _shoot(tester, key, 'remove-node-progress-$tag');
        await tester.pumpWidget(const SizedBox());
      });

      for (final node in ['forge', 'win-desk']) {
        testWidgets('node $node $tag', (tester) async {
          tester.view.physicalSize = Size(w.toDouble(), phone ? 2400 : 1400);
          tester.view.devicePixelRatio = 1;
          addTearDown(tester.view.reset);
          final events = StreamController<Event>.broadcast();
          addTearDown(events.close);
          final feed = EventFeed(() => events.stream)..start();
          addTearDown(feed.dispose);
          final key = GlobalKey();
          await tester.pumpWidget(
            RepaintBoundary(
              key: key,
              child: MaterialApp(
                debugShowCheckedModeBanner: false,
                theme: appTheme(b),
                home: NodeDetailScreen(
                  hub: _hub,
                  client: HubClient.withTransport(_hubTransport(events)),
                  feed: feed,
                  name: node,
                  versions: const Versions(hub: 'v0.9.2', app: 'v0.9.2'),
                ),
              ),
            ),
          );
          await tester.pump();
          for (var i = 0; i < 40; i++) {
            events.add(Event(kind: 'node.metrics', subject: node));
            await tester.pump(const Duration(milliseconds: 50));
          }
          await tester.pump(const Duration(milliseconds: 600));
          await _shoot(tester, key, 'node-$node-$tag');
          await tester.pumpWidget(const SizedBox());
        });
      }

      testWidgets('hubs $tag', (tester) async {
        tester.view.physicalSize = Size(w.toDouble(), phone ? 900 : 700);
        tester.view.devicePixelRatio = 1;
        addTearDown(tester.view.reset);
        SharedPreferences.setMockInitialValues({
          'hubs': jsonEncode([
            const HubProfile(
              id: '1',
              name: 'home',
              baseUrl: 'https://forge.tail0b1c.ts.net:7470',
              token: 't',
            ).toJson(),
            const HubProfile(
              id: '2',
              name: 'office',
              baseUrl: 'http://10.20.0.2:7470',
            ).toJson(),
          ]),
        });
        final store = HubStore(await SharedPreferences.getInstance());
        final key = GlobalKey();
        await tester.pumpWidget(
          RepaintBoundary(
            key: key,
            child: MaterialApp(
              debugShowCheckedModeBanner: false,
              theme: appTheme(b),
              home: HubsScreen(
                store: store,
                clientFactory: (_) => throw UnimplementedError(),
              ),
            ),
          ),
        );
        await tester.pump(const Duration(milliseconds: 100));
        await _shoot(tester, key, 'hubs-$tag');
      });

      testWidgets('terminal $tag', (tester) async {
        tester.view.physicalSize = Size(w.toDouble(), phone ? 800 : 600);
        tester.view.devicePixelRatio = 1;
        addTearDown(tester.view.reset);
        final events = StreamController<Event>.broadcast();
        addTearDown(events.close);
        final key = GlobalKey();
        await tester.pumpWidget(
          RepaintBoundary(
            key: key,
            child: MaterialApp(
              debugShowCheckedModeBanner: false,
              theme: appTheme(b),
              home: HistoryScreen(
                client: HubClient.withTransport(_hubTransport(events)),
                session: 'work',
              ),
            ),
          ),
        );
        await tester.pump();
        await tester.pump(const Duration(milliseconds: 100));
        await _shoot(tester, key, 'terminal-$tag');
      });
    }
  }
}
