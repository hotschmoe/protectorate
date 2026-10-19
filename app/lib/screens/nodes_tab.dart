import 'dart:async';

import 'package:flutter/material.dart';

import '../gen/protectorate/v1/events.pb.dart';
import '../gen/protectorate/v1/node.pb.dart';
import '../hubs/event_feed.dart';
import '../hubs/hub_client.dart';
import '../hubs/hub_profile.dart';
import '../nodes/add_node.dart';
import '../nodes/node_view.dart';
import '../nodes/version_skew.dart';
import '../theme/tokens.dart';
import '../vfd/lamp.dart';
import '../vfd/meter.dart';
import '../vfd/panel.dart';
import 'node_detail_screen.dart';

const nodeEventKinds = {
  'node.metrics',
  'node.unreachable',
  'node.joined',
  'node.invited',
  'node.removed',
};

// The hub's keep-alive on the event stream; it shows as a lamp flicker, not a
// log line.
const heartbeatKind = 'heartbeat';

class NodesTab extends StatefulWidget {
  const NodesTab({
    super.key,
    required this.hub,
    required this.client,
    required this.feed,
    this.now,
    this.pollInterval = const Duration(seconds: 30),
    this.history,
    this.versions = const Versions(),
  });

  final Versions versions;
  final HubProfile hub;
  final HubClient client;
  final EventFeed feed;
  final DateTime Function()? now;
  final Duration pollInterval;
  final NodeHistory? history;

  @override
  State<NodesTab> createState() => _NodesTabState();
}

class _NodesTabState extends State<NodesTab> {
  List<NodeStatus> _nodes = const [];
  DateTime _receivedAt = DateTime.now();
  Object? _error;
  bool _loading = true;
  StreamSubscription<Event>? _sub;
  Timer? _poll;
  Timer? _tick;
  Timer? _debounce;

  late final NodeHistory _history = widget.history ?? NodeHistory();

  DateTime _now() => (widget.now ?? DateTime.now)();

  @override
  void initState() {
    super.initState();
    _sub = widget.feed.incoming.listen((e) {
      if (!nodeEventKinds.contains(e.kind)) return;
      _debounce?.cancel();
      _debounce = Timer(const Duration(milliseconds: 300), _refresh);
    });
    _poll = Timer.periodic(widget.pollInterval, (_) => _refresh());
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
    _debounce?.cancel();
    super.dispose();
  }

  Future<void> _refresh() async {
    try {
      final r = await widget.client.node.listNodes(ListNodesRequest());
      _history.recordAll(r.nodes);
      if (mounted) {
        setState(() {
          _nodes = sortNodes(r.nodes);
          _receivedAt = _now();
          _error = null;
        });
      }
    } on Object catch (e) {
      if (mounted) setState(() => _error = e);
    } finally {
      if (mounted) setState(() => _loading = false);
    }
  }

  void _add() =>
      showAddNode(context, client: widget.client, onChanged: _refresh);

  Future<void> _open(NodeStatus s) async {
    await Navigator.of(context).push(
      MaterialPageRoute<void>(
        builder: (_) => NodeDetailScreen(
          hub: widget.hub,
          client: widget.client,
          feed: widget.feed,
          name: s.node.name,
          now: widget.now,
          history: _history,
          versions: widget.versions,
        ),
      ),
    );
    _refresh();
  }

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    if (_loading) return const BusyBar();
    return Column(
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(
            Tokens.gap,
            Tokens.gap,
            Tokens.gap,
            0,
          ),
          child: Row(
            children: [
              Expanded(
                child: Text('${_nodes.length} NODES', style: t.legend()),
              ),
              OutlinedButton.icon(
                key: const ValueKey('add-node'),
                onPressed: _add,
                icon: const Icon(Icons.add, size: 18),
                label: const Text('ADD NODE'),
              ),
            ],
          ),
        ),
        if (_error != null)
          Padding(
            padding: const EdgeInsets.fromLTRB(12, 12, 12, 0),
            child: Row(
              children: [
                Expanded(
                  child: NoteStrip(
                    'Showing the last list. Refresh failed: $_error',
                  ),
                ),
                TextButton(onPressed: _refresh, child: const Text('RETRY')),
              ],
            ),
          ),
        Expanded(
          child: _nodes.isEmpty
              ? Center(child: Text('NO NODES', style: t.legend()))
              : RefreshIndicator(
                  onRefresh: _refresh,
                  child: LayoutBuilder(
                    builder: (context, box) {
                      const pad = Tokens.gap;
                      const gap = Tokens.gap;
                      final w = box.maxWidth;
                      final cols = (w / 420).floor().clamp(1, 6);
                      final cardW = (w - 2 * pad - (cols - 1) * gap) / cols;
                      return SingleChildScrollView(
                        physics: const AlwaysScrollableScrollPhysics(),
                        padding: const EdgeInsets.all(pad),
                        child: Wrap(
                          spacing: gap,
                          runSpacing: gap,
                          children: [
                            for (final s in _nodes)
                              SizedBox(
                                width: cardW,
                                child: NodeCard(
                                  key: ValueKey('node-${s.node.name}'),
                                  status: s,
                                  age: statusAge(s, _receivedAt, _now()),
                                  cpuHistory: _history.cpu(s.node.name),
                                  hubVersion: widget.versions.hub,
                                  onTap: () => _open(s),
                                ),
                              ),
                          ],
                        ),
                      );
                    },
                  ),
                ),
        ),
      ],
    );
  }
}

