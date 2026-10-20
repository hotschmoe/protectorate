import 'dart:async';

import 'package:flutter/material.dart';

import '../gen/protectorate/v1/events.pb.dart';
import '../gen/protectorate/v1/hub.pb.dart';
import '../gen/protectorate/v1/node.pb.dart';
import '../gen/protectorate/v1/quota.pb.dart';
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
import 'quota_tab.dart';

class OverviewTab extends StatefulWidget {
  const OverviewTab({
    super.key,
    required this.hub,
    required this.client,
    required this.feed,
    this.history,
    this.now,
    this.versions = const Versions(),
  });

  final Versions versions;
  final HubProfile hub;
  final HubClient client;
  final EventFeed feed;
  final NodeHistory? history;
  final DateTime Function()? now;

  @override
  State<OverviewTab> createState() => _OverviewTabState();
}

class _OverviewTabState extends State<OverviewTab> {
  late Future<GetStatusResponse> _status = _loadStatus();
  late final NodeHistory _history = widget.history ?? NodeHistory();
  List<NodeStatus> _nodes = const [];
  DateTime _receivedAt = DateTime.now();
  Object? _nodesError;
  bool _nodesLoaded = false;
  final Map<String, ProviderQuota> _quotas = {};
  Object? _quotaError;
  StreamSubscription<Event>? _sub;
  int _hubBeat = 0;
  final Map<String, int> _nodeBeats = {};
  Timer? _poll;
  Timer? _debounce;

  EventFeed get _feed => widget.feed;
  DateTime _now() => (widget.now ?? DateTime.now)();

  @override
  void initState() {
    super.initState();
    _sub = _feed.incoming.listen((e) {
      final q = quotaFromEvent(e);
      if (q != null) setState(() => _quotas[q.provider] = q);
      if (e.kind == heartbeatKind) setState(() => _hubBeat++);
      if (e.kind == 'node.metrics') {
        setState(
          () => _nodeBeats.update(e.subject, (n) => n + 1, ifAbsent: () => 1),
        );
      }
      if (nodeEventKinds.contains(e.kind)) {
        _debounce?.cancel();
        _debounce = Timer(const Duration(milliseconds: 300), _loadNodes);
      }
    });
    _poll = Timer.periodic(const Duration(seconds: 30), (_) => _loadNodes());
    _loadNodes();
    _loadQuota();
  }

  @override
  void dispose() {
    _sub?.cancel();
    _poll?.cancel();
    _debounce?.cancel();
    super.dispose();
  }

  Future<GetStatusResponse> _loadStatus() =>
      widget.client.hub.getStatus(GetStatusRequest());

  Future<void> _loadNodes() async {
    try {
      final r = await widget.client.node.listNodes(ListNodesRequest());
      _history.recordAll(r.nodes);
      if (!mounted) return;
      setState(() {
        _nodes = sortNodes(r.nodes);
        _receivedAt = _now();
        _nodesError = null;
      });
    } on Object catch (e) {
      if (mounted) setState(() => _nodesError = e);
    } finally {
      if (mounted) setState(() => _nodesLoaded = true);
    }
  }

  Future<void> _loadQuota() async {
    try {
      final r = await widget.client.quota.getQuota(GetQuotaRequest());
      if (!mounted) return;
      setState(() {
        for (final q in r.quotas) {
          _quotas[q.provider] = q;
        }
        _quotaError = null;
      });
    } on Object catch (e) {
      if (mounted) setState(() => _quotaError = e);
    }
  }

  void _openQuota() {
    Navigator.of(context).push(
      MaterialPageRoute<void>(
        builder: (_) => Scaffold(
          appBar: AppBar(title: const Text('Quota')),
          body: SafeArea(
            child: QuotaTab(
              client: widget.client,
              feed: widget.feed,
              now: widget.now,
            ),
          ),
        ),
      ),
    );
  }

