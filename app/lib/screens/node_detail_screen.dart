import 'dart:async';

import 'package:connectrpc/connect.dart';
import 'package:flutter/material.dart';

import '../gen/protectorate/v1/lease.pb.dart';
import '../gen/protectorate/v1/node.pb.dart';
import '../gen/protectorate/v1/release.pb.dart';
import '../hubs/event_feed.dart';
import '../hubs/hub_client.dart';
import '../hubs/hub_profile.dart';
import '../nodes/add_node.dart';
import '../nodes/desk_view.dart';
import '../nodes/node_update.dart';
import '../nodes/remove_node.dart';
import '../nodes/node_view.dart';
import '../nodes/version_skew.dart';
import '../theme/tokens.dart';
import '../vfd/lamp.dart';
import '../vfd/meter.dart';
import '../vfd/panel.dart';
import 'desk_screen.dart';
import 'nodes_tab.dart';
import 'sleeves_screen.dart';

class NodeDetailScreen extends StatefulWidget {
  const NodeDetailScreen({
    super.key,
    required this.hub,
    required this.client,
    required this.feed,
    required this.name,
    this.now,
    this.history,
    this.versions = const Versions(),
  });

  final Versions versions;
  final HubProfile hub;
  final HubClient client;
  final EventFeed feed;
  final String name;
  final DateTime Function()? now;
  final NodeHistory? history;

  @override
  State<NodeDetailScreen> createState() => _NodeDetailScreenState();
}

class _NodeDetailScreenState extends State<NodeDetailScreen> {
  final _permissionAbort = CancelableSignal();
  bool _canUpdate = false;
  NodeStatus? _status;
  List<Lease> _leases = const [];
  DateTime _receivedAt = DateTime.now();
  Object? _error;
  StreamSubscription<dynamic>? _sub;
  Timer? _poll;
  Timer? _tick;

  late final NodeHistory _history = widget.history ?? NodeHistory();

  DateTime _now() => (widget.now ?? DateTime.now)();

  @override
  void initState() {
    super.initState();
    _sub = widget.feed.incoming.listen((e) {
      if (nodeEventKinds.contains(e.kind) && e.subject == widget.name) {
        _refresh();
      } else if (e.kind.startsWith('lease.')) {
        _loadLeases();
      }
    });
    _poll = Timer.periodic(const Duration(seconds: 30), (_) => _refresh());
    _tick = Timer.periodic(const Duration(seconds: 5), (_) {
      if (mounted) setState(() {});
    });
    _refresh();
    _loadUpdatePermission();
  }

  @override
  void dispose() {
    _permissionAbort.cancel();
    _sub?.cancel();
    _poll?.cancel();
    _tick?.cancel();
    super.dispose();
  }

  Future<void> _loadUpdatePermission() async {
    try {
      final release = await widget.client.releases.checkRelease(
        CheckReleaseRequest(),
        signal: TimeoutSignal(
          const Duration(seconds: 15),
          parent: _permissionAbort,
        ),
      );
      if (mounted) setState(() => _canUpdate = release.canUpdate);
    } on Object {
      return;
    }
  }

  Future<void> _loadLeases() async {
    try {
      final r = await widget.client.leases.listLeases(ListLeasesRequest());
      if (mounted) setState(() => _leases = r.leases);
    } on Object {
      return;
    }
  }

  Future<void> _refresh() async {
    unawaited(_loadLeases());
    try {
      final s = await widget.client.node.getNode(
        GetNodeRequest(name: widget.name),
      );
      _history.record(s);
      if (mounted) {
        setState(() {
          _status = s;
          _receivedAt = _now();
          _error = null;
        });
      }
    } on Object catch (e) {
      if (mounted) setState(() => _error = e);
    }
  }

  Future<void> _remove(NodeStatus s) async {
    final removed = await showRemoveNode(
      context,
      client: widget.client,
      status: s,
    );
    if (removed && mounted) Navigator.of(context).pop();
  }

