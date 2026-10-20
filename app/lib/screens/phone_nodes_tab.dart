import 'dart:async';

import 'package:connectrpc/connect.dart';
import 'package:flutter/material.dart';

import '../gen/protectorate/v1/events.pb.dart';
import '../gen/protectorate/v1/node.pb.dart';
import '../hubs/event_feed.dart';
import '../hubs/hub_client.dart';
import '../hubs/hub_profile.dart';
import '../nodes/node_view.dart';
import '../nodes/version_skew.dart';
import '../theme/tokens.dart';
import '../vfd/lamp.dart';
import '../vfd/meter.dart';
import '../vfd/panel.dart';
import 'node_detail_screen.dart';
import 'nodes_tab.dart';

class PhoneNodesTab extends StatefulWidget {
  const PhoneNodesTab({
    super.key,
    required this.hub,
    required this.client,
    required this.feed,
    required this.history,
    required this.versions,
  });

  final HubProfile hub;
  final HubClient client;
  final EventFeed feed;
  final NodeHistory history;
  final Versions versions;

  @override
  State<PhoneNodesTab> createState() => _PhoneNodesTabState();
}

class _PhoneNodesTabState extends State<PhoneNodesTab> {
  List<NodeStatus> _nodes = const [];
  final _problemSince = <String, DateTime>{};
  DateTime _receivedAt = DateTime.now();
  Object? _error;
  bool _loading = true;
  Future<void>? _pending;
  CancelableSignal? _request;
  StreamSubscription<Event>? _events;
  Timer? _poll;
  Timer? _tick;
  Timer? _debounce;

  @override
  void initState() {
    super.initState();
    _events = widget.feed.incoming.listen((event) {
      if (!nodeEventKinds.contains(event.kind)) return;
      _debounce?.cancel();
      _debounce = Timer(const Duration(milliseconds: 300), _refresh);
    });
    _poll = Timer.periodic(const Duration(seconds: 30), (_) => _refresh());
    _tick = Timer.periodic(const Duration(seconds: 5), (_) {
      if (mounted) setState(() {});
    });
    _refresh();
  }

  @override
  void dispose() {
    _events?.cancel();
    _poll?.cancel();
    _tick?.cancel();
    _debounce?.cancel();
    _request?.cancel();
    super.dispose();
  }

  bool _troubled(NodeStatus node) =>
      (node.freshness != NodeFreshness.NODE_FRESHNESS_INVITED &&
          (node.freshness == NodeFreshness.NODE_FRESHNESS_UNREACHABLE ||
              node.freshness == NodeFreshness.NODE_FRESHNESS_STALE ||
              node.error.isNotEmpty)) ||
      node.alerts.isNotEmpty;

  Future<void> _refresh() => _pending ??= _load().whenComplete(() {
    _pending = null;
  });

  Future<void> _load() async {
    final request = _request = CancelableSignal();
    try {
      final response = await widget.client.node.listNodes(
        ListNodesRequest(),
        signal: request,
      );
      if (!mounted) return;
      final now = DateTime.now();
      final names = response.nodes.map((node) => node.node.name).toSet();
      _problemSince.removeWhere((name, _) => !names.contains(name));
      for (final node in response.nodes) {
        final name = node.node.name;
        if (!_troubled(node)) {
          _problemSince.remove(name);
          continue;
        }
        final missed =
            node.freshness == NodeFreshness.NODE_FRESHNESS_STALE ||
            node.freshness == NodeFreshness.NODE_FRESHNESS_UNREACHABLE;
        final since = missed ? now.subtract(statusAge(node, now, now)) : now;
        final previous = _problemSince[name];
        if (previous == null || since.isBefore(previous)) {
          _problemSince[name] = since;
        }
      }
      widget.history.recordAll(response.nodes);
      setState(() {
        _nodes = response.nodes.toList();
        _receivedAt = now;
        _error = null;
      });
    } on Object catch (error) {
      if (mounted) setState(() => _error = error);
    } finally {
      if (identical(_request, request)) _request = null;
      if (mounted) setState(() => _loading = false);
    }
  }

