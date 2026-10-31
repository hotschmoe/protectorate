import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'model.dart';
import 'theme.dart';

// ---- text ------------------------------------------------------------------------------------

TextStyle ts(BuildContext c, {Color? color, bool bold = false, Color? bg}) {
  final p = pal(c);
  return baseStyle(p).copyWith(
    color: color ?? p.fg,
    fontWeight: bold ? FontWeight.w700 : FontWeight.w400,
    backgroundColor: bg,
  );
}

/// A run of text in one colour, for building lines out of spans.
TextSpan sp(BuildContext c, String text, {Color? color, bool bold = false, Color? bg}) =>
    TextSpan(text: text, style: ts(c, color: color, bold: bold, bg: bg));

class Line extends StatelessWidget {
  const Line(this.spans, {super.key, this.maxLines = 1});

  final List<InlineSpan> spans;
  final int? maxLines;

  @override
  Widget build(BuildContext context) => Text.rich(
    TextSpan(children: spans),
    maxLines: maxLines,
    overflow: maxLines == null ? TextOverflow.visible : TextOverflow.clip,
    softWrap: maxLines != 1,
  );
}

class T extends StatelessWidget {
  const T(this.text, {super.key, this.color, this.bold = false, this.bg, this.maxLines = 1});

  final String text;
  final Color? color;
  final bool bold;
  final Color? bg;
  final int? maxLines;

  @override
  Widget build(BuildContext context) => Text(
    text,
    style: ts(context, color: color, bold: bold, bg: bg),
    maxLines: maxLines,
    overflow: maxLines == null ? TextOverflow.visible : TextOverflow.ellipsis,
    softWrap: maxLines != 1,
  );
}

/// A fixed-width column measured in character cells.
class Cells extends StatelessWidget {
  const Cells(this.n, {super.key, required this.child, this.right = false});

  final int n;
  final Widget child;
  final bool right;

  @override
  Widget build(BuildContext context) => SizedBox(
    width: n * kCell,
    child: right ? Align(alignment: Alignment.centerRight, child: child) : child,
  );
}

String pad(String s, int n) => s.length >= n ? s.substring(0, n) : s.padRight(n);
String lpad(String s, int n) => s.length >= n ? s : s.padLeft(n);

// ---- rules and panes -------------------------------------------------------------------------

class HRule extends StatelessWidget {
  const HRule({super.key, this.label, this.status});

  final String? label;
  final String? status;

  @override
  Widget build(BuildContext context) {
    final p = pal(context);
    Widget rule() => Expanded(child: Container(height: 1, color: p.rule));
    return SizedBox(
      height: kRow,
      child: Row(
        children: [
          if (label != null) ...[
            SizedBox(width: kCell * 2, child: Container(height: 1, color: p.rule)),
            T(' $label ', color: p.dim),
          ],
          rule(),
          if (status != null) ...[
            T(' $status ', color: p.dim),
            SizedBox(width: kCell * 2, child: Container(height: 1, color: p.rule)),
          ],
        ],
      ),
    );
  }
}

/// A box of single 1 px rules with its legend and a status at the right end of the top rule.
/// The focused pane gets a double rule.
class Pane extends StatelessWidget {
  const Pane({
    super.key,
    required this.legend,
    required this.child,
    this.status,
    this.focused = false,
    this.padding = const EdgeInsets.fromLTRB(kCell, 0, kCell, 4),
  });

  final String legend;
  final Widget? status;
  final Widget child;
  final bool focused;
  final EdgeInsets padding;

