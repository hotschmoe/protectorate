import 'dart:async';

import 'package:flutter/material.dart';

import '../gen/protectorate/v1/node.pb.dart';
import '../hubs/event_feed.dart';
import '../hubs/hub_client.dart';
import '../hubs/hub_profile.dart';
import '../nodes/node_view.dart';
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
  });

  final HubProfile hub;
  final HubClient client;
  final EventFeed feed;
  final String name;
  final DateTime Function()? now;

  @override
  State<NodeDetailScreen> createState() => _NodeDetailScreenState();
}

class _NodeDetailScreenState extends State<NodeDetailScreen> {
  NodeStatus? _status;
  DateTime _receivedAt = DateTime.now();
  Object? _error;
  StreamSubscription<dynamic>? _sub;
  Timer? _poll;
  Timer? _tick;

  DateTime _now() => (widget.now ?? DateTime.now)();

  @override
  void initState() {
    super.initState();
    _sub = widget.feed.incoming.listen((e) {
      if (nodeEventKinds.contains(e.kind) && e.subject == widget.name) {
        _refresh();
      }
    });
    _poll = Timer.periodic(const Duration(seconds: 30), (_) => _refresh());
    _tick = Timer.periodic(const Duration(seconds: 5), (_) {
      if (mounted) setState(() {});
    });
    _refresh();
  }

  @override
  void dispose() {
    _sub?.cancel();
    _poll?.cancel();
    _tick?.cancel();
    super.dispose();
  }

