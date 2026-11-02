import 'dart:async';

import 'package:flutter/material.dart';

import 'api/api.g.dart';
import 'derived.dart';
import 'icons.dart';
import 'scope.dart';
import 'theme.dart';
import 'widgets.dart';

/// The transcript and composer stay readable on wide windows.
const double _readable = 920;

Widget _column(Widget child) => Align(
  alignment: Alignment.topLeft,
  child: ConstrainedBox(
    constraints: const BoxConstraints(maxWidth: _readable),
    child: SizedBox(width: double.infinity, child: child),
  ),
);

/// A sleeve's chat: header with status and actions, stats, transcript and composer. Envoy uses the
/// same view; it is a root sleeve like any other.
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
  void initState() {
    super.initState();
    _focus.addListener(() => setState(() {}));
  }

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
    final phone = Look.of(context).phone;
    final s = app.state.sleeve(widget.sleeveId);
    if (s == null) return EmptyState('No sleeve ${widget.sleeveId}', glyph: Glyph.sleeves);
    if (_takeover) {
      return _TakeoverPlaceholder(sleeve: s, onBack: () => setState(() => _takeover = false));
    }
    final running = s.lifecycle == Lifecycle.running;
    final actions = <_Action>[
      _Action('Interrupt', Glyph.interrupt, 'Stop the current turn',
          running ? () => unawaited(app.act(() => app.hub.interrupt(s.id))) : null),
      _Action('Stop', Glyph.power, 'Stop this sleeve and its children',
          s.lifecycle == Lifecycle.stopped ? null : () => setState(() => _pending = _Pending.stop)),
      _Action('Resleeve', Glyph.resleeve, 'Move this sleeve to another harness',
          () => setState(() => _pending = _Pending.resleeve)),
      _Action('Terminal', Glyph.terminal, 'Take over the harness terminal (advanced)',
          () => setState(() => _takeover = true)),
    ];
    final st = sleeveStatus(p, s);
    final status = Tick(
      builder: (c, now) => StatusPill(st,
          suffix: s.incarnation == null ? null : clock(now.difference(s.incarnation!.activity.since))),
    );
    final header = phone
        ? Container(
            padding: const EdgeInsets.fromLTRB(12, 8, 8, 8),
            decoration: BoxDecoration(border: Border(bottom: BorderSide(color: p.line))),
            child: Row(children: [
              status,
              const Spacer(),
              for (final a in actions) IconBtn(a.glyph, tooltip: a.label, onTap: a.onTap, size: 34),
            ]),
          )
        : Container(
            height: 56,
            padding: const EdgeInsets.symmetric(horizontal: 24),
            child: Row(children: [
              Expanded(
                child: Row(children: [
                  Flexible(child: T(widget.envoy ? 'Envoy' : s.name, size: kTitle, weight: FontWeight.w600)),
                  const SizedBox(width: 12),
                  status,
                ]),
              ),
              for (final a in actions) ...[
                const SizedBox(width: 4),
                Btn(a.label, glyph: a.glyph, kind: BtnKind.ghost, tooltip: a.tip, onTap: a.onTap),
              ],
            ]),
          );
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        header,
        Container(
          padding: EdgeInsets.fromLTRB(phone ? 12 : 24, phone ? 8 : 0, phone ? 12 : 24, 10),
          decoration: BoxDecoration(border: Border(bottom: BorderSide(color: p.line))),
          child: _Stats(sleeve: s),
        ),
        Expanded(child: TranscriptView(sleeveId: s.id, envoy: widget.envoy)),
        Padding(
          padding: EdgeInsets.fromLTRB(phone ? 8 : 24, 0, phone ? 8 : 24, phone ? 8 : 16),
          child: _column(Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              if (_pending != _Pending.none)
                Padding(padding: const EdgeInsets.only(bottom: 8), child: _pendingBar(app, s, p)),
              _composer(app, s, p, running, phone),
            ],
          )),
        ),
      ],
    );
  }

  Widget _composer(AppScope app, Sleeve s, Palette p, bool running, bool phone) {
    final focused = _focus.hasFocus;
    return Container(
      padding: const EdgeInsets.fromLTRB(14, 12, 8, 8),
      decoration: BoxDecoration(
        color: p.surface,
        borderRadius: BorderRadius.circular(10),
        border: Border.all(color: focused ? p.accent.withValues(alpha: 0.7) : p.edge),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Field(
            controller: _text,
            focusNode: _focus,
            bare: true,
            hint: widget.envoy ? 'Message Envoy' : 'Message ${s.name}',
            multiline: true,
            onSubmit: (_) => _send(app),
          ),
          const SizedBox(height: 6),
          Row(children: [
            if (!phone)
              Expanded(child: T('Enter to send, Shift+Enter for a new line', color: p.faint, size: kSmall))
            else
              const Spacer(),
            Btn('Send', glyph: Glyph.send, kind: BtnKind.primary, dense: true,
                onTap: running ? () => _send(app) : null),
          ]),
        ],
      ),
    );
  }

  Widget _pendingBar(AppScope app, Sleeve s, Palette p) {
    switch (_pending) {
      case _Pending.resleeve:
        final order = [
          ...s.resleeveTo,
          for (final h in const [Harness.claude, Harness.codex, Harness.grok, Harness.piLocal])
            if (!s.resleeveTo.contains(h)) h,
        ];
        final current = s.incarnation?.harness;
        return Panel(
          padding: const EdgeInsets.all(12),
          child: Buttons([
            T('Resleeve ${s.name} to', color: p.dim),
            for (final h in order)
              Btn(
                harnessName(h),
                kind: h == current ? BtnKind.ghost : BtnKind.secondary,
                tooltip: h == current ? 'Current harness' : null,
                onTap: () {
                  setState(() => _pending = _Pending.none);
                  unawaited(app.act(() => app.hub.resleeve(s.id, h),
                      done: 'Resleeving ${s.name} to ${harnessName(h)}'));
                },
              ),
            Btn('Cancel', kind: BtnKind.ghost, onTap: () => setState(() => _pending = _Pending.none)),
          ]),
        );
      case _Pending.stop:
        final kids = app.state.sleeves
            .where((c) => c.parentId == s.id && c.lifecycle != Lifecycle.stopped)
            .length;
        return Panel(
          accent: p.alarm,
          padding: const EdgeInsets.all(12),
          child: Buttons([
            T('Stop ${s.name}${kids > 0 ? ' and $kids children' : ''}?', color: p.alarm,
                weight: FontWeight.w500),
            Btn('Stop', kind: BtnKind.danger, glyph: Glyph.power, onTap: () {
              setState(() => _pending = _Pending.none);
              unawaited(app.act(() => app.hub.stop(s.id)));
            }),
            Btn('Cancel', kind: BtnKind.ghost, onTap: () => setState(() => _pending = _Pending.none)),
          ]),
        );
      case _Pending.none:
        return const SizedBox.shrink();
    }
  }
}

