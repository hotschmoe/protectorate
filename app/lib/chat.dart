import 'dart:async';

import 'package:flutter/material.dart';

import 'api/api.g.dart';
import 'derived.dart';
import 'scope.dart';
import 'theme.dart';
import 'widgets.dart';

/// A sleeve's chat: status line, transcript, composer and actions. Envoy uses the same view; it
/// is a root sleeve like any other.
class SleeveView extends StatefulWidget {
  const SleeveView({super.key, required this.sleeveId, this.envoy = false});

  final String sleeveId;
  final bool envoy;

  @override
  State<SleeveView> createState() => _SleeveViewState();
}

enum _Pending { none, resleeve, stop }

class _SleeveViewState extends State<SleeveView> {
  final _text = TextEditingController();
  final _focus = FocusNode();
  _Pending _pending = _Pending.none;
  bool _takeover = false;

  @override
  void didUpdateWidget(SleeveView old) {
    super.didUpdateWidget(old);
    if (old.sleeveId != widget.sleeveId) {
      _pending = _Pending.none;
      _takeover = false;
    }
  }

  @override
  void dispose() {
    _text.dispose();
    _focus.dispose();
    super.dispose();
  }

  void _send(AppScope app) {
    final t = _text.text.trim();
    if (t.isEmpty) return;
    _text.clear();
    unawaited(app.act(() => app.hub.send(widget.sleeveId, t)));
    _focus.requestFocus();
  }

  @override
  Widget build(BuildContext context) {
    final app = AppScope.of(context);
    final p = pal(context);
    final phone = Term.of(context).phone;
    final s = app.state.sleeve(widget.sleeveId);
    if (s == null) {
      return Pane(legend: 'SLEEVE', child: T('(no sleeve ${widget.sleeveId})', color: p.dim));
    }
    final st = sleeveStatus(p, s);
    final legend = widget.envoy ? 'ENVOY' : 'SLEEVE ${s.name}';
    final status = Tick(
      builder: (c, now) => Line([
        statusSpan(c, st),
        if (s.incarnation != null)
          sp(c, ' ${clock(now.difference(s.incarnation!.activity.since))}', color: p.dim),
      ]),
    );
    if (_takeover) {
      return Pane(
        legend: 'TERMINAL ${s.name}',
        status: T('TAKEOVER', color: p.warn),
        focused: true,
        child: _TakeoverPlaceholder(
          sleeve: s,
          onBack: () => setState(() => _takeover = false),
        ),
      );
    }
    return Pane(
      legend: legend,
      status: status,
      focused: true,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          _StatusLine(sleeve: s),
          const HRule(),
          Expanded(child: TranscriptView(sleeveId: s.id, envoy: widget.envoy)),
          const HRule(label: 'INPUT'),
          Padding(
            padding: const EdgeInsets.only(bottom: 2),
            child: Field(
              controller: _text,
              focusNode: _focus,
              prompt: phone ? '> ' : 'owner> ',
              promptColor: p.lit,
              hint: widget.envoy ? 'talk to Envoy' : 'message ${s.name}',
              multiline: true,
              onSubmit: (_) => _send(app),
            ),
          ),
          _actions(app, s, p),
        ],
      ),
    );
  }

  Widget _actions(AppScope app, Sleeve s, Palette p) {
    switch (_pending) {
      case _Pending.resleeve:
        final order = [
          ...s.resleeveTo,
          for (final h in const [Harness.claude, Harness.codex, Harness.grok, Harness.piLocal])
            if (!s.resleeveTo.contains(h)) h,
        ];
        final current = s.incarnation?.harness;
        return Actions3270([
          T('RESLEEVE TO:', color: p.warn),
          for (final h in order)
            Btn(
              harnessName(h),
              color: h == current ? p.dim : p.lit,
              onTap: () {
                setState(() => _pending = _Pending.none);
                unawaited(app.act(() => app.hub.resleeve(s.id, h),
                    done: 'resleeving ${s.name} to ${harnessName(h)}'));
              },
            ),
          Btn('CANCEL', color: p.dim, onTap: () => setState(() => _pending = _Pending.none)),
        ]);
      case _Pending.stop:
        final kids = app.state.sleeves
            .where((c) => c.parentId == s.id && c.lifecycle != Lifecycle.stopped)
            .length;
        return Actions3270([
          T('STOP ${s.name}${kids > 0 ? ' AND $kids CHILDREN' : ''}?', color: p.alarm),
          Btn('CONFIRM STOP', color: p.alarm, onTap: () {
            setState(() => _pending = _Pending.none);
            unawaited(app.act(() => app.hub.stop(s.id)));
          }),
          Btn('CANCEL', color: p.dim, onTap: () => setState(() => _pending = _Pending.none)),
        ]);
      case _Pending.none:
        final running = s.lifecycle == Lifecycle.running;
        return Actions3270([
          Btn('SEND', onTap: running ? () => _send(app) : null),
          Btn('INTERRUPT', onTap: running ? () => unawaited(app.act(() => app.hub.interrupt(s.id))) : null),
          Btn('STOP', onTap: s.lifecycle == Lifecycle.stopped
              ? null
              : () => setState(() => _pending = _Pending.stop)),
          Btn('RESLEEVE', onTap: () => setState(() => _pending = _Pending.resleeve)),
          Btn('TERMINAL (ADV)', color: p.dim, onTap: () => setState(() => _takeover = true)),
        ]);
    }
  }
}