  @override
  Widget build(BuildContext context) {
    final s = _status;
    return Scaffold(
      appBar: AppBar(
        title: Text(widget.name),
        actions: [
          IconButton(
            tooltip: 'Refresh',
            icon: const Icon(Icons.refresh),
            onPressed: _refresh,
          ),
        ],
      ),
      body: s == null
          ? (_error != null
                ? Padding(
                    padding: const EdgeInsets.all(Tokens.gap),
                    child: NoteStrip('$_error', tone: Tone.alarm),
                  )
                : const BusyBar())
          : NodeDetail(
              status: s,
              age: statusAge(s, _receivedAt, _now()),
              now: _now(),
              error: _error,
              cpuHistory: _history.cpu(widget.name),
              memHistory: _history.mem(widget.name),
              versions: widget.versions,
              gpuHolders: gpuHolders(_leases, widget.name),
              onUpdate: s.node.local || !_canUpdate
                  ? null
                  : () => showNodeUpdate(
                      context,
                      client: widget.client,
                      node: s.node.name,
                      nodeVersion: nodeVersion(s),
                      hubVersion: widget.versions.hub,
                      onFinished: _refresh,
                    ),
              onRemove: s.node.local ? null : () => _remove(s),
              onReissue: s.node.invited
                  ? () => showAddNode(
                      context,
                      client: widget.client,
                      initialName: s.node.name,
                      reissue: true,
                      onChanged: _refresh,
                    )
                  : null,
              onOpenDesk: () => Navigator.of(context).push(
                MaterialPageRoute<void>(
                  builder: (_) => DeskScreen(
                    client: widget.client,
                    feed: widget.feed,
                    name: widget.name,
                    now: widget.now,
                  ),
                ),
              ),
              onManageSleeves: s.node.local
                  ? () => Navigator.of(context).push(
                      MaterialPageRoute<void>(
                        builder: (_) => SleevesScreen(
                          hub: widget.hub,
                          client: widget.client,
                          feed: widget.feed,
                        ),
                      ),
                    )
                  : null,
            ),
    );
  }
}

class _VersionPanel extends StatelessWidget {
  const _VersionPanel({
    required this.nodeVersion,
    required this.versions,
    required this.skewed,
    required this.onUpdate,
  });

  final String nodeVersion;
  final Versions versions;
  final bool skewed;
  final VoidCallback? onUpdate;

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    Widget row(String legend, String v) => Row(
      children: [
        SizedBox(width: 56, child: Text(legend, style: t.legend())),
        Text(v.isEmpty ? '-' : v, style: t.data(t.p.emph)),
      ],
    );
    return Panel(
      legend: 'VERSION',
      trailing: skewed
          ? const LampLegend(label: 'SKEW', tone: Tone.warn)
          : nodeVersion.isNotEmpty && versions.hub.isNotEmpty
          ? const LampLegend(label: 'MATCH', tone: Tone.ok)
          : null,
      child: Row(
        children: [
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                row('NODE', nodeVersion),
                const SizedBox(height: 4),
                row('HUB', versions.hub),
              ],
            ),
          ),
          if (onUpdate != null)
            OutlinedButton(
              key: const ValueKey('update-node'),
              onPressed: onUpdate,
              child: const Text('UPDATE NODE'),
            ),
        ],
      ),
    );
  }
}

// Inventory actions: reissue or cancel a pending invite, remove a node.
class _ManagePanel extends StatelessWidget {
  const _ManagePanel({required this.status, this.onRemove, this.onReissue});

  final NodeStatus status;
  final VoidCallback? onRemove;
  final VoidCallback? onReissue;

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    final pending = status.node.invited;
    return Panel(
      legend: pending ? 'PENDING INVITE' : 'INVENTORY',
      trailing: status.draining
          ? const LampLegend(label: 'REMOVING', tone: Tone.warn)
          : null,
      child: Wrap(
        spacing: Tokens.gapS,
        runSpacing: Tokens.gapS,
        crossAxisAlignment: WrapCrossAlignment.center,
        children: [
          if (pending)
            Text(
              'The machine has not enrolled yet. A new key replaces the old one.',
              style: t.small(),
            ),
          if (onReissue != null)
            OutlinedButton(
              key: const ValueKey('reissue-key'),
              onPressed: onReissue,
              child: const Text('REISSUE KEY'),
            ),
          if (onRemove != null)
            OutlinedButton(
              key: const ValueKey('remove-node'),
              style: OutlinedButton.styleFrom(
                foregroundColor: t.p.alarm,
                side: BorderSide(color: t.p.alarm),
              ),
              onPressed: onRemove,
              child: Text(pending ? 'CANCEL INVITE' : 'REMOVE NODE'),
            ),
        ],
      ),
    );
  }
}