class _Action {
  const _Action(this.label, this.glyph, this.tip, this.onTap);

  final String label;
  final Glyph glyph;
  final String tip;
  final VoidCallback? onTap;
}

/// The incarnation at a glance: harness and model, generation, turn, context, cache, budget,
/// quota and branch.
class _Stats extends StatelessWidget {
  const _Stats({required this.sleeve});

  final Sleeve sleeve;

  @override
  Widget build(BuildContext context) {
    final p = pal(context);
    final app = AppScope.of(context);
    final inc = sleeve.incarnation;
    Widget item(String label, Widget value) => Row(
      mainAxisSize: MainAxisSize.min,
      children: [T(label, color: p.faint, size: kSmall), const SizedBox(width: 6), value],
    );
    Widget v(String t, {Color? color, bool mono = false}) =>
        T(t, color: color ?? p.dim, size: kSmall, mono: mono);
    if (inc == null) {
      return Wrap(spacing: 20, children: [
        item('No incarnation', v(cap(sleeve.lifecycle.wire))),
        if (sleeve.branch != null) _branch(context, sleeve.branch!),
      ]);
    }
    final q = app.state.quota(inc.subscription);
    final w = q?.tightest;
    return Wrap(
      spacing: 20,
      runSpacing: 6,
      crossAxisAlignment: WrapCrossAlignment.center,
      children: [
        Row(mainAxisSize: MainAxisSize.min, children: [
          Pill(harnessName(inc.harness), color: p.accent),
          const SizedBox(width: 8),
          v(inc.model, mono: true, color: p.fg),
        ]),
        item('Gen', v('${inc.g}  ${inc.mode.wire}')),
        item('Turn', v('${inc.turn}')),
        item('Context', Row(mainAxisSize: MainAxisSize.min, children: [
          Meter(inc.contextFraction, width: 64, pct: false, warnAt: 0.75, alarmAt: 0.9),
          const SizedBox(width: 6),
          v(tokens(inc.contextTokens)),
        ])),
        if (inc.cacheRatio != null) item('Cache', v('${(inc.cacheRatio! * 100).round()}%')),
        if (sleeve.budgetLimit != null)
          item('Budget', Meter((sleeve.budgetUsed ?? 0) / sleeve.budgetLimit!, width: 56, pct: false)),
        if (w != null)
          item('Quota', Line([
            sp(context, '${subscriptionLetter(inc.subscription)} ${w.label} ', color: p.dim, size: kSmall),
            sp(context, '${w.usedPct!.round()}%', size: kSmall,
                color: w.usedPct! >= 95 ? p.alarm : w.usedPct! >= 80 ? p.warn : p.dim),
            if (w.resetsAt != null)
              sp(context, '  resets in ${span(w.resetsAt!.difference(DateTime.now()))}',
                  color: p.faint, size: kSmall),
          ]))
        else
          item('Quota', v('-')),
        if (sleeve.branch != null) _branch(context, sleeve.branch!),
      ],
    );
  }