class _StatusLine extends StatelessWidget {
  const _StatusLine({required this.sleeve});

  final Sleeve sleeve;

  @override
  Widget build(BuildContext context) {
    final p = pal(context);
    final app = AppScope.of(context);
    final inc = sleeve.incarnation;
    if (inc == null) {
      return Line([sp(context, 'NO INCARNATION  ', color: p.dim), sp(context, sleeve.lifecycle.wire.toUpperCase(), color: p.dim)]);
    }
    final q = app.state.quota(inc.subscription);
    final w = q?.tightest;
    final seg = <Widget>[
      Line([
        sp(context, harnessName(inc.harness).toUpperCase(), color: p.lit, bold: true),
        sp(context, ' ${inc.model}', color: p.fg),
      ]),
      Line([sp(context, 'g', color: p.dim), sp(context, '${inc.g}', color: p.lit),
        sp(context, ' ${inc.mode.wire.toUpperCase()}', color: p.dim)]),
      Line([sp(context, 'turn ', color: p.dim), sp(context, '${inc.turn}', color: p.fg)]),
      Row(mainAxisSize: MainAxisSize.min, children: [
        T('ctx ', color: p.dim),
        Meter(inc.contextFraction, cells: 10, pct: false, warnAt: 0.75, alarmAt: 0.9),
        T(' ${tokens(inc.contextTokens)}', color: p.fg),
      ]),
      if (inc.cacheRatio != null)
        Line([sp(context, 'cache ', color: p.dim), sp(context, '${(inc.cacheRatio! * 100).round()}%', color: p.fg)]),
      if (sleeve.budgetLimit != null)
        Row(mainAxisSize: MainAxisSize.min, children: [
          T('budget ', color: p.dim),
          Meter((sleeve.budgetUsed ?? 0) / sleeve.budgetLimit!, cells: 8, pct: false),
        ]),
      if (w != null)
        Line([
          sp(context, 'quota ', color: p.dim),
          sp(context, '${subscriptionLetter(inc.subscription)} ${w.label} ', color: p.fg),
          sp(context, '${w.usedPct!.round()}%',
              color: w.usedPct! >= 95 ? p.alarm : w.usedPct! >= 80 ? p.warn : p.fg),
          if (w.resetsAt != null)
            sp(context, ' r ${span(w.resetsAt!.difference(DateTime.now()))}', color: p.dim),
        ])
      else
        Line([sp(context, 'quota ', color: p.dim), sp(context, '-', color: p.dim)]),
      if (sleeve.branch != null)
        Line([sp(context, 'branch ', color: p.dim), sp(context, sleeve.branch!, color: p.fg)]),
    ];
    return Wrap(spacing: kCell * 3, children: seg);
  }
}

