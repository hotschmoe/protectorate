import 'package:flutter/material.dart';

import '../api/api.g.dart';
import '../derived.dart';
import '../icons.dart';
import '../scope.dart';
import '../theme.dart';
import '../widgets.dart';

(String, Color) readingWord(Palette p, QuotaReading r) => switch (r) {
  QuotaReading.ok => ('Fresh', p.ok),
  QuotaReading.stale => ('Stale', p.warn),
  QuotaReading.error => ('Error', p.alarm),
  QuotaReading.unknown => ('Unknown', p.faint),
};

class QuotaScreen extends StatelessWidget {
  const QuotaScreen({super.key});

  @override
  Widget build(BuildContext context) {
    final app = AppScope.of(context);
    final p = pal(context);
    final phone = Look.of(context).phone;
    final qs = app.state.quotas;
    if (qs.isEmpty) return const EmptyState('No subscriptions', glyph: Glyph.quota);
    return Tick(builder: (c, now) => ListView(
      padding: const EdgeInsets.symmetric(vertical: 20),
      children: [
        Wrap(
          spacing: 12,
          runSpacing: 12,
          children: [
            for (final q in qs)
              SizedBox(
                width: phone ? double.infinity : 420,
                child: Panel(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      Row(children: [
                        T(cap(q.subscription.wire), weight: FontWeight.w600, size: kBody),
                        const SizedBox(width: 10),
                        Expanded(child: T(q.plan, color: p.dim)),
                        Pill(readingWord(p, q.reading).$1, color: readingWord(p, q.reading).$2, dot: true),
                      ]),
                      const SizedBox(height: 2),
                      T('Fetched ${span(now.difference(q.fetchedAt))} ago', color: p.faint, size: kSmall),
                      const SizedBox(height: 14),
                      if (q.windows.isEmpty && q.note.isEmpty)
                        const EmptyState('No windows reported', compact: true),
                      for (final w in q.subscription == Subscription.antigravity ? q.windows.take(6) : q.windows)
                        Padding(
                          padding: const EdgeInsets.only(bottom: 10),
                          child: Column(
                            crossAxisAlignment: CrossAxisAlignment.stretch,
                            children: [
                              T(w.label.startsWith('model ') ? w.label.substring(6) : w.label,
                                  color: p.dim, weight: FontWeight.w500, maxLines: null),
                              const SizedBox(height: 3),
                              Meter(w.usedPct == null ? null : w.usedPct! / 100, width: null),
                              if (w.resetsAt != null)
                                Padding(
                                  padding: const EdgeInsets.only(top: 3),
                                  child: T(
                                    'Resets in ${span(w.resetsAt!.difference(now))}  (${_day(w.resetsAt!, now)} ${hhmm(w.resetsAt!)})',
                                    color: p.faint,
                                    size: kSmall,
                                  ),
                                ),
                            ],
                          ),
                        ),
                      if (q.subscription == Subscription.antigravity && q.windows.length > 6)
                        T('+${q.windows.length - 6} more model${q.windows.length == 7 ? '' : 's'}',
                            color: p.faint, size: kSmall),
                      if (q.note.isNotEmpty)
                        T(q.note, color: q.reading == QuotaReading.ok ? p.dim : p.warn, maxLines: null,
                            size: kSmall),
                    ],
                  ),
                ),
              ),
          ],
        ),
        const SizedBox(height: 20),
        T('Readers are read-only. Antigravity usage is shown per model.',
            color: p.faint, maxLines: null, size: kSmall),
      ],
    ));
  }

  static String _day(DateTime t, DateTime now) {
    const days = ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun'];
    final d = DateTime(t.year, t.month, t.day).difference(DateTime(now.year, now.month, now.day)).inDays;
    return d == 0 ? 'today' : days[t.weekday - 1];
  }
}

/// Quota beside Envoy's chat: the tightest window of each subscription.
class QuotaMini extends StatelessWidget {
  const QuotaMini({super.key});

  @override
  Widget build(BuildContext context) {
    final app = AppScope.of(context);
    final p = pal(context);
    if (app.state.quotas.isEmpty) return const EmptyState('No subscriptions', compact: true);
    return Tick(builder: (c, now) => Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        for (final q in app.state.quotas)
          SizedBox(
            height: 26,
            child: Row(children: [
              Flexible(flex: 90, child: SizedBox(width: 90, child: T(cap(q.subscription.wire), color: p.dim))),
              Flexible(flex: 80, child: SizedBox(width: 80, child: T(q.tightest?.label.replaceFirst('model ', '') ?? '-', color: p.faint, size: kSmall))),
              Expanded(flex: 90, child: LayoutBuilder(builder: (context, constraints) =>
                  Meter(q.tightest?.usedPct == null ? null : q.tightest!.usedPct! / 100,
                      width: null, pct: constraints.maxWidth >= 40))),
              Flexible(flex: 64, child: SizedBox(
                width: 64,
                child: T(
                  q.tightest?.resetsAt == null ? '' : span(q.tightest!.resetsAt!.difference(now)),
                  color: q.reading == QuotaReading.stale ? p.warn : p.faint,
                  size: kSmall,
                  align: TextAlign.right,
                ),
              )),
            ]),
          ),
      ],
    ));
  }
}