  Widget _branch(BuildContext context, String b) => Row(mainAxisSize: MainAxisSize.min, children: [
    Icon16(Glyph.branch, color: pal(context).faint, size: 13),
    const SizedBox(width: 5),
    T(b, mono: true, size: kSmall, color: pal(context).dim),
  ]);
}

class _TakeoverPlaceholder extends StatelessWidget {
  const _TakeoverPlaceholder({required this.sleeve, required this.onBack});

  final Sleeve sleeve;
  final VoidCallback onBack;

  @override
  Widget build(BuildContext context) {
    final p = pal(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Container(
          height: 56,
          padding: const EdgeInsets.symmetric(horizontal: 24),
          decoration: BoxDecoration(border: Border(bottom: BorderSide(color: p.line))),
          child: Row(children: [
            IconBtn(Glyph.back, tooltip: 'Back to chat', onTap: onBack),
            const SizedBox(width: 8),
            Flexible(child: T('Terminal: ${sleeve.name}', size: kTitle, weight: FontWeight.w600)),
            const SizedBox(width: 12),
            Pill('Takeover', color: p.warn, dot: true),
          ]),
        ),
        Expanded(
          child: _column(Padding(
            padding: const EdgeInsets.all(24),
            child: Panel(
              accent: p.warn,
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Row(children: [
                    Icon16(Glyph.terminal, color: p.warn),
                    const SizedBox(width: 10),
                    T('Terminal takeover is not built yet', weight: FontWeight.w600, color: p.warn),
                  ]),
                  const SizedBox(height: 12),
                  T(
                    'This is the advanced, last-resort view. At a turn boundary the hub ends the headless '
                    'incarnation of ${sleeve.name} and starts the harness TUI in a hub-owned PTY with the '
                    "harness's own resume flag on the same session. Handing back works the same way in "
                    'reverse. The chat stays the normal way to work with a sleeve.',
                    maxLines: null,
                    color: p.dim,
                    height: 1.6,
                  ),
                  const SizedBox(height: 16),
                  Btn('Back to chat', glyph: Glyph.back, onTap: onBack),
                ],
              ),
            ),
          )),
        ),
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
    final phone = Look.of(context).phone;
    if (rows.isEmpty) {
      return EmptyState(widget.envoy ? 'No messages yet' : 'Nothing in this transcript yet',
          glyph: Glyph.envoy, detail: widget.envoy ? 'Ask Envoy anything below.' : null);
    }
    final app = AppScope.of(context);
    final s = app.state.sleeve(widget.sleeveId);
    final harness = s?.incarnation?.harness;
    return SelectionArea(
      child: ListView.builder(
        reverse: true,
        padding: EdgeInsets.fromLTRB(phone ? 12 : 24, 12, phone ? 12 : 24, 16),
        itemCount: rows.length,
        itemBuilder: (c, i) {
          final r = rows[rows.length - 1 - i];
          final prev = rows.length - 2 - i >= 0 ? rows[rows.length - 2 - i] : null;
          final key = switch (r) {
            ToolRow(:final call) => 't:${call.id}',
            TextRow(thinking: true, :final id) => 'r:$id',
            _ => '',
          };
          final who = _speaker(r);
          final head = who != null && (r is UserRow || prev == null || _speaker(prev) != who);
          return _column(_EntryRow(
            row: r,
            envoy: widget.envoy,
            harness: harness,
            head: head,
            open: _open.contains(key),
            onToggle: () => setState(() {
              if (!_open.remove(key)) _open.add(key);
            }),
          ));
        },
      ),
    );
  }

  /// Who a row speaks for, for grouping rows under one header; null for system lines.
  static String? _speaker(ChatRow r) => switch (r) {
    UserRow(:final msg) => 'u:${msg.from}',
    TextRow() || ToolRow() || ApprovalRow() => 'agent',
    LineRow() => null,
  };
}