  Future<void> _refresh() async {
    try {
      final s = await widget.client.node.getNode(
        GetNodeRequest(name: widget.name),
      );
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
                ? Center(child: Text('$_error'))
                : const LinearProgressIndicator())
          : NodeDetail(
              status: s,
              age: statusAge(s, _receivedAt, _now()),
              now: _now(),
              error: _error,
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

class NodeDetail extends StatelessWidget {
  const NodeDetail({
    super.key,
    required this.status,
    required this.age,
    required this.now,
    this.error,
    this.onManageSleeves,
  });

  final VoidCallback? onManageSleeves;
  final NodeStatus status;
  final Duration age;
  final DateTime now;
  final Object? error;

  @override
  Widget build(BuildContext context) {
    final cs = Theme.of(context).colorScheme;
    final text = Theme.of(context).textTheme;
    final f = status.freshness;
    final down = f == NodeFreshness.NODE_FRESHNESS_UNREACHABLE;
    final stale = f == NodeFreshness.NODE_FRESHNESS_STALE;
    final m = status.hasMetrics() ? status.metrics : null;
    final dim = stale || down;
    final badges = alertBadges(status);

    final sections = <Widget>[
      Wrap(
        spacing: 8,
        runSpacing: 4,
        crossAxisAlignment: WrapCrossAlignment.center,
        children: [
          StatusBadge(
            key: const ValueKey('freshness'),
            text: freshnessLabel(f, age),
            sev: freshnessSeverity(context, f),
          ),
          for (final b in badges)
            StatusBadge(
              text: b.label,
              sev: b.critical
                  ? severityFor(context, 100)
                  : severityFor(context, warnPercent),
            ),
          Text(
            [
              status.node.address,
              if (status.node.tier.isNotEmpty) status.node.tier,
              ...status.node.tags,
            ].join('  '),
            style: text.bodySmall?.copyWith(color: cs.onSurfaceVariant),
          ),
        ],
      ),
      if (error != null)
        _Note('Refresh failed, showing the last result: $error', cs.error),
      if (status.error.isNotEmpty) _Note(status.error, cs.error),
      if (down)
        _Note(
          m == null
              ? 'This node has never been reached. No metrics.'
              : 'Unreachable. The figures below are from ${formatAge(age)} and are not current.',
          cs.error,
        ),
      if (stale)
        _Note(
          'Last poll failed. Figures are from ${formatAge(age)} and may be outdated.',
          severityFor(context, warnPercent).fill,
        ),
    ];

    if (m != null) {
      sections.addAll([
        _Section(
          'Host',
          Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                [
                  '${m.os}/${m.arch}',
                  m.platform,
                  'agent ${m.agentVersion}',
                  'up ${formatUptime(m.uptimeSeconds.toInt())}',
                ].where((e) => e.isNotEmpty).join('  '),
                style: text.bodySmall,
              ),
              const SizedBox(height: 6),
              PercentBar(
                key: const ValueKey('cpu'),
                label: 'CPU',
                percent: m.cpu.usagePercent,
                trailing:
                    '${formatPercent(m.cpu.usagePercent)}  ${m.cpu.logicalCores}c',
                dim: dim,
              ),
              if (m.cpu.hasLoad1())
                Padding(
                  padding: const EdgeInsets.only(left: 44),
                  child: Text(
                    'load ${m.cpu.load1.toStringAsFixed(2)} ${m.cpu.load5.toStringAsFixed(2)} ${m.cpu.load15.toStringAsFixed(2)}',
                    style: text.bodySmall?.copyWith(color: cs.onSurfaceVariant),
                  ),
                ),
              PercentBar(
                key: const ValueKey('mem'),
                label: 'MEM',
                percent: m.memory.usedPercent,
                trailing:
                    '${formatBytes(m.memory.usedBytes.toInt())} / ${formatBytes(m.memory.totalBytes.toInt())}',
                dim: dim,
              ),
              if (m.memory.swapTotalBytes > 0)
                PercentBar(
                  key: const ValueKey('swap'),
                  label: 'SWAP',
                  percent: m.memory.swapUsedPercent,
                  trailing:
                      '${formatBytes(m.memory.swapUsedBytes.toInt())} / ${formatBytes(m.memory.swapTotalBytes.toInt())}',
                  dim: dim,
                ),
            ],
          ),
        ),
        if (m.warnings.isNotEmpty)
          _Section(
            'Collection warnings',
            Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [for (final w in m.warnings) Text(w)],
            ),
          ),
        _Section(
          'Filesystems',
          m.filesystems.isEmpty
              ? const Text('None reported.')
              : Column(
                  children: [
                    for (final fs in m.filesystems)
                      Padding(
                        padding: const EdgeInsets.only(bottom: 6),
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            Text(
                              '${fs.mount}  ${fs.device} ${fs.fstype}',
                              style: text.bodySmall,
                              overflow: TextOverflow.ellipsis,
                            ),
                            PercentBar(
                              key: ValueKey('fs-${fs.mount}'),
                              label: '',
                              labelWidth: 0,
                              trailingWidth: 200,
                              percent: fs.usedPercent,
                              trailing:
                                  '${formatPercent(fs.usedPercent)}  ${formatBytes(fs.usedBytes.toInt())} / ${formatBytes(fs.totalBytes.toInt())}',
                              dim: dim,
                            ),
                          ],
                        ),
                      ),
                  ],
                ),
        ),
        _Section(
          'GPUs',
          m.gpus.isEmpty
              ? const Text('None.')
              : Column(
                  children: [
                    for (final g in m.gpus)
                      Padding(
                        padding: const EdgeInsets.only(bottom: 6),
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            Text(
                              '#${g.index} ${g.vendor} ${g.name}${g.temperatureCelsius >= 0 ? '  ${g.temperatureCelsius.round()} C' : ''}',
                              style: text.bodySmall,
                            ),
                            PercentBar(
                              label: 'Util',
                              percent: g.utilizationPercent >= 0
                                  ? g.utilizationPercent
                                  : null,
                              trailing: g.utilizationPercent >= 0
                                  ? formatPercent(g.utilizationPercent)
                                  : 'n/a',
                              dim: dim,
                            ),
                            PercentBar(
                              label: 'VRAM',
                              percent: g.memoryTotalBytes > 0
                                  ? g.memoryUsedBytes.toInt() *
                                        100 /
                                        g.memoryTotalBytes.toInt()
                                  : null,
                              trailing:
                                  '${formatBytes(g.memoryUsedBytes.toInt())} / ${formatBytes(g.memoryTotalBytes.toInt())}',
                              dim: dim,
                            ),
                          ],
                        ),
                      ),
                  ],
                ),
        ),
        _Section(
          m.hasEngine()
              ? 'Container engine: ${m.engine.kind} ${m.engine.version}'
              : 'Container engine',
          m.hasEngine()
              ? _EngineDisk(engine: m.engine, now: now)
              : const Text('No container engine reachable.'),
        ),
        if (m.hasEngine())
          _Section(
            'Containers (${m.engine.containers.length}), by writable layer',
            _ContainerTable(containers: containersBySize(m)),
          ),
        if (m.sleeves.isNotEmpty || onManageSleeves != null)
          _Section(
            'Sleeves (${m.sleeves.length}), by workspace size',
            Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                if (m.sleeves.isEmpty) const Text('No sleeves.'),
                for (final s in sleevesBySize(m))
                  SleeveRow(sleeve: s, dim: dim),
                if (onManageSleeves != null)
                  Align(
                    alignment: Alignment.centerRight,
                    child: TextButton.icon(
                      key: const ValueKey('manage-sleeves'),
                      onPressed: onManageSleeves,
                      icon: const Icon(Icons.settings_ethernet),
                      label: const Text('Manage sleeves'),
                    ),
                  ),
              ],
            ),
          ),
        _Section(
          'Watched directories',
          m.watchDirs.isEmpty
              ? const Text('None configured.')
              : Column(
                  children: [
                    for (final d in m.watchDirs) _WatchRow(dir: d, now: now),
                  ],
                ),
        ),
        _Section('Top disk offenders', _Offenders(offenders: topOffenders(m))),
        Padding(
          padding: const EdgeInsets.symmetric(vertical: 8),
          child: Text(
            'Cleanup actions are not available yet. They will require explicit confirmation.',
            key: const ValueKey('cleanup-note'),
            style: text.bodySmall?.copyWith(color: cs.onSurfaceVariant),
          ),
        ),
      ]);
    }

    return LayoutBuilder(
      builder: (context, box) => Align(
        alignment: Alignment.topCenter,
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 900),
          child: ListView(
            padding: const EdgeInsets.all(12),
            children: [
              for (final s in sections)
                Padding(padding: const EdgeInsets.only(bottom: 10), child: s),
            ],
          ),
        ),
      ),
    );
  }
}