  @override
  Widget build(BuildContext context) {
    final p = pal(context);
    final color = focused ? p.edge : p.rule;
    const half = kRow / 2;
    return Stack(
      children: [
        Positioned.fill(
          top: half,
          child: DecoratedBox(
            decoration: BoxDecoration(border: Border.all(color: color)),
            child: focused
                ? Padding(
                    padding: const EdgeInsets.all(2),
                    child: DecoratedBox(
                      decoration: BoxDecoration(border: Border.all(color: color)),
                    ),
                  )
                : null,
          ),
        ),
        Padding(
          padding: padding.copyWith(top: kRow + padding.top),
          child: child,
        ),
        Positioned(
          top: 0,
          left: kCell,
          child: ColoredBox(
            color: p.bg,
            child: T(' $legend ', color: focused ? p.lit : p.dim, bold: focused),
          ),
        ),
        if (status != null)
          Positioned(
            top: 0,
            right: kCell,
            child: ColoredBox(
              color: p.bg,
              child: Padding(
                padding: const EdgeInsets.symmetric(horizontal: kCell),
                child: status,
              ),
            ),
          ),
      ],
    );
  }
}

// ---- buttons ---------------------------------------------------------------------------------

/// A bracketed action, `[APPROVE]`. Hover and keyboard focus show it in reverse video.
class Btn extends StatefulWidget {
  const Btn(this.label, {super.key, this.onTap, this.color, this.brackets = true, this.active});

  final String label;
  final VoidCallback? onTap;
  final Color? color;
  final bool brackets;

  /// Shown in reverse video regardless of hover, for the current screen or a pending choice.
  final bool? active;

  @override
  State<Btn> createState() => _BtnState();
}

class _BtnState extends State<Btn> {
  bool _hot = false;

  @override
  Widget build(BuildContext context) {
    final p = pal(context);
    final enabled = widget.onTap != null;
    final c = enabled ? (widget.color ?? p.lit) : p.rule;
    final rev = enabled && (_hot || (widget.active ?? false));
    final text = widget.brackets ? '[${widget.label}]' : widget.label;
    return FocusableActionDetector(
      enabled: enabled,
      mouseCursor: enabled ? SystemMouseCursors.click : SystemMouseCursors.basic,
      onShowHoverHighlight: (v) => setState(() => _hot = v),
      onShowFocusHighlight: (v) => setState(() => _hot = v),
      actions: {
        ActivateIntent: CallbackAction<ActivateIntent>(onInvoke: (_) => widget.onTap?.call()),
      },
      child: GestureDetector(
        behavior: HitTestBehavior.opaque,
        onTap: widget.onTap,
        child: Text(
          text,
          maxLines: 1,
          style: ts(context, color: rev ? p.bg : c, bg: rev ? c : null, bold: rev),
        ),
      ),
    );
  }
}

/// Space-separated buttons that wrap on narrow screens.
class Actions3270 extends StatelessWidget {
  const Actions3270(this.children, {super.key});

  final List<Widget> children;

  @override
  Widget build(BuildContext context) =>
      Wrap(spacing: kCell, runSpacing: 0, children: children);
}

// ---- meters ----------------------------------------------------------------------------------

/// `[######..........]  38%`, 16 cells by default; null is unknown: `[ -------------- ]`.
class Meter extends StatelessWidget {
  const Meter(this.fraction, {super.key, this.cells = 16, this.pct = true, this.warnAt = 0.8,
      this.alarmAt = 0.95});

  final double? fraction;
  final int cells;
  final bool pct;
  final double warnAt;
  final double alarmAt;

  @override
  Widget build(BuildContext context) {
    final p = pal(context);
    final f = fraction;
    if (f == null) {
      return Line([
        sp(context, '[', color: p.dim),
        sp(context, ' ${'-' * (cells - 2)} ', color: p.rule),
        sp(context, ']', color: p.dim),
        if (pct) sp(context, '   - ', color: p.dim),
      ]);
    }
    final v = f.clamp(0.0, 1.0);
    final n = (v * cells).round();
    final zone = v >= alarmAt
        ? p.alarm
        : v >= warnAt
        ? p.warn
        : p.lit;
    return Line([
      sp(context, '[', color: p.dim),
      sp(context, '#' * n, color: zone),
      sp(context, '.' * (cells - n), color: p.edge),
      sp(context, ']', color: p.dim),
      if (pct) sp(context, lpad('${(v * 100).round()}%', 5), color: zone),
    ]);
  }
}