class _EntryRow extends StatelessWidget {
  const _EntryRow({
    required this.row,
    required this.envoy,
    required this.harness,
    required this.head,
    required this.open,
    required this.onToggle,
  });

  final ChatRow row;
  final bool envoy;
  final Harness? harness;
  final bool head;
  final bool open;
  final VoidCallback onToggle;

  @override
  Widget build(BuildContext context) {
    final p = pal(context);
    final phone = Look.of(context).phone;
    final r = row;
    final indent = phone ? 0.0 : 36.0;
    if (r is LineRow) return Padding(padding: EdgeInsets.only(left: indent), child: _LineEntry(row: r));
    final Widget body = switch (r) {
      UserRow(:final msg, :final ack) => Line([
        sp(context, msg.text, size: kBody),
        if (ack == AckKind.confirmed) sp(context, '  ✓', color: p.faint, size: kSmall),
      ], maxLines: null),
      TextRow(thinking: true) => _Thinking(row: r, open: open, onToggle: onToggle),
      TextRow(:final text, :final streaming) => Markdown(text: text, streaming: streaming),
      ToolRow() => _ToolBody(row: r, open: open, onToggle: onToggle),
      ApprovalRow(:final request) => ApprovalBlock(approvalId: request.approvalId),
      LineRow() => const SizedBox.shrink(),
    };
    if (!head) {
      return Padding(
        padding: EdgeInsets.only(left: indent, top: r is ToolRow ? 4 : 6),
        child: body,
      );
    }
    final (name, nameColor, avatar) = switch (r) {
      UserRow(:final msg) when msg.from == 'owner' => ('You', p.accent, _Avatar.letter('Y', p.accent)),
      UserRow(:final msg) => (msg.from, p.info, _Avatar.letter(msg.from, p.info)),
      _ when envoy => ('Envoy', p.fg, const _Avatar.logo()),
      _ => (harness == null ? 'Agent' : harnessName(harness!), p.fg, _Avatar.letter(
          harness == null ? 'A' : harnessName(harness!), p.dim)),
    };
    final steer = r is UserRow && r.msg.steer;
    return Padding(
      padding: const EdgeInsets.only(top: 18),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          if (!phone) ...[avatar, const SizedBox(width: 12)],
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Row(children: [
                  T(name, weight: FontWeight.w600, color: nameColor),
                  const SizedBox(width: 8),
                  T(hhmm(r.at), color: p.faint, size: kSmall),
                  if (steer) ...[const SizedBox(width: 8), Pill('steer', color: p.info)],
                ]),
                const SizedBox(height: 4),
                body,
              ],
            ),
          ),
        ],
      ),
    );
  }
}

class _Avatar extends StatelessWidget {
  const _Avatar.letter(this.text, this.color) : logo = false;
  const _Avatar.logo() : text = '', color = null, logo = true;

  final String text;
  final Color? color;
  final bool logo;

