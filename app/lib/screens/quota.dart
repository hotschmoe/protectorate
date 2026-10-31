import 'package:flutter/material.dart';

import '../model.dart';
import '../scope.dart';
import '../theme.dart';
import '../widgets.dart';

(String, Color) readingWord(Palette p, QuotaReading r) => switch (r) {
  QuotaReading.ok => ('OK', p.ok),
  QuotaReading.stale => ('STALE', p.warn),
  QuotaReading.error => ('ERROR', p.alarm),
  QuotaReading.unknown => ('UNKNOWN', p.dim),
};

class QuotaScreen extends StatelessWidget {
  const QuotaScreen({super.key});

  @override
  Widget build(BuildContext context) {
    final app = AppScope.of(context);
    final p = pal(context);
    final phone = Term.of(context).phone;
    final qs = app.state.quotas;
    if (qs.isEmpty) return T('(none)', color: p.dim);
    return Tick(builder: (c, now) => ListView(children: [
      for (final q in qs) ...[
        Row(children: [
          Cells(9, child: T(q.subscription.name.toUpperCase(), color: p.lit, bold: true)),
          Expanded(child: T(q.plan, color: p.fg)),
          T('fetched ${span(now.difference(q.fetchedAt))} ago  ', color: p.dim),
          T(readingWord(p, q.reading).$1, color: readingWord(p, q.reading).$2),
        ]),
        for (final w in q.windows)
          Row(children: [
            const Cells(2, child: SizedBox()),
            Cells(7, child: T(w.label, color: p.dim)),
            Cells(phone ? 15 : 23, child: Meter(w.usedPct == null ? null : w.usedPct! / 100,
                cells: phone ? 8 : 16)),
            Expanded(child: T(
              w.resetsAt == null
                  ? ''
                  : phone
                  ? 'r ${span(w.resetsAt!.difference(now))}'
                  : 'resets in ${span(w.resetsAt!.difference(now))}  (${_day(w.resetsAt!, now)} ${hhmm(w.resetsAt!)})',
              color: p.dim,
            )),
          ]),
        if (q.note.isNotEmpty)
          Padding(
            padding: const EdgeInsets.only(left: kCell * 2),
            child: T(q.note, color: q.reading == QuotaReading.ok ? p.dim : p.warn, maxLines: null),
          ),
        const SizedBox(height: kRow / 2),
      ],
      const HRule(),
      T('Readers are read-only. Dispatch weighs projected unused quota at reset and falls back to '
          'the local model (SPEC 8).', color: p.dim, maxLines: null),
    ]));
  }

  static String _day(DateTime t, DateTime now) {
    const days = ['MON', 'TUE', 'WED', 'THU', 'FRI', 'SAT', 'SUN'];
    final d = DateTime(t.year, t.month, t.day).difference(DateTime(now.year, now.month, now.day)).inDays;
    return d == 0 ? 'today' : days[t.weekday - 1];
  }
}

/// The compact QUOTA pane in the desktop left column.
class QuotaMini extends StatelessWidget {
  const QuotaMini({super.key});

  @override
  Widget build(BuildContext context) {
    final app = AppScope.of(context);
    final p = pal(context);
    return Tick(builder: (c, now) => Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        for (final q in app.state.quotas)
          Row(children: [
            Cells(8, child: T(q.subscription.name, color: p.fg)),
            Cells(4, child: T(q.tightest?.label ?? '-', color: p.dim)),
            Cells(15, child: Meter(q.tightest?.usedPct == null ? null : q.tightest!.usedPct! / 100,
                cells: 8)),
            Expanded(child: T(
              q.tightest?.resetsAt == null ? '' : 'r ${span(q.tightest!.resetsAt!.difference(now))}',
              color: q.reading == QuotaReading.stale ? p.warn : p.dim,
            )),
          ]),
      ],
    ));
  }
}
