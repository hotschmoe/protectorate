import 'dart:async';

import 'package:flutter/material.dart';

import '../gen/protectorate/v1/events.pb.dart';
import '../gen/protectorate/v1/node.pb.dart';
import '../hubs/event_feed.dart';
import '../hubs/hub_client.dart';
import '../nodes/node_view.dart';
import 'node_detail_screen.dart';

const nodeEventKinds = {'node.metrics', 'node.unreachable'};

class NodesTab extends StatefulWidget {
  const NodesTab({
    super.key,
    required this.client,
    required this.feed,
    this.now,
    this.pollInterval = const Duration(seconds: 30),
  });

  final HubClient client;
  final EventFeed feed;
  final DateTime Function()? now;
  final Duration pollInterval;

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

  void _open(NodeStatus s) {
    Navigator.of(context).push(
      MaterialPageRoute<void>(
        builder: (_) => NodeDetailScreen(
          client: widget.client,
          feed: widget.feed,
          name: s.node.name,
          now: widget.now,
        ),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    if (_loading) return const LinearProgressIndicator();
    return Column(
      children: [
        if (_error != null)
          MaterialBanner(
            content: Text('Showing the last list. Refresh failed: $_error'),
            actions: [
              TextButton(onPressed: _refresh, child: const Text('Retry')),
            ],
          ),
        Expanded(
          child: _nodes.isEmpty
              ? const Center(child: Text('No nodes.'))
              : RefreshIndicator(
                  onRefresh: _refresh,
                  child: LayoutBuilder(
                    builder: (context, box) {
                      final w = box.maxWidth;
                      final cols = (w / 420).floor().clamp(1, 6);
                      final cardW = (w - 16 - (cols - 1) * 8) / cols;
                      return SingleChildScrollView(
                        physics: const AlwaysScrollableScrollPhysics(),
                        padding: const EdgeInsets.all(8),
                        child: Wrap(
                          spacing: 8,
                          runSpacing: 8,
                          children: [
                            for (final s in _nodes)
                              SizedBox(
                                width: cardW,
                                child: NodeCard(
                                  key: ValueKey('node-${s.node.name}'),
                                  status: s,
                                  age: statusAge(s, _receivedAt, _now()),
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

Severity freshnessSeverity(BuildContext context, NodeFreshness f) {
  final cs = Theme.of(context).colorScheme;
  return switch (f) {
    NodeFreshness.NODE_FRESHNESS_UNREACHABLE => Severity(
      cs.error,
      cs.errorContainer,
      cs.onErrorContainer,
    ),
    NodeFreshness.NODE_FRESHNESS_STALE => severityFor(context, warnPercent),
    _ => severityFor(context, 0),
  };
}

String freshnessLabel(NodeFreshness f, Duration age) => switch (f) {
  NodeFreshness.NODE_FRESHNESS_UNREACHABLE => 'UNREACHABLE',
  NodeFreshness.NODE_FRESHNESS_STALE => 'STALE ${formatAge(age)}',
  NodeFreshness.NODE_FRESHNESS_FRESH => 'LIVE',
  _ => 'UNKNOWN',
};

class NodeCard extends StatelessWidget {
  const NodeCard({
    super.key,
    required this.status,
    required this.age,
    required this.onTap,
  });

  final NodeStatus status;
  final Duration age;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final cs = Theme.of(context).colorScheme;
    final text = Theme.of(context).textTheme;
    final f = status.freshness;
    final down = f == NodeFreshness.NODE_FRESHNESS_UNREACHABLE;
    final stale = f == NodeFreshness.NODE_FRESHNESS_STALE;
    final m = status.hasMetrics() ? status.metrics : null;
    final fsev = freshnessSeverity(context, f);
    final badges = alertBadges(status);
    final os = m != null && m.os.isNotEmpty ? m.os : status.node.os;
    final arch = m != null && m.arch.isNotEmpty ? m.arch : status.node.arch;
    final dim = stale;

    final rows = <Widget>[];
    if (m != null && !down) {
      rows.add(
        PercentBar(
          key: const ValueKey('cpu'),
          label: 'CPU',
          percent: m.cpu.usagePercent,
          trailing:
              '${formatPercent(m.cpu.usagePercent)}  ${m.cpu.logicalCores}c',
          dim: dim,
        ),
      );
      rows.add(
        PercentBar(
          key: const ValueKey('mem'),
          label: 'MEM',
          percent: m.memory.usedPercent,
          trailing:
              '${formatPercent(m.memory.usedPercent)}  ${formatBytes(m.memory.totalBytes.toInt())}',
          dim: dim,
        ),
      );
      final gpu = summarizeGpus(m);
      if (gpu != null) {
        rows.add(
          PercentBar(
            key: const ValueKey('gpu'),
            label: gpu.count > 1 ? 'GPU x${gpu.count}' : 'GPU',
            percent: gpu.utilPercent,
            trailing: gpu.utilPercent == null
                ? 'n/a'
                : formatPercent(gpu.utilPercent!),
            dim: dim,
            labelWidth: 44,
          ),
        );
        if (gpu.vramPercent != null) {
          rows.add(
            PercentBar(
              key: const ValueKey('vram'),
              label: 'VRAM',
              percent: gpu.vramPercent,
              trailing:
                  '${formatBytes(gpu.vramUsed)} / ${formatBytes(gpu.vramTotal)}',
              dim: dim,
            ),
          );
        }
      }
      final worst = worstFilesystem(m);
      if (worst != null) {
        rows.add(
          PercentBar(
            key: const ValueKey('disk'),
            label: 'DISK',
            percent: worst.usedPercent,
            trailing: '${formatPercent(worst.usedPercent)}  ${worst.mount}',
            dim: dim,
          ),
        );
      }
    }

    return Card(
      margin: EdgeInsets.zero,
      clipBehavior: Clip.antiAlias,
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(12),
        side: BorderSide(
          color: down ? cs.error : (stale ? fsev.fill : cs.outlineVariant),
          width: down || stale ? 2 : 1,
        ),
      ),
      color: down ? cs.errorContainer.withValues(alpha: 0.35) : null,
      child: InkWell(
        onTap: onTap,
        child: Padding(
          padding: const EdgeInsets.all(12),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Row(
                children: [
                  Icon(
                    down ? Icons.cloud_off : Icons.memory,
                    size: 18,
                    color: down ? cs.error : cs.onSurfaceVariant,
                  ),
                  const SizedBox(width: 6),
                  Flexible(
                    child: Text(
                      status.node.name,
                      overflow: TextOverflow.ellipsis,
                      style: text.titleSmall,
                    ),
                  ),
                  if (status.node.local) ...[
                    const SizedBox(width: 6),
                    Text('hub', style: text.labelSmall),
                  ],
                  const Spacer(),
                  StatusBadge(
                    key: const ValueKey('freshness'),
                    text: freshnessLabel(f, age),
                    sev: fsev,
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
                style: text.bodySmall?.copyWith(color: cs.onSurfaceVariant),
                overflow: TextOverflow.ellipsis,
              ),
              const SizedBox(height: 6),
              if (down)
                Padding(
                  padding: const EdgeInsets.symmetric(vertical: 4),
                  child: Text(
                    [
                      m == null
                          ? 'Never reached.'
                          : 'No metrics since ${formatAge(age)}.',
                      if (status.error.isNotEmpty) status.error,
                    ].join(' '),
                    key: const ValueKey('down-note'),
                    style: text.bodySmall?.copyWith(color: cs.error),
                  ),
                )
              else if (m == null)
                Text('No metrics yet.', style: text.bodySmall)
              else
                ...rows,
              if (m != null && !down)
                Padding(
                  padding: const EdgeInsets.only(top: 4),
                  child: Text(
                    m.hasEngine()
                        ? '${m.engine.kind}: ${m.engine.containers.length} containers'
                        : 'no container engine',
                    key: const ValueKey('containers'),
                    style: text.bodySmall?.copyWith(color: cs.onSurfaceVariant),
                  ),
                ),
              if (badges.isNotEmpty)
                Padding(
                  padding: const EdgeInsets.only(top: 6),
                  child: Wrap(
                    spacing: 6,
                    runSpacing: 4,
                    children: [
                      for (final b in badges)
                        StatusBadge(
                          key: ValueKey('alert-${b.label}'),
                          text: b.label,
                          sev: b.critical
                              ? severityFor(context, 100)
                              : severityFor(context, warnPercent),
                        ),
                    ],
                  ),
                ),
            ],
          ),
        ),
      ),
    );
  }
}