  void _open(NodeStatus s) {
    Navigator.of(context).push(
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
  }

  @override
  Widget build(BuildContext context) {
    final quotas = [for (final k in _quotas.keys.toList()..sort()) _quotas[k]!];
    return LayoutBuilder(
      builder: (context, box) {
        final wide = box.maxWidth >= 1100;
        final status = _StatusPanel(
          nodes: _nodes,
          quotas: quotas,
          feed: _feed,
          hub: _status,
          onRetry: () => setState(() => _status = _loadStatus()),
          beat: _hubBeat,
          hubCpu: _history.cpu(
            _nodes.where((n) => n.node.local).firstOrNull?.node.name ?? '',
          ),
          wide: wide,
          versions: widget.versions,
        );
        final board = Panel(
          legend: 'MACHINE ROOM',
          trailing: Text(
            '${_nodes.length} NODES',
            style: Tokens.of(context).legend(),
          ),
          padding: const EdgeInsets.symmetric(vertical: 6, horizontal: 4),
          child: _Board(
            nodes: _nodes,
            loaded: _nodesLoaded,
            error: _nodesError,
            ageOf: (s) => statusAge(s, _receivedAt, _now()),
            beats: _nodeBeats,
            wide: wide,
            onOpen: _open,
            hubVersion: widget.versions.hub,
          ),
        );
        final quota = Panel(
          legend: 'SUBSCRIPTIONS',
          child: Semantics(
            button: true,
            label: 'Open quota',
            child: InkWell(
              key: const ValueKey('open-quota'),
              onTap: _openQuota,
              child: _QuotaSummary(
                quotas: quotas,
                error: _quotaError,
                now: _now(),
              ),
            ),
          ),
        );
        final events = _EventLog(feed: _feed);
        Widget column(List<Widget> ws) => Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            for (var i = 0; i < ws.length; i++) ...[
              if (i > 0) const SizedBox(height: Tokens.gap),
              ws[i],
            ],
          ],
        );
        return SingleChildScrollView(
          padding: EdgeInsets.all(wide ? Tokens.gapL : Tokens.gap),
          child: Center(
            child: ConstrainedBox(
              constraints: const BoxConstraints(maxWidth: Tokens.boardMaxWidth),
              child: wide
                  ? column([
                      status,
                      board,
                      Row(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Expanded(flex: 3, child: quota),
                          const SizedBox(width: Tokens.gapL),
                          Expanded(flex: 2, child: events),
                        ],
                      ),
                    ])
                  : column([status, board, quota, events]),
            ),
          ),
        );
      },
    );
  }
}

class _StatusPanel extends StatelessWidget {
  const _StatusPanel({
    required this.nodes,
    required this.quotas,
    required this.feed,
    required this.hub,
    required this.onRetry,
    required this.beat,
    required this.hubCpu,
    required this.wide,
    required this.versions,
  });