int _rank(NodeStatus s) => switch (s.freshness) {
  NodeFreshness.NODE_FRESHNESS_UNREACHABLE => 0,
  NodeFreshness.NODE_FRESHNESS_STALE => 1,
  _ => s.alerts.isNotEmpty ? 2 : 3,
};

List<NodeStatus> sortNodes(Iterable<NodeStatus> nodes) {
  final list = nodes.toList();
  list.sort((a, b) {
    final c = _rank(a).compareTo(_rank(b));
    return c != 0 ? c : a.node.name.compareTo(b.node.name);
  });
  return list;
}

Tone freshnessTone(NodeFreshness f) => switch (f) {
  NodeFreshness.NODE_FRESHNESS_UNREACHABLE => Tone.alarm,
  NodeFreshness.NODE_FRESHNESS_STALE => Tone.warn,
  NodeFreshness.NODE_FRESHNESS_FRESH => Tone.ok,
  NodeFreshness.NODE_FRESHNESS_INVITED => Tone.info,
  _ => Tone.dim,
};

String freshnessLabel(NodeFreshness f, Duration age) => switch (f) {
  NodeFreshness.NODE_FRESHNESS_UNREACHABLE => 'UNREACHABLE',
  NodeFreshness.NODE_FRESHNESS_STALE => 'STALE ${formatAge(age)}',
  NodeFreshness.NODE_FRESHNESS_FRESH => 'LIVE',
  NodeFreshness.NODE_FRESHNESS_INVITED => 'PENDING',
  _ => 'UNKNOWN',
};

class FreshnessLamp extends StatelessWidget {
  const FreshnessLamp({super.key, required this.freshness, required this.age});

  final NodeFreshness freshness;
  final Duration age;

  @override
  Widget build(BuildContext context) => LampLegend(
    label: freshnessLabel(freshness, age),
    tone: freshnessTone(freshness),
  );
}

class AlertStrip extends StatelessWidget {
  const AlertStrip({super.key, required this.badges});

  final List<AlertBadge> badges;

  @override
  Widget build(BuildContext context) => Wrap(
    spacing: 6,
    runSpacing: 6,
    children: [
      for (final b in badges)
        Annunciator(
          key: ValueKey('alert-${b.label}'),
          label: b.label,
          tone: b.critical ? Tone.alarm : Tone.warn,
        ),
    ],
  );
}

List<Widget> nodeMeters(
  NodeMetrics m, {
  required bool dim,
  bool detail = false,
}) {
  final rows = <Widget>[
    MeterRow(
      key: const ValueKey('cpu'),
      label: 'CPU',
      percent: m.cpu.usagePercent,
      value: '${formatPercent(m.cpu.usagePercent)}  ${m.cpu.logicalCores}c',
      dim: dim,
    ),
    MeterRow(
      key: const ValueKey('mem'),
      label: 'MEM',
      percent: m.memory.usedPercent,
      value: detail
          ? '${formatBytes(m.memory.usedBytes.toInt())} / ${formatBytes(m.memory.totalBytes.toInt())}'
          : '${formatPercent(m.memory.usedPercent)}  ${formatBytes(m.memory.totalBytes.toInt())}',
      dim: dim,
    ),
  ];
  if (detail) {
    if (m.memory.swapTotalBytes > 0) {
      rows.add(
        MeterRow(
          key: const ValueKey('swap'),
          label: 'SWAP',
          percent: m.memory.swapUsedPercent,
          value:
              '${formatBytes(m.memory.swapUsedBytes.toInt())} / ${formatBytes(m.memory.swapTotalBytes.toInt())}',
          dim: dim,
        ),
      );
    }
    return rows;
  }
  final worst = worstFilesystem(m);
  if (worst != null) {
    rows.add(
      MeterRow(
        key: const ValueKey('disk'),
        label: 'DISK',
        percent: worst.usedPercent,
        value: '${formatPercent(worst.usedPercent)}  ${worst.mount}',
        dim: dim,
      ),
    );
  }
  for (final g in m.gpus) {
    rows.add(GpuLines(key: ValueKey('gpu-${g.index}'), gpu: g, dim: dim));
  }
  for (final (i, n) in m.npus.indexed) {
    rows.add(NpuLines(key: ValueKey('npu-$i'), npu: n, index: i, dim: dim));
  }
  return rows;
}