class _TakeoverPlaceholder extends StatelessWidget {
  const _TakeoverPlaceholder({required this.sleeve, required this.onBack});

  final Sleeve sleeve;
  final VoidCallback onBack;

  @override
  Widget build(BuildContext context) {
    final p = pal(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        T('TERMINAL TAKEOVER IS NOT BUILT YET.', color: p.warn, bold: true),
        const SizedBox(height: kRow),
        T(
          'This is the advanced, last-resort view. At a turn boundary the hub ends the headless '
          'incarnation of ${sleeve.name} and starts the harness TUI in a hub-owned PTY with the '
          "harness's own resume flag on the same session. Handing back works the same way in "
          'reverse. The chat stays the normal way to work with a sleeve.',
          maxLines: null,
        ),
        const SizedBox(height: kRow),
        Btn('BACK TO CHAT', onTap: onBack),
      ],
    );
  }
}

/// The transcript, newest at the bottom, with tool calls collapsed to one line.
class TranscriptView extends StatefulWidget {
  const TranscriptView({super.key, required this.sleeveId, this.envoy = false});

  final String sleeveId;
  final bool envoy;

  @override
  State<TranscriptView> createState() => _TranscriptViewState();
}

class _TranscriptViewState extends State<TranscriptView> {
  final Set<String> _open = {};
  StreamSubscription<Transcript>? _sub;
  late Transcript _t;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _listen();
  }

  @override
  void didUpdateWidget(TranscriptView old) {
    super.didUpdateWidget(old);
    if (old.sleeveId != widget.sleeveId) {
      _open.clear();
      _listen();
    }
  }

  void _listen() {
    final hub = AppScope.of(context).hub;
    if (_sub != null && _t.sleeveId == widget.sleeveId) return;
    unawaited(_sub?.cancel());
    _t = hub.transcript(widget.sleeveId);
    _sub = hub.transcripts(widget.sleeveId).listen((t) => setState(() => _t = t));
  }

  @override
  void dispose() {
    unawaited(_sub?.cancel());
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final rows = foldTranscript(_t.entries);
    if (rows.isEmpty) return T('(none)', color: pal(context).dim);
    return SelectionArea(
      child: ListView.builder(
        reverse: true,
        padding: const EdgeInsets.symmetric(vertical: 4),
        itemCount: rows.length,
        itemBuilder: (c, i) {
          final r = rows[rows.length - 1 - i];
          final prev = rows.length - 2 - i >= 0 ? rows[rows.length - 2 - i] : null;
          final key = switch (r) {
            ToolRow(:final call) => 't:${call.id}',
            TextRow(thinking: true, :final id) => 'r:$id',
            _ => '',
          };
          return _EntryRow(
            row: r,
            envoy: widget.envoy,
            showTime: prev == null || hhmm(prev.at) != hhmm(r.at) || prev.runtimeType != r.runtimeType,
            open: _open.contains(key),
            onToggle: () => setState(() {
              if (!_open.remove(key)) _open.add(key);
            }),
          );
        },
      ),
    );
  }
}

class _EntryRow extends StatelessWidget {
  const _EntryRow({
    required this.row,
    required this.envoy,
    required this.showTime,
    required this.open,
    required this.onToggle,
  });

  final ChatRow row;
  final bool envoy;
  final bool showTime;
  final bool open;
  final VoidCallback onToggle;

  Color _level(Palette p, Level l) => switch (l) {
    Level.info || Level.unknown => p.dim,
    Level.warn => p.warn,
    Level.alarm => p.alarm,
  };