  final Versions versions;
  final List<NodeStatus> nodes;
  final List<ProviderQuota> quotas;
  final EventFeed feed;
  final Future<GetStatusResponse> hub;
  final VoidCallback onRetry;
  final int beat;
  final List<double?> hubCpu;
  final bool wide;

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    final online = nodes
        .where((n) => n.freshness == NodeFreshness.NODE_FRESHNESS_FRESH)
        .length;
    final down = nodes
        .where((n) => n.freshness == NodeFreshness.NODE_FRESHNESS_UNREACHABLE)
        .length;
    final stale = nodes
        .where((n) => n.freshness == NodeFreshness.NODE_FRESHNESS_STALE)
        .length;
    final badges = [for (final n in nodes) ...alertBadges(n)];
    final critical = badges.where((b) => b.critical).length;
    final skew = versionSkew(
      hubVersion: versions.hub,
      appVersion: versions.app,
      nodes: nodes,
    );
    final fresh = quotas
        .where((q) => q.freshness == Freshness.FRESHNESS_FRESH)
        .length;
    final unknown = quotas.where(quotaUnknown).length;
    Widget tile(String legend, String value, String unit, Tone tone) =>
        ReadoutTile(
          legend: legend,
          readout: SegmentReadout(
            value: value,
            unit: unit,
            size: Tokens.readoutL,
            color: tone == Tone.lit ? t.p.emph : t.tone(tone),
          ),
        );
    final tiles = Wrap(
      spacing: 28,
      runSpacing: 14,
      children: [
        tile(
          'NODES LIVE',
          '$online',
          'OF ${nodes.length}',
          online < nodes.length ? Tone.warn : Tone.lit,
        ),
        tile(
          'ALERTS',
          '${badges.length}',
          critical > 0 ? '$critical CRIT' : '',
          critical > 0
              ? Tone.alarm
              : badges.isNotEmpty
              ? Tone.warn
              : Tone.lit,
        ),
        tile(
          'QUOTA FRESH',
          '$fresh',
          'OF ${quotas.length}',
          unknown > 0 ? Tone.warn : Tone.lit,
        ),
        _HubTile(status: hub, onRetry: onRetry, appVersion: versions.app),
      ],
    );
    return Panel(
      legend: 'STATUS',
      trailing: ListenableBuilder(
        listenable: feed,
        builder: (context, _) => LampLegend(
          key: const ValueKey('events-lamp'),
          beat: beat,
          label: switch (feed.state) {
            FeedState.connecting => 'EVENTS CONNECTING',
            FeedState.live => 'EVENTS LIVE',
            FeedState.retrying => 'EVENTS RETRY ${feed.attempt}',
          },
          tone: switch (feed.state) {
            FeedState.connecting => Tone.info,
            FeedState.live => Tone.ok,
            FeedState.retrying => Tone.alarm,
          },
        ),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          if (wide && hubCpu.length > 1)
            Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Expanded(child: tiles),
                const SizedBox(width: Tokens.gapXL),
                SizedBox(
                  width: 360,
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text('HUB CPU HISTORY', style: t.legend()),
                      const SizedBox(height: 8),
                      DotMatrixGraph(samples: hubCpu, rows: 10, pitch: 7),
                    ],
                  ),
                ),
              ],
            )
          else
            tiles,
          const SizedBox(height: 14),
          Wrap(
            spacing: 6,
            runSpacing: 6,
            children: [
              Annunciator(label: 'NODE DOWN', tone: Tone.alarm, lit: down > 0),
              Annunciator(label: 'STALE', tone: Tone.warn, lit: stale > 0),
              Annunciator(
                label: 'DISK',
                tone: Tone.alarm,
                lit: badges.any((b) => b.label.startsWith('DISK')),
              ),
              Annunciator(
                label: 'MEM',
                tone: Tone.alarm,
                lit: badges.any((b) => b.label.startsWith('MEM')),
              ),
              Annunciator(
                label: 'QUOTA UNKNOWN',
                tone: Tone.alarm,
                lit: unknown > 0,
              ),
              Tooltip(
                message: skewMessage(skew, versions),
                child: Annunciator(
                  key: const ValueKey('skew-annunciator'),
                  label: 'VERSION SKEW',
                  tone: Tone.warn,
                  lit: skew.any,
                ),
              ),
              ListenableBuilder(
                listenable: feed,
                builder: (context, _) => Annunciator(
                  label: 'EVENTS',
                  tone: Tone.alarm,
                  lit: feed.state == FeedState.retrying,
                ),
              ),
            ],
          ),
          ListenableBuilder(
            listenable: feed,
            builder: (context, _) =>
                feed.state == FeedState.retrying && feed.lastError != null
                ? Padding(
                    padding: const EdgeInsets.only(top: 8),
                    child: Text('${feed.lastError}', style: t.small(t.p.alarm)),
                  )
                : const SizedBox.shrink(),
          ),
        ],
      ),
    );
  }
}

class _Cell extends StatelessWidget {
  const _Cell(
    this.label,
    this.percent,
    this.value, {
    this.dim = false,
    this.wide = false,
  });

  final String label;
  final double? percent;
  final String value;
  final bool dim;
  final bool wide;

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    final level = t.levelTone(percent);
    return Row(
      children: [
        if (!wide) SizedBox(width: 38, child: Text(label, style: t.legend())),
        Expanded(
          child: SegmentMeter(
            percent: percent,
            segments: wide ? 20 : 12,
            dim: dim,
          ),
        ),
        SizedBox(
          width: 48,
          child: Text(
            value,
            textAlign: TextAlign.end,
            style: t.data(
              dim
                  ? t.p.dim
                  : level == Tone.lit || level == Tone.dim
                  ? t.p.emph
                  : t.tone(level),
            ),
          ),
        ),
      ],
    );
  }
}