  @override
  Widget build(BuildContext context) {
    final p = pal(context);
    if (logo) {
      return ClipRRect(
        borderRadius: BorderRadius.circular(6),
        child: Image.asset('assets/logo.png', width: 24, height: 24, filterQuality: FilterQuality.medium),
      );
    }
    final c = color ?? p.dim;
    return Container(
      width: 24,
      height: 24,
      alignment: Alignment.center,
      decoration: BoxDecoration(color: p.tint(c, 0.16), borderRadius: BorderRadius.circular(6)),
      child: T(text.isEmpty ? '?' : text[0].toUpperCase(), color: c, size: kSmall, weight: FontWeight.w600),
    );
  }
}

/// Session starts, notices and exits as a quiet divider; turn footers as a faint line; errors in a
/// tinted box.
class _LineEntry extends StatelessWidget {
  const _LineEntry({required this.row});

  final LineRow row;

  @override
  Widget build(BuildContext context) {
    final p = pal(context);
    final r = row;
    final color = switch (r.level) {
      Level.info || Level.unknown => p.faint,
      Level.warn => p.warn,
      Level.alarm => p.alarm,
    };
    if (r.who == 'turn') {
      return Padding(
        padding: const EdgeInsets.only(top: 6),
        child: T(r.text.replaceAll('  ', '  ·  '), color: color, size: 11.5, mono: true),
      );
    }
    if (r.who == 'error' || r.level == Level.alarm) {
      return Padding(
        padding: const EdgeInsets.only(top: 10),
        child: Container(
          padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 8),
          decoration: BoxDecoration(
            color: p.tint(p.alarm, 0.08),
            borderRadius: BorderRadius.circular(kRadius),
            border: Border.all(color: p.alarm.withValues(alpha: 0.35)),
          ),
          child: Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
            Padding(padding: const EdgeInsets.only(top: 2), child: Icon16(Glyph.alert, color: p.alarm, size: 14)),
            const SizedBox(width: 8),
            Expanded(child: T(r.text, color: p.alarm, maxLines: null, mono: r.text.contains('\n'))),
          ]),
        ),
      );
    }
    if (r.text.contains('\n')) {
      return Padding(
        padding: const EdgeInsets.only(top: 10),
        child: T(r.text, color: color, maxLines: null, mono: true, size: 12),
      );
    }
    return Padding(
      padding: const EdgeInsets.only(top: 14, bottom: 2),
      child: Row(children: [
        Expanded(child: Container(height: 1, color: p.line)),
        const SizedBox(width: 10),
        Flexible(flex: 4, child: T(r.text, color: color, size: kSmall, align: TextAlign.center, maxLines: 2)),
        const SizedBox(width: 10),
        Expanded(child: Container(height: 1, color: p.line)),
      ]),
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
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Hover(
          onTap: onToggle,
          builder: (c, hot, down, focus) => Row(children: [
            Icon16(open ? Glyph.chevronDown : Glyph.chevronRight, color: hot ? p.fg : p.faint, size: 13),
            const SizedBox(width: 6),
            T('Thinking', color: hot ? p.fg : p.dim, size: kSmall, weight: FontWeight.w500),
            if (!open) ...[
              const SizedBox(width: 8),
              Expanded(child: Text(row.text.split('\n').first, maxLines: 1, overflow: TextOverflow.ellipsis,
                  style: ts(c, color: p.faint, size: kSmall).copyWith(fontStyle: FontStyle.italic))),
            ],
          ]),
        ),
        if (open)
          Container(
            margin: const EdgeInsets.only(top: 6, left: 6),
            padding: const EdgeInsets.only(left: 12),
            decoration: BoxDecoration(border: Border(left: BorderSide(color: p.edge, width: 2))),
            child: Text(row.text,
                style: ts(context, color: p.dim, size: 13, height: 1.55).copyWith(fontStyle: FontStyle.italic)),
          ),
      ],
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
      null => ('running', p.ok),
      ToolStatus.completed => ('ok', p.faint),
      ToolStatus.failed => ('error', p.alarm),
      ToolStatus.declined => ('declined', p.warn),
      ToolStatus.cancelled => ('cancelled', p.warn),
      ToolStatus.unknown => ('?', p.faint),
    };
    final head = Hover(
      onTap: onToggle,
      builder: (c, hot, down, focus) => Container(
        height: 32,
        padding: const EdgeInsets.symmetric(horizontal: 10),
        decoration: BoxDecoration(
          color: hot ? p.raised : p.surface,
          borderRadius: open
              ? const BorderRadius.vertical(top: Radius.circular(kRadius))
              : BorderRadius.circular(kRadius),
          border: Border.all(color: focus ? p.accent : p.line),
        ),
        child: Row(
          children: [
            Icon16(open ? Glyph.chevronDown : Glyph.chevronRight, color: p.faint, size: 13),
            const SizedBox(width: 8),
            T(r.call.name, mono: true, weight: FontWeight.w700, size: 12),
            const SizedBox(width: 10),
            Expanded(child: T(r.call.summary, mono: true, color: p.dim, size: 12)),
            const SizedBox(width: 10),
            Tick(builder: (c, now) {
              final d = (r.endedAt ?? now).difference(r.at);
              return Row(mainAxisSize: MainAxisSize.min, children: [
                if (res == null) Pill(word, color: color, dot: true) else T(word, color: color, size: kSmall),
                SizedBox(width: 52, child: T(span(d), color: p.faint, size: kSmall, align: TextAlign.right)),
              ]);
            }),
          ],
        ),
      ),
    );
    if (!open) return head;
    Widget block(String label, String text) => Padding(
      padding: const EdgeInsets.fromLTRB(12, 10, 12, 0),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Label(label),
          const SizedBox(height: 4),
          T(text.isEmpty ? '(none)' : text, mono: true, size: 12, maxLines: null,
              color: text.isEmpty ? p.faint : p.fg),
        ],
      ),
    );
    final out = res == null ? '(running)' : '${res.output}${res.truncated ? '\n(truncated)' : ''}';
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        head,
        Container(
          padding: const EdgeInsets.only(bottom: 12),
          decoration: BoxDecoration(
            color: p.dark ? p.side : p.raised,
            borderRadius: const BorderRadius.vertical(bottom: Radius.circular(kRadius)),
            border: Border(
              left: BorderSide(color: p.line),
              right: BorderSide(color: p.line),
              bottom: BorderSide(color: p.line),
            ),
          ),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              block('Input', '${r.input}${r.call.truncated ? '\n(truncated)' : ''}'),
              block('Output', out),
            ],
          ),
        ),
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
    if (a == null) return T('Approval $approvalId (gone)', color: p.faint);
    if (a.state != ApprovalState.pending) {
      final (word, color) = switch (a.state) {
        ApprovalState.approved =>
          (a.decision == Decision.allowSession ? 'Approved for session' : 'Approved', p.ok),
        ApprovalState.denied => ('Denied', p.alarm),
        ApprovalState.expired => ('Expired', p.faint),
        ApprovalState.cancelled => ('Cancelled', p.faint),
        ApprovalState.fenced => ('Fenced', p.warn),
        ApprovalState.abandoned => ('Abandoned', p.warn),
        _ => ('Unknown', p.faint),
      };
      return Wrap(
        spacing: 8,
        runSpacing: 4,
        crossAxisAlignment: WrapCrossAlignment.center,
        children: [
          Pill(word, color: color, dot: true),
          Line([
            sp(context, '${a.title} ', weight: FontWeight.w500),
            sp(context, a.summary, color: p.dim),
          ], maxLines: null),
          Line([
            sp(context, a.id, mono: true, color: p.faint, size: 11.5),
            if (a.decidedBy != null) sp(context, '  by ${a.decidedBy!.wire}', color: p.faint, size: kSmall),
            if (a.decidedAt != null) sp(context, '  ${hhmm(a.decidedAt!)}', color: p.faint, size: kSmall),
            if (a.note.isNotEmpty) sp(context, '  ${a.note}', color: p.faint, size: kSmall),
          ], maxLines: null),
        ],
      );
    }
    return Panel(
      accent: p.warn,
      padding: const EdgeInsets.fromLTRB(14, 12, 14, 12),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(children: [
            Pill('Needs you', color: p.warn, filled: true),
            const SizedBox(width: 8),
            T(a.kind == ApprovalKind.action ? 'Action' : 'Tool', color: p.dim, size: kSmall),
            const SizedBox(width: 8),
            Flexible(child: T(a.id, mono: true, color: p.faint, size: 11.5)),
          ]),
          const SizedBox(height: 8),
          Line([
            sp(context, '${a.title}  ', weight: FontWeight.w600, mono: true, size: 12.5),
            sp(context, a.summary, mono: true, size: 12.5),
          ], maxLines: null),
          if (a.note.isNotEmpty) ...[
            const SizedBox(height: 6),
            T(a.note, color: p.warn, maxLines: null),
          ],
          if (a.detail.isNotEmpty && a.detail != a.summary) ...[
            const SizedBox(height: 6),
            T(a.detail, color: p.dim, maxLines: null, height: 1.5),
          ],
          const SizedBox(height: 12),
          Tick(builder: (c, now) => Buttons([
            Btn('Approve', kind: BtnKind.approve, glyph: Glyph.check,
                onTap: () => unawaited(app.act(() => app.hub.decide(a.id, Decision.allowOnce)))),
            if (a.options.contains(Decision.allowSession))
              Btn('Approve for session',
                  onTap: () => unawaited(app.act(() => app.hub.decide(a.id, Decision.allowSession)))),
            Btn('Deny', kind: BtnKind.danger, glyph: Glyph.close,
                onTap: () => unawaited(app.act(() => app.hub.decide(a.id, Decision.deny)))),
            T([
              'Asked ${span(now.difference(a.requestedAt))} ago',
              if (a.timeoutAt != null) 'times out in ${span(a.timeoutAt!.difference(now))}',
            ].join(', '), color: p.faint, size: kSmall),
          ])),
        ],
      ),
    );
  }
}