  @override
  Widget build(BuildContext context) {
    final p = pal(context);
    final phone = Term.of(context).phone;
    final r = row;
    final (who, whoColor) = switch (r) {
      UserRow(:final msg) => (msg.from, msg.from == 'owner' ? p.lit : p.dim),
      TextRow(thinking: true) => ('think', p.dim),
      TextRow() => (envoy ? 'envoy' : 'agent', p.dim),
      ToolRow() => ('tool', p.dim),
      ApprovalRow() => ('appr', p.warn),
      LineRow(:final who, :final level) => (who, level == Level.alarm ? p.alarm : p.dim),
    };
    final Widget body = switch (r) {
      UserRow(:final msg, :final ack) => Line([
        sp(context, msg.text, color: msg.from == 'owner' ? p.lit : p.fg),
        if (msg.steer) sp(context, '  STEER', color: p.dim),
        if (ack == AckKind.confirmed) sp(context, '  +', color: p.dim),
      ], maxLines: null),
      TextRow(thinking: true) => _Thinking(row: r, open: open, onToggle: onToggle),
      TextRow(:final text, :final streaming) => Line([
        sp(context, text, color: p.fg),
        if (streaming) sp(context, ' ', bg: p.lit),
      ], maxLines: null),
      ToolRow() => _ToolBody(row: r, open: open, onToggle: onToggle),
      ApprovalRow(:final request) => ApprovalBlock(approvalId: request.approvalId),
      LineRow(:final text, :final level) => T(text, maxLines: null, color: _level(p, level)),
    };
    final gap = r is UserRow || (r is TextRow && !r.thinking && showTime);
    return Padding(
      padding: EdgeInsets.only(top: gap ? 6 : 0),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          if (!phone) Cells(6, child: T(showTime ? hhmm(r.at) : '', color: p.rule)),
          Cells(phone ? 7 : 9, child: T(who.length > 8 ? who.substring(0, 8) : who, color: whoColor)),
          Expanded(child: body),
        ],
      ),
    );
  }
}

/// Reasoning, collapsed to its first line until opened.
class _Thinking extends StatelessWidget {
  const _Thinking({required this.row, required this.open, required this.onToggle});

  final TextRow row;
  final bool open;
  final VoidCallback onToggle;

  @override
  Widget build(BuildContext context) {
    final p = pal(context);
    return MouseRegion(
      cursor: SystemMouseCursors.click,
      child: GestureDetector(
        behavior: HitTestBehavior.opaque,
        onTap: onToggle,
        child: Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            T(open ? 'v ' : '> ', color: p.info),
            Expanded(
              child: T(open ? row.text : row.text.split('\n').first, color: p.dim, maxLines: open ? null : 1),
            ),
          ],
        ),
      ),
    );
  }
}

class _ToolBody extends StatelessWidget {
  const _ToolBody({required this.row, required this.open, required this.onToggle});

  final ToolRow row;
  final bool open;
  final VoidCallback onToggle;

  @override
  Widget build(BuildContext context) {
    final p = pal(context);
    final r = row;
    final res = r.result;
    final (word, color) = switch (res?.status) {
      null => ('RUNNING', p.ok),
      ToolStatus.completed => ('OK', p.dim),
      ToolStatus.failed => ('ERROR', p.alarm),
      ToolStatus.declined => ('DECLINED', p.warn),
      ToolStatus.cancelled => ('CANCELLED', p.warn),
      ToolStatus.unknown => ('?', p.dim),
    };
    final head = MouseRegion(
      cursor: SystemMouseCursors.click,
      child: GestureDetector(
        behavior: HitTestBehavior.opaque,
        onTap: onToggle,
        child: Row(
          children: [
            T(open ? 'v ' : '> ', color: p.info),
            T('${r.call.name} ', color: p.fg, bold: true),
            Expanded(child: T(r.call.summary, color: p.fg)),
            Tick(builder: (c, now) {
              final d = (r.endedAt ?? now).difference(r.at);
              return Line([
                sp(c, ' $word', color: color),
                sp(c, ' ${lpad(span(d), 6)}', color: p.dim),
              ]);
            }),
          ],
        ),
      ),
    );
    if (!open) return head;
    Widget block(String label, String text) => Padding(
      padding: const EdgeInsets.only(left: kCell, top: 2),
      child: Container(
        decoration: BoxDecoration(border: Border(left: BorderSide(color: p.rule))),
        padding: const EdgeInsets.only(left: kCell),
        child: Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Cells(4, child: T(label, color: p.dim)),
            Expanded(child: T(text.isEmpty ? '(none)' : text, color: p.fg, maxLines: null)),
          ],
        ),
      ),
    );
    final out = res == null ? '(running)' : '${res.output}${res.truncated ? '\n(truncated)' : ''}';
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        head,
        block('in', '${r.input}${r.call.truncated ? '\n(truncated)' : ''}'),
        block('out', out),
      ],
    );
  }
}