class _Board extends StatelessWidget {
  const _Board({
    required this.nodes,
    required this.loaded,
    required this.error,
    required this.ageOf,
    required this.beats,
    required this.wide,
    required this.onOpen,
    required this.hubVersion,
  });

  final String hubVersion;
  final List<NodeStatus> nodes;
  final bool loaded;
  final Object? error;
  final Duration Function(NodeStatus) ageOf;
  final Map<String, int> beats;
  final bool wide;
  final void Function(NodeStatus) onOpen;

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    if (!loaded) return const BusyBar();
    if (nodes.isEmpty) {
      return Padding(
        padding: const EdgeInsets.all(8),
        child: error != null
            ? Text('Nodes unavailable: $error', style: t.small(t.p.alarm))
            : Text('NO NODES', style: t.legend()),
      );
    }
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        if (error != null)
          Padding(
            padding: const EdgeInsets.all(8),
            child: Text(
              'Showing the last list. Refresh failed: $error',
              style: t.small(t.p.warn),
            ),
          ),
        if (wide)
          Padding(
            padding: const EdgeInsets.fromLTRB(10, 6, 10, 4),
            child: Row(
              children: [
                SizedBox(width: 128, child: Text('NODE', style: t.legend())),
                const SizedBox(width: 12),
                for (final (i, h) in ['CPU', 'GPU', 'MEM', 'DISK'].indexed) ...[
                  if (i > 0) const SizedBox(width: 14),
                  Expanded(child: Text(h, style: t.legend())),
                ],
              ],
            ),
          ),
        for (var i = 0; i < nodes.length; i++) _row(context, t, i, nodes[i]),
      ],
    );
  }

  Widget _row(BuildContext context, Tokens t, int i, NodeStatus s) {
    final f = s.freshness;
    final down = f == NodeFreshness.NODE_FRESHNESS_UNREACHABLE;
    final stale = f == NodeFreshness.NODE_FRESHNESS_STALE;
    final m = s.hasMetrics() ? s.metrics : null;
    final age = ageOf(s);
    final gpu = m == null ? null : summarizeGpus(m);
    final disk = m == null ? null : primaryFilesystem(m);
    String pct(double? v) => v == null ? '--' : '${v.round()}%';
    final cells = m == null || down
        ? <Widget>[]
        : [
            _Cell(
              'CPU',
              m.cpu.usagePercent,
              pct(m.cpu.usagePercent),
              dim: stale,
              wide: wide,
            ),
            _Cell(
              'GPU',
              gpu?.utilPercent,
              pct(gpu?.utilPercent),
              dim: stale,
              wide: wide,
            ),
            _Cell(
              'MEM',
              m.memory.usedPercent,
              pct(m.memory.usedPercent),
              dim: stale,
              wide: wide,
            ),
            _Cell(
              'DISK',
              disk?.usedPercent,
              pct(disk?.usedPercent),
              dim: stale,
              wide: wide,
            ),
          ];
    final name = Row(
      children: [
        Lamp(tone: freshnessTone(f), beat: beats[s.node.name]),
        const SizedBox(width: 10),
        Flexible(
          child: Text(
            s.node.name,
            overflow: TextOverflow.ellipsis,
            style: t
                .data(down ? t.p.alarm : t.p.emph)
                .copyWith(fontWeight: FontWeight.w700),
          ),
        ),
        if (s.node.local) ...[
          const SizedBox(width: 6),
          Text('HUB', style: t.legend(t.p.info)),
        ],
      ],
    );
    final note = down
        ? Text(
            [
              m == null ? 'never reached' : 'last seen ${formatAge(age)}',
              if (s.error.isNotEmpty) s.error,
            ].join('  '),
            style: t.small(t.p.alarm),
            maxLines: 2,
            overflow: TextOverflow.ellipsis,
          )
        : m == null
        ? Text('NO METRICS YET', style: t.legend())
        : null;
    final state = Text(
      freshnessLabel(f, age),
      style: t.legend(t.tone(freshnessTone(f))),
      textAlign: TextAlign.end,
    );
    final Widget body;
    if (wide) {
      body = Row(
        children: [
          SizedBox(
            width: 128,
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                name,
                const SizedBox(height: 3),
                Padding(
                  padding: const EdgeInsets.only(left: 20),
                  child: Text(
                    freshnessLabel(f, age),
                    style: t.legend(t.tone(freshnessTone(f))),
                  ),
                ),
              ],
            ),
          ),
          const SizedBox(width: 12),
          if (note != null)
            Expanded(child: note)
          else
            for (var c = 0; c < cells.length; c++) ...[
              if (c > 0) const SizedBox(width: 14),
              Expanded(child: cells[c]),
            ],
        ],
      );
    } else {
      body = Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            children: [
              Expanded(child: name),
              state,
            ],
          ),
          const SizedBox(height: 8),
          if (note != null)
            note
          else ...[
            Row(
              children: [
                Expanded(child: cells[0]),
                const SizedBox(width: 16),
                Expanded(child: cells[1]),
              ],
            ),
            const SizedBox(height: 6),
            Row(
              children: [
                Expanded(child: cells[2]),
                const SizedBox(width: 16),
                Expanded(child: cells[3]),
              ],
            ),
          ],
        ],
      );
    }
    final skewed = nodeSkewed(s, hubVersion);
    return InkWell(
      key: ValueKey('board-${s.node.name}'),
      borderRadius: BorderRadius.circular(Tokens.innerRadius),
      onTap: () => onOpen(s),
      child: BarRow(
        index: i,
        padding: EdgeInsets.symmetric(horizontal: 10, vertical: wide ? 10 : 12),
        child: skewed
            ? Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  body,
                  const SizedBox(height: 8),
                  Align(
                    alignment: Alignment.centerLeft,
                    child: SkewBadge(
                      key: ValueKey('skew-${s.node.name}'),
                      nodeVersion: nodeVersion(s),
                      hubVersion: hubVersion,
                    ),
                  ),
                ],
              )
            : body,
      ),
    );
  }
}

