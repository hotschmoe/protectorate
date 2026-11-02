import 'dart:async';

import 'package:flutter/material.dart';

import '../api/api.g.dart';
import '../icons.dart';
import '../scope.dart';
import '../theme.dart';
import '../widgets.dart';

/// Mail and events, newest first. Opening the screen marks them read. With a [limit] it is the
/// compact list beside Envoy's chat.
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

  Color _color(Palette p, HubEvent e) => switch (e.level) {
    Level.alarm => p.alarm,
    Level.warn => p.warn,
    Level.info || Level.unknown => e.read ? p.dim : p.fg,
  };

  @override
  Widget build(BuildContext context) {
    final app = AppScope.of(context);
    final p = pal(context);
    final phone = Look.of(context).phone;
    final compact = widget.limit != null;
    var evs = app.state.events.reversed.where((e) => _filter == null || e.kind == _filter);
    if (widget.limit != null) evs = evs.take(widget.limit!);
    final list = evs.toList();
    final Widget rows;
    if (list.isEmpty) {
      rows = compact
          ? const Padding(padding: EdgeInsets.symmetric(horizontal: 16), child: EmptyState('Nothing yet', compact: true))
          : const EmptyState('No events', glyph: Glyph.events);
    } else {
      rows = ListView.builder(
        padding: EdgeInsets.symmetric(horizontal: compact ? 8 : 0, vertical: compact ? 0 : 8),
        itemCount: list.length,
        itemBuilder: (c, i) {
          final e = list[i];
          final who = e.target == null ? e.source : '${e.source} → ${e.target}';
          final unread = Dot(e.read ? const Color(0x00000000) : p.accent, size: 6);
          if (compact || phone) {
            return Container(
              padding: EdgeInsets.symmetric(horizontal: compact ? 8 : 0, vertical: 7),
              decoration: compact
                  ? null
                  : BoxDecoration(border: Border(bottom: BorderSide(color: p.line.withValues(alpha: 0.6)))),
              child: Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
                Padding(padding: const EdgeInsets.only(top: 6), child: unread),
                const SizedBox(width: 8),
                Expanded(
                  child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
                    Row(children: [
                      Flexible(child: T(who, color: e.read ? p.dim : p.fg, size: kSmall,
                          weight: FontWeight.w500)),
                      const SizedBox(width: 6),
                      T(e.kind.wire, color: p.faint, size: 11),
                      const Spacer(),
                      T(hhmm(e.at), color: p.faint, size: 11),
                    ]),
                    const SizedBox(height: 2),
                    T(e.text, color: _color(p, e), maxLines: compact ? 2 : null, size: kSmall + 0.5),
                  ]),
                ),
              ]),
            );
          }
          return Container(
            padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 9),
            decoration: BoxDecoration(border: Border(bottom: BorderSide(color: p.line.withValues(alpha: 0.6)))),
            child: Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
              SizedBox(width: 50, child: T(hhmm(e.at), color: p.faint, size: kSmall)),
              Padding(padding: const EdgeInsets.only(top: 6, right: 10), child: unread),
              SizedBox(width: 90, child: Align(alignment: Alignment.centerLeft,
                  child: Pill(e.kind.wire, color: _kindColor(p, e.kind)))),
              SizedBox(width: 220, child: T(who, color: e.read ? p.dim : p.fg, weight: FontWeight.w500)),
              Expanded(child: T(e.text, color: _color(p, e), maxLines: null)),
            ]),
          );
        },
      );
    }
    if (compact) return rows;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Padding(
          padding: const EdgeInsets.only(top: 16, bottom: 8),
          child: Buttons(spacing: 6, [
            for (final k in const [null, EventKind.mail, EventKind.sleeve, EventKind.approval,
                EventKind.job, EventKind.node, EventKind.guardrail])
              Btn(k == null ? 'All' : cap(k.wire),
                  dense: true,
                  kind: BtnKind.ghost,
                  active: _filter == k,
                  onTap: () => setState(() => _filter = k)),
          ]),
        ),
        Expanded(child: rows),
      ],
    );
  }

  Color _kindColor(Palette p, EventKind k) => switch (k) {
    EventKind.mail => p.info,
    EventKind.approval => p.warn,
    EventKind.guardrail => p.alarm,
    _ => p.dim,
  };
}