/// An approval with its decision buttons; used inline in a chat and in the queue.
class ApprovalBlock extends StatelessWidget {
  const ApprovalBlock({super.key, required this.approvalId});

  final String approvalId;

  @override
  Widget build(BuildContext context) {
    final app = AppScope.of(context);
    final p = pal(context);
    final a = app.state.approval(approvalId);
    if (a == null) return T('approval $approvalId (gone)', color: p.dim);
    if (a.state != ApprovalState.pending) {
      final (word, color) = switch (a.state) {
        ApprovalState.approved =>
          (a.decision == Decision.allowSession ? 'APPROVED FOR SESSION' : 'APPROVED', p.ok),
        ApprovalState.denied => ('DENIED', p.alarm),
        ApprovalState.expired => ('EXPIRED', p.dim),
        ApprovalState.cancelled => ('CANCELLED', p.dim),
        ApprovalState.fenced => ('FENCED', p.warn),
        ApprovalState.abandoned => ('ABANDONED', p.warn),
        _ => ('UNKNOWN', p.dim),
      };
      return Line([
        sp(context, '${a.id} ', color: p.dim),
        sp(context, word, color: color),
        if (a.decidedBy != null) sp(context, ' by ${a.decidedBy!.wire}', color: p.dim),
        if (a.decidedAt != null) sp(context, ' ${hhmm(a.decidedAt!)}', color: p.dim),
        sp(context, '  ${a.title} ${a.summary}', color: p.dim),
        if (a.note.isNotEmpty) sp(context, '  (${a.note})', color: p.dim),
      ], maxLines: null);
    }
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Line([
          sp(context, ' NEEDS YOU ', color: p.bg, bg: p.warn, bold: true),
          sp(context, ' ${a.id} ', color: p.warn),
          sp(context, a.kind == ApprovalKind.action ? 'ACTION ' : 'TOOL ', color: p.dim),
          sp(context, '${a.title} ', color: p.lit, bold: true),
          sp(context, a.summary, color: p.lit),
        ], maxLines: null),
        if (a.note.isNotEmpty)
          Padding(
            padding: const EdgeInsets.only(left: kCell),
            child: T(a.note, color: p.warn, maxLines: null),
          ),
        if (a.detail.isNotEmpty && a.detail != a.summary)
          Padding(
            padding: const EdgeInsets.only(left: kCell),
            child: T(a.detail, color: p.fg, maxLines: null),
          ),
        Tick(builder: (c, now) => Actions3270([
          Btn('APPROVE', color: p.ok,
              onTap: () => unawaited(app.act(() => app.hub.decide(a.id, Decision.allowOnce)))),
          if (a.options.contains(Decision.allowSession))
            Btn('FOR SESSION', color: p.ok,
                onTap: () => unawaited(app.act(() => app.hub.decide(a.id, Decision.allowSession)))),
          Btn('DENY', color: p.alarm,
              onTap: () => unawaited(app.act(() => app.hub.decide(a.id, Decision.deny)))),
          T('asked ${span(now.difference(a.requestedAt))} ago', color: p.dim),
          if (a.timeoutAt != null)
            T('times out in ${span(a.timeoutAt!.difference(now))}', color: p.dim),
        ])),
      ],
    );
  }
}