// ---- markdown --------------------------------------------------------------------------------

/// Assistant prose: paragraphs, headings, lists, quotes, inline code, bold, italics and links,
/// with fenced code blocks in monospace (diffs coloured).
class Markdown extends StatelessWidget {
  const Markdown({super.key, required this.text, this.streaming = false});

  final String text;
  final bool streaming;

  @override
  Widget build(BuildContext context) {
    final p = pal(context);
    final blocks = <Widget>[];
    final body = ts(context, size: kBody, height: 1.6);
    final para = <String>[];
    final heading = RegExp(r'^ {0,3}(#{1,6})\s+(.+?)(?:\s+#+)?$');
    final item = RegExp(r'^(\s*)([-+*]|\d+[.)])\s+(.+)$');
    Widget prose(String s, TextStyle style) =>
        Text.rich(TextSpan(children: _inline(context, s, style)), style: style);
    void flush() {
      if (para.isEmpty) return;
      blocks.add(prose(para.join('\n'), body));
      para.clear();
    }
    final lines = text.split('\n');
    for (var i = 0; i < lines.length; i++) {
      final l = lines[i];
      if (l.trimLeft().startsWith('```')) {
        flush();
        final lang = l.trimLeft().substring(3).trim();
        final code = <String>[];
        i++;
        while (i < lines.length && !lines[i].trimLeft().startsWith('```')) {
          code.add(lines[i]);
          i++;
        }
        blocks.add(CodeBlock(text: code.join('\n'), lang: lang));
        continue;
      }
      final h = heading.firstMatch(l);
      final li = item.firstMatch(l);
      if (l.trim().isEmpty) {
        flush();
      } else if (h != null) {
        flush();
        blocks.add(prose(h[2]!, body.copyWith(
            fontSize: h[1]!.length <= 2 ? 16 : kBody, fontWeight: FontWeight.w600)));
      } else if (li != null) {
        flush();
        final ordered = !'-+*'.contains(li[2]!);
        blocks.add(Padding(
          padding: EdgeInsets.only(left: 4 + li[1]!.length.clamp(0, 8) * 6.0),
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              SizedBox(width: ordered ? 22 : 14, child: Text(ordered ? li[2]! : '•', style: body.copyWith(color: p.faint))),
              Expanded(child: prose(li[3]!, body)),
            ],
          ),
        ));
      } else if (l.trimLeft().startsWith('> ')) {
        flush();
        blocks.add(Container(
          padding: const EdgeInsets.only(left: 12),
          decoration: BoxDecoration(border: Border(left: BorderSide(color: p.edge, width: 2))),
          child: prose(l.trimLeft().substring(2), body.copyWith(color: p.dim)),
        ));
      } else {
        para.add(l);
      }
    }
    flush();
    if (streaming) {
      blocks.add(Container(width: 7, height: 15, color: p.accent));
    }
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        for (var i = 0; i < blocks.length; i++)
          Padding(padding: EdgeInsets.only(top: i == 0 ? 0 : 8), child: blocks[i]),
      ],
    );
  }
}