class NodeDetail extends StatelessWidget {
  const NodeDetail({
    super.key,
    required this.status,
    required this.age,
    required this.now,
    this.error,
    this.onManageSleeves,
    this.onRemove,
    this.onReissue,
    this.onUpdate,
    this.onOpenDesk,
    this.gpuHolders = const {},
    this.versions = const Versions(),
    this.cpuHistory = const [],
    this.memHistory = const [],
  });

  final VoidCallback? onManageSleeves;
  final VoidCallback? onRemove;
  final VoidCallback? onReissue;
  final VoidCallback? onUpdate;
  final VoidCallback? onOpenDesk;
  final Map<int, String> gpuHolders;
  final Versions versions;
  final NodeStatus status;
  final Duration age;
  final DateTime now;
  final Object? error;
  final List<double?> cpuHistory;
  final List<double?> memHistory;

  static const _wide = 1000.0;

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    final f = status.freshness;
    final down = f == NodeFreshness.NODE_FRESHNESS_UNREACHABLE;
    final stale = f == NodeFreshness.NODE_FRESHNESS_STALE;
    final m = status.hasMetrics() ? status.metrics : null;
    final dim = stale || down;
    final badges = alertBadges(status);

    final head = <Widget>[
      Wrap(
        spacing: 12,
        runSpacing: 8,
        crossAxisAlignment: WrapCrossAlignment.center,
        children: [
          FreshnessLamp(
            key: const ValueKey('freshness'),
            freshness: f,
            age: age,
          ),
          if (badges.isNotEmpty) AlertStrip(badges: badges),
          if (isCareNode(status))
            const Annunciator(
              key: ValueKey('tier-care'),
              label: 'CARE',
              tone: Tone.info,
            ),
          Text(
            [
              status.node.address,
              if (status.node.tier.isNotEmpty) status.node.tier,
              ...status.node.tags,
            ].join('  '),
            style: t.small(),
          ),
        ],
      ),
      _VersionPanel(
        nodeVersion: nodeVersion(status),
        versions: versions,
        skewed: nodeSkewed(status, versions.hub),
        onUpdate: down ? null : onUpdate,
      ),
      if (error != null)
        NoteStrip(
          'Refresh failed, showing the last result: $error',
          tone: Tone.alarm,
        ),
      if (status.error.isNotEmpty)
        NoteStrip(
          status.error,
          tone: status.node.invited && status.alerts.isEmpty
              ? Tone.info
              : Tone.alarm,
        ),
      if (onRemove != null || onReissue != null)
        _ManagePanel(status: status, onRemove: onRemove, onReissue: onReissue),
      if (isWindowsNode(status) && !status.node.invited)
        DeskPanel(status: status, now: now, onOpen: down ? null : onOpenDesk),
      if (down)
        NoteStrip(
          m == null
              ? 'This node has never been reached. No metrics.'
              : 'Unreachable. The figures below are from ${formatAge(age)} and are not current.',
          tone: Tone.alarm,
        ),
      if (stale)
        NoteStrip(
          'Last poll failed. Figures are from ${formatAge(age)} and may be outdated.',
        ),
    ];

