import 'dart:async';

import 'package:flutter/material.dart';

import '../gen/protectorate/v1/events.pb.dart';
import '../gen/protectorate/v1/hub.pb.dart';
import '../gen/protectorate/v1/sleeve.pb.dart';
import '../gen/protectorate/v1/quota.pb.dart';
import '../hubs/event_feed.dart';
import '../hubs/hub_client.dart';
import '../theme/tokens.dart';
import '../updates/hub_update_card.dart';
import '../version.dart';
import '../vfd/lamp.dart';
import '../vfd/meter.dart';
import '../vfd/panel.dart';
import 'quota_tab.dart';

class OverviewTab extends StatefulWidget {
  const OverviewTab({
    super.key,
    required this.client,
    required this.feed,
    this.now,
  });

  final HubClient client;
  final EventFeed feed;
  final DateTime Function()? now;

  @override
  State<OverviewTab> createState() => _OverviewTabState();
}

class _OverviewTabState extends State<OverviewTab> {
  late Future<GetStatusResponse> _status = _loadStatus();
  List<Sleeve> _sleeves = const [];
  final Map<String, ProviderQuota> _quotas = {};
  Object? _quotaError;
  StreamSubscription<Event>? _sub;
  int _hubBeat = 0;
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
      if (e.kind.startsWith('sleeve.')) {
        _debounce?.cancel();
        _debounce = Timer(const Duration(milliseconds: 300), _loadSleeves);
      }
    });
    _poll = Timer.periodic(const Duration(seconds: 30), (_) => _loadSleeves());
    _loadSleeves();
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

  Future<void> _loadSleeves() async {
    try {
      final r = await widget.client.sleeves.listSleeves(ListSleevesRequest());
      if (mounted) setState(() => _sleeves = r.sleeves);
    } on Object {
      // The tile keeps its last count.
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

  @override
  Widget build(BuildContext context) {
    final quotas = [for (final k in _quotas.keys.toList()..sort()) _quotas[k]!];
    return LayoutBuilder(
      builder: (context, box) {
        final wide = box.maxWidth >= 1100;
        final status = _StatusPanel(
          sleeves: _sleeves,
          quotas: quotas,
          feed: _feed,
          hub: _status,
          onRetry: () => setState(() => _status = _loadStatus()),
          beat: _hubBeat,
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
                      HubUpdateCard(client: widget.client, feed: widget.feed),
                      Row(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Expanded(flex: 3, child: quota),
                          const SizedBox(width: Tokens.gapL),
                          Expanded(flex: 2, child: events),
                        ],
                      ),
                    ])
                  : column([
                      status,
                      HubUpdateCard(client: widget.client, feed: widget.feed),
                      quota,
                      events,
                    ]),
            ),
          ),
        );
      },
    );
  }
}

class _StatusPanel extends StatelessWidget {
  const _StatusPanel({
    required this.sleeves,
    required this.quotas,
    required this.feed,
    required this.hub,
    required this.onRetry,
    required this.beat,
  });

  final List<Sleeve> sleeves;
  final List<ProviderQuota> quotas;
  final EventFeed feed;
  final Future<GetStatusResponse> hub;
  final VoidCallback onRetry;
  final int beat;

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    final running = sleeves
        .where((s) => s.state == SleeveState.SLEEVE_STATE_RUNNING)
        .length;
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
        tile('SLEEVES RUNNING', '$running', 'OF ${sleeves.length}', Tone.lit),
        tile(
          'QUOTA FRESH',
          '$fresh',
          'OF ${quotas.length}',
          unknown > 0 ? Tone.warn : Tone.lit,
        ),
        _HubTile(status: hub, onRetry: onRetry),
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
          tiles,
          const SizedBox(height: 14),
          Wrap(
            spacing: 6,
            runSpacing: 6,
            children: [
              Annunciator(
                label: 'QUOTA UNKNOWN',
                tone: Tone.alarm,
                lit: unknown > 0,
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
  const _HubTile({required this.status, required this.onRetry});

  final Future<GetStatusResponse> status;
  final VoidCallback onRetry;

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
              .where((e) => e.kind != heartbeatKind)
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