final _inlineRe = RegExp(
  r'\\[\\`*_\[\]()]|`([^`\n]+)`|\[([^\]\n]+)\]\(([^)\n]+)\)|\*\*(.+?)\*\*|__(.+?)__|\*([^*\n]+)\*|(?<![\w])_([^_\n]+)_(?![\w])',
);

List<InlineSpan> _inline(BuildContext c, String text, TextStyle style, [int depth = 0]) {
  final p = pal(c);
  if (depth >= 6) return [TextSpan(text: text, style: style)];
  final out = <InlineSpan>[];
  var end = 0;
  for (final m in _inlineRe.allMatches(text)) {
    if (m.start > end) out.add(TextSpan(text: text.substring(end, m.start), style: style));
    if (m[1] != null) {
      out.add(TextSpan(
        text: m[1],
        style: monoStyle(p, size: (style.fontSize ?? kBody) - 1.5, color: p.fg)
            .copyWith(backgroundColor: p.raised),
      ));
    } else if (m[2] != null) {
      out.addAll(_inline(c, m[2]!, style.copyWith(color: p.info), depth + 1));
      if (m[2] != m[3]) out.add(TextSpan(text: ' (${m[3]})', style: style.copyWith(color: p.faint)));
    } else if (m[4] != null || m[5] != null) {
      out.addAll(_inline(c, m[4] ?? m[5]!, style.copyWith(fontWeight: FontWeight.w600), depth + 1));
    } else if (m[6] != null || m[7] != null) {
      out.addAll(_inline(c, m[6] ?? m[7]!, style.copyWith(fontStyle: FontStyle.italic), depth + 1));
    } else {
      out.add(TextSpan(text: m[0]!.substring(1), style: style));
    }
    end = m.end;
  }
  if (end < text.length) out.add(TextSpan(text: text.substring(end), style: style));
  return out;
}