  Future<void> _open(NodeStatus node) async {
    await Navigator.of(context).push(
      MaterialPageRoute<void>(
        builder: (_) => NodeDetailScreen(
          hub: widget.hub,
          client: widget.client,
          feed: widget.feed,
          name: node.node.name,
          phone: true,
          history: widget.history,
          versions: widget.versions,
        ),
      ),
    );
    if (mounted) await _refresh();
  }

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    if (_loading) return const BusyBar();
    final nodes = [..._nodes]
      ..sort((a, b) {
        final aSince = _problemSince[a.node.name];
        final bSince = _problemSince[b.node.name];
        if (aSince != null && bSince == null) return -1;
        if (aSince == null && bSince != null) return 1;
        final age = aSince != null && bSince != null
            ? aSince.compareTo(bSince)
            : 0;
        return age != 0 ? age : a.node.name.compareTo(b.node.name);
      });
    final now = DateTime.now();
    return RefreshIndicator(
      onRefresh: _refresh,
      child: ListView(
        physics: const AlwaysScrollableScrollPhysics(),
        padding: const EdgeInsets.all(Tokens.gap),
        children: [
          Row(
            children: [
              Expanded(
                child: Text(
                  '${nodes.length} NODES · ${_problemSince.length} NEED ATTENTION',
                  style: t.legend(),
                ),
              ),
              IconButton(
                tooltip: 'Refresh nodes',
                onPressed: _refresh,
                icon: const Icon(Icons.refresh),
              ),
            ],
          ),
          Text(
            'Oldest problems first. Tap a node for details.',
            style: t.small(),
          ),
          if (_error != null) ...[
            const SizedBox(height: Tokens.gap),
            NoteStrip('Could not refresh nodes: $_error'),
            TextButton(onPressed: _refresh, child: const Text('RETRY')),
          ],
          if (nodes.isEmpty)
            Padding(
              padding: const EdgeInsets.symmetric(vertical: Tokens.gapL),
              child: Text('No nodes connected.', style: t.body()),
            ),
          for (final node in nodes) ...[
            const SizedBox(height: Tokens.gap),
            _PhoneNodeCard(
              key: ValueKey('phone-node-${node.node.name}'),
              node: node,
              age: statusAge(node, _receivedAt, now),
              onTap: () => _open(node),
            ),
          ],
        ],
      ),
    );
  }
}

class _PhoneNodeCard extends StatelessWidget {
  const _PhoneNodeCard({
    super.key,
    required this.node,
    required this.age,
    required this.onTap,
  });