    final left = <Widget>[];
    final right = <Widget>[];
    if (m != null) {
      left.addAll([
        _HostPanel(
          m: m,
          busy: gpuBusy(status),
          dim: dim,
          cpuHistory: cpuHistory,
          memHistory: memHistory,
        ),
        if (m.hasJobs())
          Panel(
            key: const ValueKey('jobs-isolation'),
            legend: 'JOBS',
            tone: jobsUnisolated(m.jobs) ? Tone.warn : null,
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                for (final l in jobIsolationLines(m.jobs))
                  Text(l, style: t.data()),
              ],
            ),
          ),
        if (m.warnings.isNotEmpty)
          Panel(
            legend: 'COLLECTION WARNINGS',
            tone: Tone.warn,
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                for (final w in m.warnings) Text(w, style: t.data(t.p.warn)),
              ],
            ),
          ),
        Panel(
          legend: 'FILESYSTEMS',
          trailing: Text('${m.filesystems.length}', style: t.legend()),
          child: m.filesystems.isEmpty
              ? Text('None reported.', style: t.small())
              : Column(
                  children: [
                    for (final fs in m.filesystems)
                      Padding(
                        padding: const EdgeInsets.symmetric(vertical: 4),
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            Text.rich(
                              TextSpan(
                                children: [
                                  TextSpan(
                                    text: fs.mount,
                                    style: t.data(t.p.emph),
                                  ),
                                  TextSpan(
                                    text: '  ${fs.device} ${fs.fstype}',
                                    style: t.small(),
                                  ),
                                ],
                              ),
                              overflow: TextOverflow.ellipsis,
                            ),
                            MeterRow(
                              key: ValueKey('fs-${fs.mount}'),
                              label: '',
                              labelChars: 0,
                              valueChars: 24,
                              percent: fs.usedPercent,
                              value:
                                  '${formatPercent(fs.usedPercent)}  ${formatBytes(fs.usedBytes.toInt())} / ${formatBytes(fs.totalBytes.toInt())}',
                              dim: dim,
                            ),
                          ],
                        ),
                      ),
                  ],
                ),
        ),
        Panel(
          legend: 'GPUS',
          trailing: Text('${m.gpus.length}', style: t.legend()),
          child: m.gpus.isEmpty
              ? Text('None.', style: t.small())
              : Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    for (final g in m.gpus)
                      GpuLines(
                        key: ValueKey('gpu-${g.index}'),
                        gpu: g,
                        dim: dim,
                        detail: true,
                        holder: gpuHolders[g.index] ?? '',
                        busy: gpuBusy(status)[g.index] ?? '',
                      ),
                  ],
                ),
        ),
        if (m.npus.isNotEmpty)
          Panel(
            legend: 'NPUS',
            trailing: Text('${m.npus.length}', style: t.legend()),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                for (final (i, n) in m.npus.indexed)
                  NpuLines(
                    key: ValueKey('npu-$i'),
                    npu: n,
                    index: i,
                    dim: dim,
                    detail: true,
                  ),
              ],
            ),
          ),
        Panel(
          legend: 'TOP DISK OFFENDERS',
          child: _Offenders(offenders: topOffenders(m)),
        ),
      ]);
      right.addAll([
        Panel(
          legend: m.hasEngine()
              ? 'CONTAINER ENGINE  ${m.engine.kind} ${m.engine.version}'
                    .toUpperCase()
              : 'CONTAINER ENGINE',
          child: m.hasEngine()
              ? _EngineDisk(engine: m.engine, now: now)
              : Text('No container engine reachable.', style: t.small()),
        ),
        if (m.hasEngine())
          Panel(
            legend: 'CONTAINERS BY WRITABLE LAYER',
            trailing: Text('${m.engine.containers.length}', style: t.legend()),
            child: _ContainerTable(containers: containersBySize(m)),
          ),
        if (m.sleeves.isNotEmpty || onManageSleeves != null)
          Panel(
            legend: 'SLEEVES BY WORKSPACE SIZE',
            trailing: Text('${m.sleeves.length}', style: t.legend()),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                if (m.sleeves.isEmpty) Text('No sleeves.', style: t.small()),
                for (final s in sleevesBySize(m))
                  SleeveRow(sleeve: s, dim: dim),
                if (onManageSleeves != null)
                  Align(
                    alignment: Alignment.centerRight,
                    child: OutlinedButton(
                      key: const ValueKey('manage-sleeves'),
                      onPressed: onManageSleeves,
                      child: const Text('MANAGE SLEEVES'),
                    ),
                  ),
              ],
            ),
          ),
        Panel(
          legend: 'WATCHED DIRECTORIES',
          child: m.watchDirs.isEmpty
              ? Text('None configured.', style: t.small())
              : Column(
                  children: [
                    for (var i = 0; i < m.watchDirs.length; i++)
                      _WatchRow(index: i, dir: m.watchDirs[i], now: now),
                  ],
                ),
        ),
        Padding(
          padding: const EdgeInsets.symmetric(horizontal: 4),
          child: Text(
            'Cleanup actions are not available yet. They will require explicit confirmation.',
            key: const ValueKey('cleanup-note'),
            style: t.small(),
          ),
        ),
      ]);
    }

    Widget gapped(List<Widget> ws) => Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        for (final w in ws)
          Padding(
            padding: const EdgeInsets.only(bottom: Tokens.gap),
            child: w,
          ),
      ],
    );

    return LayoutBuilder(
      builder: (context, box) {
        final wide = box.maxWidth >= _wide;
        if (!wide) {
          return ListView(
            padding: const EdgeInsets.all(Tokens.gap),
            children: [
              for (final w in [...head, ...left, ...right])
                Padding(
                  padding: const EdgeInsets.only(bottom: Tokens.gap),
                  child: w,
                ),
            ],
          );
        }
        return SingleChildScrollView(
          padding: const EdgeInsets.all(Tokens.gapL),
          child: Center(
            child: ConstrainedBox(
              constraints: const BoxConstraints(maxWidth: Tokens.boardMaxWidth),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  gapped(head),
                  Row(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Expanded(child: gapped(left)),
                      const SizedBox(width: Tokens.gapL),
                      Expanded(child: gapped(right)),
                    ],
                  ),
                ],
              ),
            ),
          ),
        );
      },
    );
  }
}