class _QuotaSummary extends StatelessWidget {
  const _QuotaSummary({
    required this.quotas,
    required this.error,
    required this.now,
  });

  final List<ProviderQuota> quotas;
  final Object? error;
  final DateTime now;

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    if (quotas.isEmpty) {
      return Text(
        error != null ? 'Quota unavailable: $error' : 'NO PROVIDERS',
        style: error != null ? t.small(t.p.alarm) : t.legend(),
      );
    }
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        for (var i = 0; i < quotas.length; i++) ...[
          if (i > 0) Divider(height: 20, color: t.p.rule),
          _provider(t, quotas[i]),
        ],
      ],
    );
  }

  Widget _provider(Tokens t, ProviderQuota q) {
    final unknown = quotaUnknown(q);
    final stale = q.freshness == Freshness.FRESHNESS_STALE;
    return Column(
      key: ValueKey('summary-${q.provider}'),
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Row(
          children: [
            Expanded(
              child: Text(
                [q.provider, if (q.plan.isNotEmpty) q.plan].join('  '),
                style: t.data(t.p.emph).copyWith(fontWeight: FontWeight.w700),
              ),
            ),
            LampLegend(label: quotaBadge(q), tone: quotaTone(q)),
          ],
        ),
        const SizedBox(height: 6),
        if (unknown)
          Row(
            children: [
              const Expanded(child: SegmentMeter(percent: null, height: 8)),
              const SizedBox(width: 10),
              Text('NO DATA', style: t.legend(t.p.alarm)),
            ],
          )
        else
          for (final w in q.windows)
            Semantics(
              label: windowSemantics(w, now),
              container: true,
              excludeSemantics: true,
              child: Padding(
                padding: const EdgeInsets.symmetric(vertical: 3),
                child: ResetSoonFrame(
                  on: resetsSoon(w, now),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      Row(
                        children: [
                          Expanded(
                            child: Text(
                              w.name.toUpperCase(),
                              style: t.legend(),
                            ),
                          ),
                          Text(
                            w.hasResetsAt()
                                ? formatCountdown(
                                    w.resetsAt.toDateTime().difference(now),
                                  )
                                : '?',
                            style: t.small(
                              resetsSoon(w, now) ? t.p.warn : t.p.info,
                            ),
                          ),
                        ],
                      ),
                      const SizedBox(height: 3),
                      Row(
                        children: [
                          Expanded(
                            child: SegmentMeter(
                              percent: w.usedPercent,
                              height: 8,
                              dim: stale,
                            ),
                          ),
                          SizedBox(
                            width: 48,
                            child: Text(
                              '${w.usedPercent.round()}%',
                              textAlign: TextAlign.end,
                              style: t.data(
                                stale
                                    ? t.p.dim
                                    : t.levelTone(w.usedPercent) == Tone.lit
                                    ? t.p.emph
                                    : t.level(w.usedPercent),
                              ),
                            ),
                          ),
                        ],
                      ),
                    ],
                  ),
                ),
              ),
            ),
      ],
    );
  }
}