// ---- status words ----------------------------------------------------------------------------

class Status {
  const Status(this.word, this.glyph, this.color, {this.reverse = false});

  final String word;
  final String glyph;
  final Color color;
  final bool reverse;
}

/// The one status a sleeve shows: a lifecycle badge unless it is running, then its activity.
Status sleeveStatus(Palette p, Sleeve s) {
  switch (s.lifecycle) {
    case Lifecycle.running:
      break;
    case Lifecycle.provisioning:
    case Lifecycle.launching:
      return Status('STARTING', '^', p.dim);
    case Lifecycle.recovering:
      return Status('RECOVERING', '~', p.warn);
    case Lifecycle.resleeving:
      return Status('RESLEEVING', '~', p.warn);
    case Lifecycle.stopping:
      return Status('STOPPING', 'x', p.warn);
    case Lifecycle.stopped:
      return Status('STOPPED', '-', p.dim);
    case Lifecycle.parked:
      return Status('PARKED', '#', p.alarm);
    case Lifecycle.removed:
      return Status('REMOVED', '-', p.rule);
    case Lifecycle.unknown:
      return Status('UNKNOWN', '?', p.dim);
  }
  final a = s.incarnation?.activity;
  if (a == null) return Status('UNKNOWN', '?', p.dim);
  return switch (a.state) {
    ActivityState.booting => Status('BOOTING', '^', p.dim),
    ActivityState.idle => a.seen ? Status('IDLE', '.', p.dim) : Status('DONE', '+', p.lit),
    ActivityState.delivering => Status('DELIVERING', '>', p.ok),
    ActivityState.working => Status('WORKING', '*', p.ok),
    ActivityState.blocked => Status('BLOCKED', '!', p.warn, reverse: true),
    ActivityState.interrupting => Status('INTERRUPTING', 'x', p.warn),
    ActivityState.compacting => Status('COMPACTING', '~', p.ok),
    ActivityState.unknown => Status('UNKNOWN', '?', p.dim),
    ActivityState.ended => Status('ENDED', '-', p.dim),
  };
}

TextSpan statusSpan(BuildContext c, Status s, {int width = 0}) {
  final p = pal(c);
  final w = width > 0 ? pad(s.word, width) : s.word;
  return s.reverse
      ? sp(c, w, color: p.bg, bg: s.color, bold: true)
      : sp(c, w, color: s.color);
}

String harnessName(Harness h) => switch (h) {
  Harness.piVendor => 'pi_vendor',
  Harness.piLocal => 'pi_local',
  _ => h.name,
};

String subscriptionLetter(Subscription s) => switch (s) {
  Subscription.claude => 'C',
  Subscription.codex => 'X',
  Subscription.grok => 'G',
  Subscription.local => 'L',
  Subscription.unknown => '?',
};

// ---- formatting ------------------------------------------------------------------------------

String two(int n) => n.toString().padLeft(2, '0');
String hhmm(DateTime t) => '${two(t.hour)}:${two(t.minute)}';
String hhmmss(DateTime t) => '${hhmm(t)}:${two(t.second)}';

/// `00:14`, `1:02:09`, for running clocks.
String clock(Duration d) {
  if (d.isNegative) d = Duration.zero;
  final h = d.inHours;
  final m = d.inMinutes % 60;
  final s = d.inSeconds % 60;
  return h > 0 ? '$h:${two(m)}:${two(s)}' : '${two(m)}:${two(s)}';
}

/// `12s`, `4m`, `3h12m`, `2d04h`, for ages and countdowns.
String span(Duration d) {
  if (d.isNegative) d = Duration.zero;
  if (d.inSeconds < 60) return '${d.inSeconds}s';
  if (d.inMinutes < 60) return '${d.inMinutes}m';
  if (d.inHours < 24) return '${d.inHours}h${two(d.inMinutes % 60)}m';
  return '${d.inDays}d${two(d.inHours % 24)}h';
}