class NodeCard extends StatelessWidget {
  const NodeCard({
    super.key,
    required this.status,
    required this.age,
    required this.onTap,
    this.cpuHistory = const [],
    this.hubVersion = '',
  });

  final String hubVersion;
  final NodeStatus status;
  final Duration age;
  final VoidCallback onTap;
  final List<double?> cpuHistory;

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    final f = status.freshness;
    final down = f == NodeFreshness.NODE_FRESHNESS_UNREACHABLE;
    final stale = f == NodeFreshness.NODE_FRESHNESS_STALE;
    final m = status.hasMetrics() ? status.metrics : null;
    final badges = alertBadges(status);
    final os = m != null && m.os.isNotEmpty ? m.os : status.node.os;
    final arch = m != null && m.arch.isNotEmpty ? m.arch : status.node.arch;

    return Panel(
      tone: down ? Tone.alarm : (stale ? Tone.warn : null),
      onTap: onTap,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Expanded(
                child: Row(
                  children: [
                    Flexible(
                      child: Text(
                        status.node.name,
                        overflow: TextOverflow.ellipsis,
                        style: t.title(down ? t.p.alarm : null),
                      ),
                    ),
                    if (status.node.local) ...[
                      const SizedBox(width: 8),
                      const Annunciator(label: 'HUB', tone: Tone.info),
                    ],
                    if (status.draining) ...[
                      const SizedBox(width: 8),
                      const Annunciator(label: 'REMOVING', tone: Tone.warn),
                    ],
                  ],
                ),
              ),
              const SizedBox(width: 8),
              FreshnessLamp(
                key: const ValueKey('freshness'),
                freshness: f,
                age: age,
              ),
            ],
          ),
          const SizedBox(height: 2),
          Text(
            [
              if (os.isNotEmpty) '$os/$arch',
              if (m != null && m.platform.isNotEmpty) m.platform,
              if (m != null && !down)
                'up ${formatUptime(m.uptimeSeconds.toInt())}',
            ].join('  '),
            style: t.small(),
            overflow: TextOverflow.ellipsis,
          ),
          const SizedBox(height: 10),
          if (down)
            Text(
              [
                m == null
                    ? 'Never reached.'
                    : 'No metrics since ${formatAge(age)}.',
                if (status.error.isNotEmpty) status.error,
              ].join(' '),
              key: const ValueKey('down-note'),
              style: t.data(t.p.alarm),
            )
          else if (status.node.invited)
            Text(
              'Invite pending: waiting for the machine to run the install line.',
              key: const ValueKey('pending-note'),
              style: t.small(),
            )
          else if (m == null)
            Text('No metrics yet.', style: t.small())
          else ...[
            ...nodeMeters(m, dim: stale),
            if (cpuHistory.length > 1) ...[
              const SizedBox(height: 8),
              Row(
                crossAxisAlignment: CrossAxisAlignment.end,
                children: [
                  SizedBox(
                    width: 4 * (11 * Tokens.monoAdvance + 1.4) + 8,
                    child: Text('HIST', style: t.legend()),
                  ),
                  Expanded(
                    child: DotMatrixGraph(
                      samples: cpuHistory,
                      rows: 6,
                      pitch: 5,
                      dim: stale,
                    ),
                  ),
                ],
              ),
            ],
          ],
          if (m != null && !down)
            Padding(
              padding: const EdgeInsets.only(top: 8),
              child: Text(
                m.hasEngine()
                    ? '${m.engine.kind}: ${m.engine.containers.length} containers'
                    : 'no container engine',
                key: const ValueKey('containers'),
                style: t.small(),
              ),
            ),
          if (badges.isNotEmpty)
            Padding(
              padding: const EdgeInsets.only(top: 10),
              child: AlertStrip(badges: badges),
            ),
          if (nodeSkewed(status, hubVersion))
            Padding(
              padding: const EdgeInsets.only(top: 10),
              child: SkewBadge(
                key: const ValueKey('skew'),
                nodeVersion: nodeVersion(status),
                hubVersion: hubVersion,
              ),
            ),
        ],
      ),
    );
  }
}