class _Note extends StatelessWidget {
  const _Note(this.text, this.color);

  final String text;
  final Color color;

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.all(10),
      decoration: BoxDecoration(
        border: Border.all(color: color, width: 1.5),
        borderRadius: BorderRadius.circular(8),
      ),
      child: Text(text),
    );
  }
}

class _Section extends StatelessWidget {
  const _Section(this.title, this.child);

  final String title;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    return Card(
      margin: EdgeInsets.zero,
      child: Padding(
        padding: const EdgeInsets.all(12),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(title, style: Theme.of(context).textTheme.titleSmall),
            const SizedBox(height: 8),
            child,
          ],
        ),
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
    final text = Theme.of(context).textTheme;
    if (!engine.hasDisk()) {
      return const Text('Disk usage not measured yet.');
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
        for (final r in rows)
          Padding(
            padding: const EdgeInsets.symmetric(vertical: 2),
            child: Row(
              key: ValueKey('engine-${r.$1}'),
              children: [
                Expanded(flex: 3, child: Text(r.$1)),
                Expanded(
                  flex: 2,
                  child: Text(
                    '${r.$2.count} items',
                    style: text.bodySmall,
                    textAlign: TextAlign.end,
                  ),
                ),
                Expanded(
                  flex: 3,
                  child: Text(
                    formatBytes(r.$2.sizeBytes.toInt()),
                    textAlign: TextAlign.end,
                  ),
                ),
                Expanded(
                  flex: 4,
                  child: Text(
                    '${formatBytes(r.$2.reclaimableBytes.toInt())} reclaimable',
                    style: text.bodySmall,
                    textAlign: TextAlign.end,
                  ),
                ),
              ],
            ),
          ),
        if (engine.hasDiskMeasuredAt())
          Padding(
            padding: const EdgeInsets.only(top: 4),
            child: Text(
              'measured ${formatAge(now.difference(engine.diskMeasuredAt.toDateTime()))}',
              style: text.bodySmall?.copyWith(
                color: Theme.of(context).colorScheme.onSurfaceVariant,
              ),
            ),
          ),
      ],
    );
  }
}

class _ContainerTable extends StatelessWidget {
  const _ContainerTable({required this.containers});

  final List<ContainerMetrics> containers;

