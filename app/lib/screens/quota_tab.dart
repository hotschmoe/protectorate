import 'dart:async';

import 'package:flutter/material.dart';

import '../gen/protectorate/v1/events.pb.dart';
import '../gen/protectorate/v1/quota.pb.dart';
import '../hubs/event_feed.dart';
import '../hubs/hub_client.dart';

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
    final providers = _quotas.keys.toList()..sort();
    return Column(
      children: [
        if (_loading) const LinearProgressIndicator(),
        if (_error != null)
          ListTile(
            leading: const Icon(Icons.error_outline),
            title: Text('$_error'),
          ),
        Expanded(
          child: RefreshIndicator(
            onRefresh: () => _load(true),
            child: ListView(
              physics: const AlwaysScrollableScrollPhysics(),
              padding: const EdgeInsets.all(8),
              children: [
                for (final p in providers)
                  ProviderCard(quota: _quotas[p]!, now: _now()),
              ],
            ),
          ),
        ),
      ],
    );
  }
}

class ProviderCard extends StatelessWidget {
  const ProviderCard({super.key, required this.quota, required this.now});

  final ProviderQuota quota;
  final DateTime now;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    final unknown =
        quota.freshness != Freshness.FRESHNESS_FRESH &&
        quota.freshness != Freshness.FRESHNESS_STALE;
    final stale = quota.freshness == Freshness.FRESHNESS_STALE;
    return Card(
      key: ValueKey('provider-${quota.provider}'),
      child: Padding(
        padding: const EdgeInsets.all(12),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                Expanded(
                  child: Text(
                    quota.plan.isEmpty
                        ? quota.provider
                        : '${quota.provider} (${quota.plan})',
                    style: Theme.of(context).textTheme.titleMedium,
                  ),
                ),
                Chip(
                  key: ValueKey('freshness-${quota.provider}'),
                  label: Text(_badge()),
                  backgroundColor: unknown
                      ? scheme.errorContainer
                      : stale
                      ? scheme.tertiaryContainer
                      : scheme.secondaryContainer,
                ),
              ],
            ),
            if (quota.error.isNotEmpty)
              Text(quota.error, style: TextStyle(color: scheme.error)),
            if (unknown)
              const Text('No data. This is not spare capacity.')
            else ...[
              if (stale)
                const Text('Stale data; may not reflect current usage.'),
              for (final w in quota.windows) _window(context, w, stale),
              for (final r in quota.resets)
                Text(
                  '${r.kind}: ${r.count} reset credit(s)'
                  '${r.hasExpiresAt() ? ', expires in ${formatCountdown(r.expiresAt.toDateTime().difference(now))}' : ''}',
                ),
            ],
          ],
        ),
      ),
    );
  }

  String _badge() {
    switch (quota.freshness) {
      case Freshness.FRESHNESS_FRESH:
        return 'fresh';
      case Freshness.FRESHNESS_STALE:
        final secs = quota.age.seconds.toInt();
        return secs == 0
            ? 'stale'
            : 'stale ${formatCountdown(Duration(seconds: secs))}';
      default:
        return 'unknown';
    }
  }

  Widget _window(BuildContext context, QuotaWindow w, bool stale) {
    final used = (w.usedPercent / 100).clamp(0.0, 1.0);
    final resets = w.hasResetsAt()
        ? 'resets in ${formatCountdown(w.resetsAt.toDateTime().difference(now))}'
        : 'reset unknown';
    final projected = w.hasEstimatedUnusedPercentAtReset()
        ? ', ~${w.estimatedUnusedPercentAtReset.toStringAsFixed(0)}% unused at reset (est.)'
        : '';
    return Padding(
      padding: const EdgeInsets.only(top: 8),
      child: Opacity(
        opacity: stale ? 0.55 : 1,
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text('${w.name}: ${w.usedPercent.toStringAsFixed(0)}% used'),
            const SizedBox(height: 4),
            LinearProgressIndicator(
              key: ValueKey('bar-${w.name}'),
              value: used,
            ),
            Text(
              '$resets$projected',
              style: Theme.of(context).textTheme.bodySmall,
            ),
          ],
        ),
      ),
    );
  }
}