bool _looksLikeDiff(String s) =>
    s.startsWith('--- ') || s.startsWith('diff ') || RegExp(r'^@@ .* @@', multiLine: true).hasMatch(s);

/// Monospace text in a box that scrolls sideways; a diff gets its additions and deletions coloured.
class CodeBlock extends StatelessWidget {
  const CodeBlock({super.key, required this.text, this.lang = ''});

  final String text;
  final String lang;

  @override
  Widget build(BuildContext context) {
    final p = pal(context);
    final diff = lang == 'diff' || _looksLikeDiff(text);
    final base = monoStyle(p, size: 12.5, height: 1.55);
    final lines = text.replaceAll('\t', '    ').split('\n');
    Widget line(String l) {
      Color? fg;
      Color? bg;
      if (diff) {
        if (l.startsWith('+++') || l.startsWith('---')) {
          fg = p.faint;
        } else if (l.startsWith('@@')) {
          fg = p.info;
        } else if (l.startsWith('+')) {
          fg = p.ok;
          bg = p.tint(p.ok, 0.10);
        } else if (l.startsWith('-')) {
          fg = p.alarm;
          bg = p.tint(p.alarm, 0.10);
        }
      }
      return Container(
        color: bg,
        padding: const EdgeInsets.symmetric(horizontal: 12),
        child: Text(l.isEmpty ? ' ' : l, softWrap: false, style: fg == null ? base : base.copyWith(color: fg)),
      );
    }
    return Container(
      decoration: BoxDecoration(
        color: p.dark ? p.side : p.raised,
        borderRadius: BorderRadius.circular(kRadius),
        border: Border.all(color: p.line),
      ),
      padding: const EdgeInsets.symmetric(vertical: 10),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          if (lang.isNotEmpty)
            Padding(padding: const EdgeInsets.fromLTRB(12, 0, 12, 6), child: Label(lang)),
          LayoutBuilder(
            builder: (c, box) => SingleChildScrollView(
              scrollDirection: Axis.horizontal,
              child: ConstrainedBox(
                constraints: BoxConstraints(minWidth: box.maxWidth),
                child: IntrinsicWidth(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [for (final l in lines) line(l)],
                  ),
                ),
              ),
            ),
          ),
        ],
      ),
    );
  }
}
