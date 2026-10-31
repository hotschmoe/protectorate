import 'dart:async';

import 'package:flutter/material.dart';

import '../model.dart';
import '../scope.dart';
import '../theme.dart';
import '../widgets.dart';

/// Mail and events, newest first. Opening the screen marks them read.
class EventsScreen extends StatefulWidget {
  const EventsScreen({super.key, this.limit, this.markRead = true});

  final int? limit;
  final bool markRead;

  @override
  State<EventsScreen> createState() => _EventsScreenState();
}

class _EventsScreenState extends State<EventsScreen> {
  EventKind? _filter;

  @override
  void initState() {
    super.initState();
    if (widget.markRead) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted) unawaited(AppScope.of(context).hub.markEventsRead());
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    final app = AppScope.of(context);
    final p = pal(context);
    final phone = Term.of(context).phone;
    var evs = app.state.events.reversed.where((e) => _filter == null || e.kind == _filter);
    if (widget.limit != null) evs = evs.take(widget.limit!);
    final list = evs.toList();
    final rows = ListView.builder(
      itemCount: list.isEmpty ? 1 : list.length,
      itemBuilder: (c, i) {
        if (list.isEmpty) return T('(none)', color: p.dim);
        final e = list[i];
        final color = switch (e.level) {
          Level.alarm => p.alarm,
          Level.warn => p.warn,
          Level.info => e.read ? p.fg : p.lit,
        };
        final who = e.target == null ? e.source : '${e.source} -> ${e.target}';
        if (phone) {
          return Padding(
            padding: const EdgeInsets.only(bottom: 4),
            child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
              Line([
                sp(context, '${hhmm(e.at)} ', color: p.rule),
                sp(context, pad(e.kind.name.toUpperCase(), 10), color: p.dim),
                sp(context, who, color: e.read ? p.dim : p.lit),
              ]),
              Padding(
                padding: const EdgeInsets.only(left: kCell * 6),
                child: T(e.text, color: color, maxLines: null),
              ),
            ]),
          );
        }
        return Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
          Cells(6, child: T(hhmm(e.at), color: p.rule)),
          Cells(2, child: T(e.read ? '' : '*', color: p.lit)),
          Cells(10, child: T(e.kind.name.toUpperCase(), color: p.dim)),
          Cells(28, child: T(who, color: e.read ? p.dim : p.fg)),
          Expanded(child: T(e.text, color: color, maxLines: null)),
        ]);
      },
    );
    if (widget.limit != null) return rows;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Actions3270([
          for (final k in const [null, EventKind.mail, EventKind.sleeve, EventKind.approval,
              EventKind.job, EventKind.node, EventKind.guardrail])
            Btn(k == null ? 'ALL' : k.name.toUpperCase(),
                active: _filter == k,
                color: p.dim,
                onTap: () => setState(() => _filter = k)),
        ]),
        const SizedBox(height: 4),
        Expanded(child: rows),
      ],
    );
  }
}