class _HostPanel extends StatelessWidget {
  const _HostPanel({
    required this.m,
    required this.busy,
    required this.dim,
    required this.cpuHistory,
    required this.memHistory,
  });

  final NodeMetrics m;
  final Map<int, String> busy;
  final bool dim;
  final List<double?> cpuHistory;
  final List<double?> memHistory;

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    final gpu = summarizeGpus(m);
    final legendW = 4 * (11 * Tokens.monoAdvance + 1.4) + 8;
    Widget hist(String label, List<double?> samples) => Padding(
      padding: const EdgeInsets.only(top: 6),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.end,
        children: [
          SizedBox(
            width: legendW,
            child: Text(label, style: t.legend()),
          ),
          Expanded(
            child: DotMatrixGraph(samples: samples, dim: dim),
          ),
        ],
      ),
    );
    return Panel(
      legend: 'HOST',
      trailing: Flexible(
        child: Text(
          [
            '${m.os}/${m.arch}',
            m.platform,
            'agent ${m.agentVersion}',
            'up ${formatUptime(m.uptimeSeconds.toInt())}',
          ].where((e) => e.isNotEmpty).join('  '),
          style: t.small(t.p.legend),
          overflow: TextOverflow.ellipsis,
        ),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          LayoutBuilder(
            builder: (context, box) {
              final n = gpu?.utilPercent != null ? 3 : 2;
              final size = ((box.maxWidth - (n - 1) * 12) / n).clamp(
                96.0,
                150.0,
              );
              return Wrap(
                alignment: WrapAlignment.spaceEvenly,
                spacing: 12,
                children: [
                  ArcGauge(
                    percent: m.cpu.usagePercent,
                    value: m.cpu.usagePercent.round().toString(),
                    unit: '%',
                    caption: 'CPU ${m.cpu.logicalCores}C',
                    size: size,
                    dim: dim,
                  ),
                  ArcGauge(
                    percent: m.memory.usedPercent,
                    value: m.memory.usedPercent.round().toString(),
                    unit: '%',
                    caption: 'MEM',
                    size: size,
                    dim: dim,
                  ),
                  if (gpu?.utilPercent != null)
                    ArcGauge(
                      percent: gpu!.utilPercent,
                      value: gpu.utilPercent!.round().toString(),
                      unit: '%',
                      caption: gpu.count > 1 ? 'GPU0 OF ${gpu.count}' : 'GPU',
                      size: size,
                      dim: dim,
                    ),
                ],
              );
            },
          ),
          if (m.cpu.hasLoad1())
            Padding(
              padding: const EdgeInsets.only(top: 8, bottom: 6),
              child: Row(
                children: [
                  SizedBox(
                    width: legendW,
                    child: Text('LOAD', style: t.legend()),
                  ),
                  SegmentReadout.parts(
                    size: Tokens.readoutM,
                    color: t.p.lit,
                    parts: [
                      (m.cpu.load1.toStringAsFixed(2), '1M'),
                      (m.cpu.load5.toStringAsFixed(2), '5M'),
                      (m.cpu.load15.toStringAsFixed(2), '15M'),
                    ],
                  ),
                ],
              ),
            ),
          ...nodeMeters(m, dim: dim, detail: true, gpuBusy: busy),
          if (cpuHistory.length > 1) hist('CPU', cpuHistory),
          if (memHistory.length > 1) hist('MEM', memHistory),
        ],
      ),
    );
  }
}

