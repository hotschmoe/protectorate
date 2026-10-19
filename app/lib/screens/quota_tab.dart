import 'dart:async';

import 'package:flutter/material.dart';

import '../gen/protectorate/v1/events.pb.dart';
import '../gen/protectorate/v1/quota.pb.dart';
import '../hubs/event_feed.dart';
import '../hubs/hub_client.dart';
import '../theme/tokens.dart';
import '../vfd/lamp.dart';
import '../vfd/meter.dart';
import '../vfd/panel.dart';

const quotaEventKind = 'quota.updated';

ProviderQuota? quotaFromEvent(Event event) {
  if (event.kind != quotaEventKind || !event.hasPayload()) return null;
  try {
    return ProviderQuota()..mergeFromProto3Json(event.payload.toProto3Json());
  } on Object {
    return null;
  }
}

String formatCountdown(Duration d) {
  if (d.isNegative) return 'reset due';
  if (d.inDays > 0) return '${d.inDays}d ${d.inHours % 24}h';
  if (d.inHours > 0) return '${d.inHours}h ${d.inMinutes % 60}m';
  return '${d.inMinutes}m';
}

class QuotaTab extends StatefulWidget {
  const QuotaTab({
    super.key,
    required this.client,
    required this.feed,
    this.now,
  });

  final HubClient client;
  final EventFeed feed;
  final DateTime Function()? now;

  @override
  State<QuotaTab> createState() => _QuotaTabState();
}

class _QuotaTabState extends State<QuotaTab> {
  final Map<String, ProviderQuota> _quotas = {};
  Object? _error;
  bool _loading = true;
  StreamSubscription<Event>? _sub;
  Timer? _tick;

  DateTime _now() => (widget.now ?? DateTime.now)();

  @override
  void initState() {
    super.initState();
    _sub = widget.feed.incoming.listen((e) {
      final q = quotaFromEvent(e);
      if (q != null) setState(() => _quotas[q.provider] = q);
    });
    _tick = Timer.periodic(const Duration(seconds: 30), (_) => setState(() {}));
    _load(false);
  }

  Future<void> _load(bool refresh) async {
    setState(() => _loading = true);
    try {
      final r = await widget.client.quota.getQuota(
        GetQuotaRequest(refresh: refresh),
      );
      if (!mounted) return;
      setState(() {
        for (final q in r.quotas) {
          _quotas[q.provider] = q;
        }
        _error = null;
      });
    } on Object catch (e) {
      if (mounted) setState(() => _error = e);
    } finally {
      if (mounted) setState(() => _loading = false);
    }
  }