  @override
  Widget build(BuildContext context) {
    final text = Theme.of(context).textTheme;
    final cs = Theme.of(context).colorScheme;
    if (containers.isEmpty) return const Text('No containers.');
    final head = text.labelSmall!.copyWith(color: cs.onSurfaceVariant);
    Widget cell(String s, int flex, {TextStyle? style, bool end = true}) =>
        Expanded(
          flex: flex,
          child: Text(
            s,
            style: style ?? text.bodySmall,
            textAlign: end ? TextAlign.end : TextAlign.start,
            overflow: TextOverflow.ellipsis,
          ),
        );
    return Column(
      children: [
        Row(
          children: [
            cell('Name', 5, style: head, end: false),
            cell('CPU', 2, style: head),
            cell('Mem', 3, style: head),
            cell('Writable', 3, style: head),
          ],
        ),
        const Divider(height: 8),
        for (final c in containers)
          Padding(
            padding: const EdgeInsets.symmetric(vertical: 2),
            child: Row(
              key: ValueKey('container-${c.name}'),
              children: [
                cell(
                  c.state.isEmpty || c.state == 'running'
                      ? c.name
                      : '${c.name} (${c.state})',
                  5,
                  end: false,
                ),
                cell(
                  c.statsError.isNotEmpty
                      ? 'err'
                      : '${c.cpuPercent.toStringAsFixed(1)}%',
                  2,
                ),
                cell(formatBytes(c.memoryUsedBytes.toInt()), 3),
                cell(
                  c.writableLayerBytes > 0
                      ? formatBytes(c.writableLayerBytes.toInt())
                      : '-',
                  3,
                ),
              ],
            ),
          ),
      ],
    );
  }
}

class _WatchRow extends StatelessWidget {
  const _WatchRow({required this.dir, required this.now});

  final DirSize dir;
  final DateTime now;

  @override
  Widget build(BuildContext context) {
    final text = Theme.of(context).textTheme;
    final cs = Theme.of(context).colorScheme;
    final measured = dir.hasMeasuredAt()
        ? 'measured ${formatAge(now.difference(dir.measuredAt.toDateTime()))}'
        : 'not measured';
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 3),
      child: Row(
        key: ValueKey('watch-${dir.path}'),
        children: [
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(dir.path, overflow: TextOverflow.ellipsis),
                Text(
                  dir.error.isNotEmpty
                      ? dir.error
                      : '$measured${dir.complete ? '' : ', partial walk'}',
                  style: text.bodySmall?.copyWith(
                    color: dir.error.isNotEmpty
                        ? cs.error
                        : cs.onSurfaceVariant,
                  ),
                ),
              ],
            ),
          ),
          Text(
            '${dir.complete ? '' : '>= '}${formatBytes(dir.sizeBytes.toInt())}',
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
    OffenderKind.container: 'container',
    OffenderKind.engine: 'engine',
    OffenderKind.watchedDir: 'dir',
    OffenderKind.sleeve: 'sleeve',
  };

  @override
  Widget build(BuildContext context) {
    if (offenders.isEmpty) return const Text('Nothing measured yet.');
    final text = Theme.of(context).textTheme;
    final cs = Theme.of(context).colorScheme;
    final max = offenders.first.bytes;
    return Column(
      children: [
        for (var i = 0; i < offenders.length; i++)
          Padding(
            padding: const EdgeInsets.symmetric(vertical: 3),
            child: Column(
              key: ValueKey('offender-$i'),
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Row(
                  children: [
                    Text('${i + 1}. ', style: text.bodySmall),
                    Expanded(
                      child: Text(
                        '${offenders[i].label}  (${_kind[offenders[i].kind]}${offenders[i].detail.isEmpty ? '' : ', ${offenders[i].detail}'})',
                        overflow: TextOverflow.ellipsis,
                      ),
                    ),
                    Text(formatBytes(offenders[i].bytes)),
                  ],
                ),
                const SizedBox(height: 2),
                ClipRRect(
                  borderRadius: BorderRadius.circular(3),
                  child: LinearProgressIndicator(
                    minHeight: 5,
                    value: offenders[i].bytes / max,
                    color: cs.tertiary,
                    backgroundColor: cs.surfaceContainerHighest,
                  ),
                ),
              ],
            ),
          ),
      ],
    );
  }
}