  final NodeStatus node;
  final Duration age;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    final down = node.freshness == NodeFreshness.NODE_FRESHNESS_UNREACHABLE;
    final stale = node.freshness == NodeFreshness.NODE_FRESHNESS_STALE;
    final dim = down || stale;
    final metrics = node.hasMetrics() ? node.metrics : null;
    GpuMetrics? gpu;
    if (metrics != null) {
      for (final candidate in metrics.gpus) {
        if (gpu == null ||
            candidate.utilizationPercent > gpu.utilizationPercent) {
          gpu = candidate;
        }
      }
    }
    final disk = metrics == null ? null : primaryFilesystem(metrics);
    final badges = alertBadges(node);
    return Panel(
      tone: down ? Tone.alarm : (stale || badges.isNotEmpty ? Tone.warn : null),
      onTap: onTap,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Expanded(
                child: Text(
                  node.node.name,
                  style: t.title(down ? t.p.alarm : null),
                  overflow: TextOverflow.ellipsis,
                ),
              ),
              if (node.node.local)
                const Annunciator(label: 'HUB', tone: Tone.info),
              const SizedBox(width: 4),
              const Icon(Icons.chevron_right, size: 20),
            ],
          ),
          const SizedBox(height: 8),
          Wrap(
            spacing: 10,
            runSpacing: 6,
            crossAxisAlignment: WrapCrossAlignment.center,
            children: [
              FreshnessLamp(freshness: node.freshness, age: age),
              if (metrics != null)
                Text('Sample ${formatAge(age)}', style: t.small()),
              if (node.draining)
                const Annunciator(label: 'REMOVING', tone: Tone.warn),
            ],
          ),
          if (metrics == null)
            Padding(
              padding: const EdgeInsets.only(top: Tokens.gap),
              child: Text(
                node.node.invited
                    ? 'Waiting for this node to join.'
                    : 'No hardware sample yet.',
                style: t.small(),
              ),
            )
          else ...[
            const SizedBox(height: Tokens.gap),
            Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Expanded(
                  child: _HardwareReading(
                    label: 'CPU',
                    percent: metrics.hasCpu()
                        ? knownPercent(metrics.cpu.usagePercent)
                        : null,
                    detail: metrics.hasCpu()
                        ? '${metrics.cpu.logicalCores} cores'
                        : 'Unknown',
                    dim: dim,
                  ),
                ),
                const SizedBox(width: Tokens.gap),
                Expanded(
                  child: _HardwareReading(
                    label: 'MEMORY',
                    percent: metrics.hasMemory()
                        ? knownPercent(metrics.memory.usedPercent)
                        : null,
                    detail: metrics.hasMemory()
                        ? '${formatBytes(metrics.memory.usedBytes.toInt())} / ${formatBytes(metrics.memory.totalBytes.toInt())}'
                        : 'Unknown',
                    dim: dim,
                  ),
                ),
              ],
            ),
            const SizedBox(height: Tokens.gap),
            Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Expanded(
                  child: _HardwareReading(
                    label: metrics.gpus.length <= 1
                        ? 'GPU'
                        : '${metrics.gpus.length} GPUS · PEAK',
                    percent: gpu == null
                        ? null
                        : knownPercent(gpu.utilizationPercent),
                    detail: gpu == null ? 'None reported' : gpu.name,
                    dim: dim,
                  ),
                ),
                const SizedBox(width: Tokens.gap),
                Expanded(
                  child: _HardwareReading(
                    label: 'DISK',
                    percent: disk == null
                        ? null
                        : knownPercent(disk.usedPercent),
                    detail: disk == null
                        ? 'Unknown'
                        : '${formatBytes(disk.usedBytes.toInt())} / ${formatBytes(disk.totalBytes.toInt())}',
                    dim: dim,
                  ),
                ),
              ],
            ),
          ],
          if (node.error.isNotEmpty) ...[
            const SizedBox(height: Tokens.gap),
            Text(
              node.error,
              style: t.small(down ? t.p.alarm : t.p.warn),
              maxLines: 3,
              overflow: TextOverflow.ellipsis,
            ),
          ],
          if (badges.isNotEmpty) ...[
            const SizedBox(height: Tokens.gap),
            for (final badge in badges)
              Padding(
                padding: const EdgeInsets.only(top: 4),
                child: Text(
                  badge.label,
                  style: t.small(badge.critical ? t.p.alarm : t.p.warn),
                ),
              ),
          ],
        ],
      ),
    );
  }
}

class _HardwareReading extends StatelessWidget {
  const _HardwareReading({
    required this.label,
    required this.percent,
    required this.detail,
    required this.dim,
  });

  final String label;
  final double? percent;
  final String detail;
  final bool dim;

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          children: [
            Expanded(child: Text(label, style: t.legend())),
            Text(
              percent == null ? '—' : '${percent!.round()}%',
              style: t.data(dim ? t.p.dim : t.p.lit),
            ),
          ],
        ),
        const SizedBox(height: 5),
        SegmentMeter(percent: percent, dim: dim, segments: 12),
        const SizedBox(height: 5),
        Text(
          detail,
          style: t.small(),
          maxLines: 1,
          overflow: TextOverflow.ellipsis,
        ),
      ],
    );
  }
}