String bytes(int? b) {
  if (b == null) return '-';
  const k = 1024;
  if (b >= k * k * k * k) return '${(b / (k * k * k * k)).toStringAsFixed(1)}T';
  if (b >= k * k * k) return '${(b / (k * k * k)).round()}G';
  if (b >= k * k) return '${(b / (k * k)).round()}M';
  return '${(b / k).round()}K';
}

String tokens(int n) {
  if (n >= 1000000) return '${(n / 1000000).toStringAsFixed(2)}M';
  if (n >= 1000) return '${(n / 1000).toStringAsFixed(1)}k';
  return '$n';
}

// ---- time ------------------------------------------------------------------------------------

/// Rebuilds its child once a second, for clocks and elapsed times.
class Tick extends StatefulWidget {
  const Tick({super.key, required this.builder});

  final Widget Function(BuildContext, DateTime) builder;

  @override
  State<Tick> createState() => _TickState();
}

class _TickState extends State<Tick> {
  late final Timer _t;

  @override
  void initState() {
    super.initState();
    _t = Timer.periodic(const Duration(seconds: 1), (_) => setState(() {}));
  }

  @override
  void dispose() {
    _t.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => widget.builder(context, DateTime.now());
}

// ---- input -----------------------------------------------------------------------------------

/// A single-style text field with a prompt; Enter submits, Shift+Enter breaks the line.
class Field extends StatelessWidget {
  const Field({
    super.key,
    required this.controller,
    required this.prompt,
    required this.onSubmit,
    this.focusNode,
    this.hint,
    this.multiline = false,
    this.obscure = false,
    this.promptColor,
  });

  final TextEditingController controller;
  final FocusNode? focusNode;
  final String prompt;
  final String? hint;
  final bool multiline;
  final bool obscure;
  final Color? promptColor;
  final void Function(String) onSubmit;

  @override
  Widget build(BuildContext context) {
    final p = pal(context);
    final field = TextField(
      controller: controller,
      focusNode: focusNode,
      style: ts(context, color: p.lit),
      cursorColor: p.lit,
      cursorWidth: kCell,
      cursorHeight: kRow - 4,
      cursorRadius: Radius.zero,
      cursorOpacityAnimates: false,
      obscureText: obscure,
      minLines: 1,
      maxLines: multiline ? 6 : 1,
      keyboardType: multiline ? TextInputType.multiline : TextInputType.text,
      textInputAction: multiline ? TextInputAction.newline : TextInputAction.done,
      decoration: InputDecoration.collapsed(
        hintText: hint,
        hintStyle: ts(context, color: p.rule),
      ),
      onSubmitted: multiline ? null : onSubmit,
    );
    return Row(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        T(prompt, color: promptColor ?? p.dim),
        Expanded(
          child: multiline
              ? CallbackShortcuts(
                  bindings: {
                    const SingleActivator(LogicalKeyboardKey.enter): () =>
                        onSubmit(controller.text),
                    const SingleActivator(LogicalKeyboardKey.numpadEnter): () =>
                        onSubmit(controller.text),
                  },
                  child: field,
                )
              : field,
        ),
      ],
    );
  }
}

/// Gives a table its minimum width in cells and scrolls sideways below it.
class Wide extends StatelessWidget {
  const Wide({super.key, required this.cells, required this.child});

  final int cells;
  final Widget child;

  @override
  Widget build(BuildContext context) => LayoutBuilder(
    builder: (c, box) {
      final min = cells * kCell;
      if (box.maxWidth >= min) return child;
      return SingleChildScrollView(
        scrollDirection: Axis.horizontal,
        child: SizedBox(width: min, height: box.maxHeight, child: child),
      );
    },
  );
}