  @override
  void dispose() {
    _sub?.cancel();
    _tick?.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    final providers = _quotas.keys.toList()..sort();
    return Column(
      children: [
        if (_loading) const BusyBar(),
        if (_error != null)
          Padding(
            padding: const EdgeInsets.fromLTRB(12, 12, 12, 0),
            child: NoteStrip('$_error', tone: Tone.alarm),
          ),
        Expanded(
          child: RefreshIndicator(
            onRefresh: () => _load(true),
            child: LayoutBuilder(
              builder: (context, box) {
                const pad = Tokens.gap;
                final cols = (box.maxWidth / 460).floor().clamp(1, 4);
                final w = (box.maxWidth - 2 * pad - (cols - 1) * pad) / cols;
                return SingleChildScrollView(
                  physics: const AlwaysScrollableScrollPhysics(),
                  padding: const EdgeInsets.all(pad),
                  child: providers.isEmpty && !_loading
                      ? Center(child: Text('NO PROVIDERS', style: t.legend()))
                      : Wrap(
                          spacing: pad,
                          runSpacing: pad,
                          children: [
                            for (final p in providers)
                              SizedBox(
                                width: w,
                                child: ProviderCard(
                                  quota: _quotas[p]!,
                                  now: _now(),
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

List<(String, String)> countdownParts(Duration d) {
  if (d.isNegative) return [('--', 'DUE')];
  String two(int n) => n.toString().padLeft(2, '0');
  if (d.inDays > 0) return [('${d.inDays}', 'D'), (two(d.inHours % 24), 'H')];
  if (d.inHours > 0) {
    return [('${d.inHours}', 'H'), (two(d.inMinutes % 60), 'M')];
  }
  return [('${d.inMinutes}', 'M')];
}

bool quotaUnknown(ProviderQuota q) =>
    q.freshness != Freshness.FRESHNESS_FRESH &&
    q.freshness != Freshness.FRESHNESS_STALE;

Tone quotaTone(ProviderQuota q) => switch (q.freshness) {
  Freshness.FRESHNESS_FRESH => Tone.ok,
  Freshness.FRESHNESS_STALE => Tone.warn,
  _ => Tone.alarm,
};

String quotaBadge(ProviderQuota q) {
  switch (q.freshness) {
    case Freshness.FRESHNESS_FRESH:
      return 'FRESH';
    case Freshness.FRESHNESS_STALE:
      final secs = q.age.seconds.toInt();
      return secs == 0
          ? 'STALE'
          : 'STALE ${formatCountdown(Duration(seconds: secs))}';
    default:
      return 'UNKNOWN';
  }
}

// A window longer than a day that resets within the next day: its unused share
// is about to expire. Shorter windows reset all the time and are left out.
bool resetsSoon(QuotaWindow w, DateTime now) {
  if (!w.hasResetsAt()) return false;
  final long = w.windowSeconds > 0
      ? w.windowSeconds > const Duration(days: 1).inSeconds
      : !w.name.toLowerCase().endsWith('h');
  final left = w.resetsAt.toDateTime().difference(now);
  return long && !left.isNegative && left <= const Duration(days: 1);
}

class ResetSoonFrame extends StatelessWidget {
  const ResetSoonFrame({super.key, required this.on, required this.child});

  final bool on;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    if (!on) return child;
    final t = Tokens.of(context);
    return Container(
      key: const ValueKey('reset-soon'),
      padding: const EdgeInsets.fromLTRB(8, 4, 8, 6),
      decoration: BoxDecoration(
        border: Border.all(color: t.p.warn, width: 1.2),
        borderRadius: BorderRadius.circular(Tokens.innerRadius),
      ),
      child: child,
    );
  }
}

String windowSemantics(QuotaWindow w, DateTime now) =>
    '${w.name}: ${w.usedPercent.toStringAsFixed(0)}% used'
    '${w.hasResetsAt() ? ', resets in ${formatCountdown(w.resetsAt.toDateTime().difference(now))}' : ''}'
    '${resetsSoon(w, now) ? ', resets within a day' : ''}';

class ProviderCard extends StatelessWidget {
  const ProviderCard({super.key, required this.quota, required this.now});

  final ProviderQuota quota;
  final DateTime now;

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    final unknown = quotaUnknown(quota);
    final stale = quota.freshness == Freshness.FRESHNESS_STALE;
    return Panel(
      key: ValueKey('provider-${quota.provider}'),
      legend: [
        quota.provider.toUpperCase(),
        if (quota.plan.isNotEmpty) quota.plan.toUpperCase(),
      ].join('  '),
      tone: unknown ? Tone.alarm : null,
      trailing: LampLegend(
        key: ValueKey('freshness-${quota.provider}'),
        label: quotaBadge(quota),
        tone: quotaTone(quota),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          if (quota.error.isNotEmpty)
            Padding(
              padding: const EdgeInsets.only(bottom: 8),
              child: Text(quota.error, style: t.data(t.p.alarm)),
            ),
          if (unknown) ...[
            const SegmentMeter(percent: null),
            const SizedBox(height: 10),
            Text(
              'No data. This is not spare capacity.',
              style: t.data(t.p.alarm),
            ),
          ] else ...[
            if (stale)
              Padding(
                padding: const EdgeInsets.only(bottom: 4),
                child: Text(
                  'Stale data; may not reflect current usage.',
                  style: t.small(t.p.warn),
                ),
              ),
            for (final w in quota.windows) _window(t, w, stale),
            for (final r in quota.resets)
              Padding(
                padding: const EdgeInsets.only(top: 8),
                child: Row(
                  children: [
                    Annunciator(label: 'RESET x${r.count}', tone: Tone.info),
                    const SizedBox(width: 8),
                    Expanded(
                      child: Text(
                        '${r.kind}${r.hasExpiresAt() ? ', expires in ${formatCountdown(r.expiresAt.toDateTime().difference(now))}' : ''}',
                        style: t.small(),
                      ),
                    ),
                  ],
                ),
              ),
          ],
        ],
      ),
    );
  }

  Widget _window(Tokens t, QuotaWindow w, bool stale) {
    final pct = w.usedPercent;
    final level = t.levelTone(pct);
    final color = stale
        ? t.p.dim
        : level == Tone.lit
        ? t.p.emph
        : t.tone(level);
    return Semantics(
      label: windowSemantics(w, now),
      container: true,
      excludeSemantics: true,
      child: Padding(
        padding: const EdgeInsets.symmetric(vertical: 8),
        child: ResetSoonFrame(
          on: resetsSoon(w, now),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Row(
                crossAxisAlignment: CrossAxisAlignment.end,
                children: [
                  Expanded(
                    child: ReadoutTile(
                      legend: '${w.name.toUpperCase()} WINDOW',
                      readout: SegmentReadout(
                        value: pct.toStringAsFixed(0),
                        unit: '% USED',
                        size: Tokens.readoutL,
                        color: color,
                      ),
                    ),
                  ),
                  Column(
                    crossAxisAlignment: CrossAxisAlignment.end,
                    children: [
                      Text('RESETS IN', style: t.legend()),
                      const SizedBox(height: 6),
                      w.hasResetsAt()
                          ? SegmentReadout.parts(
                              parts: countdownParts(
                                w.resetsAt.toDateTime().difference(now),
                              ),
                              size: Tokens.readoutM,
                              color: stale
                                  ? t.p.dim
                                  : resetsSoon(w, now)
                                  ? t.p.warn
                                  : t.p.info,
                            )
                          : Text('UNKNOWN', style: t.legend(t.p.warn)),
                    ],
                  ),
                ],
              ),
              const SizedBox(height: 10),
              SegmentMeter(
                key: ValueKey('bar-${w.name}'),
                percent: pct,
                dim: stale,
                height: 12,
              ),
              if (w.hasEstimatedUnusedPercentAtReset())
                Padding(
                  padding: const EdgeInsets.only(top: 6),
                  child: Text(
                    '~${w.estimatedUnusedPercentAtReset.toStringAsFixed(0)}% unused at reset (est.)',
                    style: t.small(),
                  ),
                ),
            ],
          ),
        ),
      ),
    );
  }
}