class _EngineDisk extends StatelessWidget {
  const _EngineDisk({required this.engine, required this.now});

  final ContainerEngine engine;
  final DateTime now;

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    if (!engine.hasDisk()) {
      return Text('Disk usage not measured yet.', style: t.small());
    }
    final d = engine.disk;
    final rows = [
      ('Images', d.images),
      ('Build cache', d.buildCache),
      ('Volumes', d.volumes),
      ('Containers', d.containers),
    ];
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        for (var i = 0; i < rows.length; i++)
          BarRow(
            index: i,
            child: Row(
              key: ValueKey('engine-${rows[i].$1}'),
              children: [
                Expanded(
                  flex: 3,
                  child: Text(rows[i].$1, style: t.data(t.p.emph)),
                ),
                Expanded(
                  flex: 2,
                  child: Text(
                    '${rows[i].$2.count} items',
                    style: t.small(),
                    textAlign: TextAlign.end,
                  ),
                ),
                Expanded(
                  flex: 3,
                  child: Text(
                    formatBytes(rows[i].$2.sizeBytes.toInt()),
                    style: t.data(),
                    textAlign: TextAlign.end,
                  ),
                ),
                Expanded(
                  flex: 4,
                  child: Text(
                    '${formatBytes(rows[i].$2.reclaimableBytes.toInt())} reclaimable',
                    style: t.small(),
                    textAlign: TextAlign.end,
                  ),
                ),
              ],
            ),
          ),
        if (engine.hasDiskMeasuredAt())
          Padding(
            padding: const EdgeInsets.only(top: 6, left: 8),
            child: Text(
              'measured ${formatAge(now.difference(engine.diskMeasuredAt.toDateTime()))}',
              style: t.small(),
            ),
          ),
      ],
    );
  }
}

bool _isRunning(ContainerMetrics c) => c.state.isEmpty || c.state == 'running';

class _ContainerTable extends StatelessWidget {
  const _ContainerTable({required this.containers});