class _HubTile extends StatelessWidget {
  const _HubTile({
    required this.status,
    required this.onRetry,
    required this.appVersion,
  });

  final Future<GetStatusResponse> status;
  final VoidCallback onRetry;
  final String appVersion;

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    return FutureBuilder<GetStatusResponse>(
      future: status,
      builder: (context, snap) {
        if (snap.connectionState != ConnectionState.done) {
          return const SizedBox(width: 120, child: BusyBar());
        }
        if (snap.hasError) {
          return ConstrainedBox(
            constraints: const BoxConstraints(maxWidth: 320),
            child: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                Flexible(
                  child: Text('${snap.error}', style: t.small(t.p.alarm)),
                ),
                TextButton(onPressed: onRetry, child: const Text('RETRY')),
              ],
            ),
          );
        }
        final s = snap.requireData;
        return Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          mainAxisSize: MainAxisSize.min,
          children: [
            ReadoutTile(
              legend: 'HUB UPTIME',
              readout: SegmentReadout.parts(
                parts: countdownParts(
                  Duration(seconds: s.uptimeSeconds.toInt()),
                ),
                size: Tokens.readoutL,
              ),
            ),
            const SizedBox(height: 4),
            Text('${s.hostname} (${s.os}/${s.arch})', style: t.small(t.p.lit)),
            Text('version ${s.version}', style: t.small()),
            if (appVersion.isNotEmpty)
              Text('app $appVersion', style: t.small()),
          ],
        );
      },
    );
  }
}

class _EventLog extends StatelessWidget {
  const _EventLog({required this.feed});

  final EventFeed feed;

  static const _shown = 24;

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    return Panel(
      legend: 'EVENT LOG',
      padding: const EdgeInsets.all(6),
      child: ListenableBuilder(
        listenable: feed,
        builder: (context, _) {
          final events = feed.events
              .where(
                (e) =>
                    e.kind != heartbeatKind && !nodeEventKinds.contains(e.kind),
              )
              .take(_shown)
              .toList();
          if (events.isEmpty) {
            return Padding(
              padding: const EdgeInsets.all(6),
              child: Text('NO EVENTS YET', style: t.legend()),
            );
          }
          return Column(
            children: [
              for (var i = 0; i < events.length; i++)
                BarRow(
                  index: i,
                  child: Row(
                    children: [
                      SizedBox(
                        width: 72,
                        child: Text(
                          events[i].hasTime()
                              ? events[i].time
                                    .toDateTime()
                                    .toLocal()
                                    .toString()
                                    .substring(11, 19)
                              : '',
                          style: t.small(),
                        ),
                      ),
                      Expanded(
                        child: Text(
                          events[i].kind,
                          style: t.data(t.p.lit),
                          overflow: TextOverflow.ellipsis,
                        ),
                      ),
                      const SizedBox(width: 8),
                      Flexible(
                        child: Text(
                          events[i].subject,
                          style: t.data(t.p.emph),
                          overflow: TextOverflow.ellipsis,
                        ),
                      ),
                    ],
                  ),
                ),
            ],
          );
        },
      ),
    );
  }
}