  final List<ContainerMetrics> containers;

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    if (containers.isEmpty) return Text('No containers.', style: t.small());
    final now = DateTime.now();
    Widget cell(String s, int flex, {TextStyle? style, bool end = true}) =>
        Expanded(
          flex: flex,
          child: Text(
            s,
            style: style ?? t.data(),
            textAlign: end ? TextAlign.end : TextAlign.start,
            overflow: TextOverflow.ellipsis,
          ),
        );
    final head = t.legend();
    return Column(
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(8, 0, 8, 6),
          child: Row(
            children: [
              cell('NAME', 5, style: head, end: false),
              cell('CPU', 2, style: head),
              cell('MEM', 3, style: head),
              cell('WRITABLE', 3, style: head),
              cell('AGE', 2, style: head),
            ],
          ),
        ),
        for (var i = 0; i < containers.length; i++)
          BarRow(
            index: i,
            child: Row(
              key: ValueKey('container-${containers[i].name}'),
              children: [
                cell(
                  _isRunning(containers[i])
                      ? containers[i].name
                      : '${containers[i].name} (${containers[i].state})',
                  5,
                  end: false,
                  style: t.data(_isRunning(containers[i]) ? t.p.emph : t.p.dim),
                ),
                cell(
                  !_isRunning(containers[i])
                      ? '-'
                      : containers[i].statsError.isNotEmpty
                      ? 'err'
                      : '${containers[i].cpuPercent.toStringAsFixed(1)}%',
                  2,
                ),
                cell(
                  _isRunning(containers[i])
                      ? formatBytes(containers[i].memoryUsedBytes.toInt())
                      : '-',
                  3,
                ),
                cell(
                  containers[i].writableLayerBytes > 0
                      ? formatBytes(containers[i].writableLayerBytes.toInt())
                      : '-',
                  3,
                ),
                cell(
                  containers[i].hasCreatedAt()
                      ? formatAge(
                          now.difference(containers[i].createdAt.toDateTime()),
                        )
                      : '-',
                  2,
                  style: t.small(),
                ),
              ],
            ),
          ),
      ],
    );
  }
}

class _WatchRow extends StatelessWidget {
  const _WatchRow({required this.index, required this.dir, required this.now});

  final int index;
  final DirSize dir;
  final DateTime now;

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    final measured = dir.hasMeasuredAt()
        ? 'measured ${formatAge(now.difference(dir.measuredAt.toDateTime()))}'
        : 'not measured';
    return BarRow(
      index: index,
      child: Row(
        key: ValueKey('watch-${dir.path}'),
        children: [
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  dir.path,
                  overflow: TextOverflow.ellipsis,
                  style: t.data(t.p.emph),
                ),
                Text(
                  dir.error.isNotEmpty
                      ? dir.error
                      : '$measured${dir.complete ? '' : ', partial walk'}',
                  style: t.small(dir.error.isNotEmpty ? t.p.alarm : null),
                ),
              ],
            ),
          ),
          Text(
            '${dir.complete ? '' : '>= '}${formatBytes(dir.sizeBytes.toInt())}',
            style: t.data(),
          ),
        ],
      ),
    );
  }
}

class _Offenders extends StatelessWidget {
  const _Offenders({required this.offenders});

  final List<Offender> offenders;

  static const _kind = {
    OffenderKind.container: 'CONTAINER',
    OffenderKind.engine: 'ENGINE',
    OffenderKind.watchedDir: 'DIR',
    OffenderKind.sleeve: 'SLEEVE',
  };

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    if (offenders.isEmpty) {
      return Text('Nothing measured yet.', style: t.small());
    }
    final max = offenders.first.bytes;
    return Column(
      children: [
        for (var i = 0; i < offenders.length; i++)
          Padding(
            padding: const EdgeInsets.symmetric(vertical: 4),
            child: Column(
              key: ValueKey('offender-$i'),
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Row(
                  children: [
                    SizedBox(
                      width: 24,
                      child: Text('${i + 1}', style: t.legend()),
                    ),
                    Expanded(
                      child: Text.rich(
                        TextSpan(
                          children: [
                            TextSpan(
                              text: offenders[i].label,
                              style: t.data(t.p.emph),
                            ),
                            TextSpan(
                              text: '  ${_kind[offenders[i].kind]}',
                              style: t.legend(),
                            ),
                            if (offenders[i].detail.isNotEmpty)
                              TextSpan(
                                text: '  ${offenders[i].detail}',
                                style: t.small(),
                              ),
                          ],
                        ),
                        overflow: TextOverflow.ellipsis,
                      ),
                    ),
                    Text(formatBytes(offenders[i].bytes), style: t.data()),
                  ],
                ),
                const SizedBox(height: 4),
                Padding(
                  padding: const EdgeInsets.only(left: 24),
                  child: SegmentMeter(
                    percent: offenders[i].bytes * 100 / max,
                    segments: 40,
                    height: 6,
                    tone: Tone.info,
                  ),
                ),
              ],
            ),
          ),
      ],
    );
  }
}
