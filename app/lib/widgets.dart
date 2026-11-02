import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'api/api.g.dart';
import 'icons.dart';
import 'theme.dart';

// ---- text ------------------------------------------------------------------------------------

TextStyle ts(BuildContext c, {Color? color, double? size, FontWeight weight = FontWeight.w400,
    bool mono = false, Color? bg, double? height}) {
  final p = pal(c);
  final s = mono
      ? monoStyle(p, size: size ?? 12.5, weight: weight, color: color, height: height)
      : sansStyle(p, size: size ?? kBase, weight: weight, color: color, height: height);
  return bg == null ? s : s.copyWith(backgroundColor: bg);
}

/// A run of text for building lines out of spans.
TextSpan sp(BuildContext c, String text, {Color? color, FontWeight weight = FontWeight.w400,
    bool mono = false, double? size, Color? bg}) =>
    TextSpan(text: text, style: ts(c, color: color, weight: weight, mono: mono, size: size, bg: bg));

class Line extends StatelessWidget {
  const Line(this.spans, {super.key, this.maxLines = 1});

  final List<InlineSpan> spans;
  final int? maxLines;

  @override
  Widget build(BuildContext context) => Text.rich(
    TextSpan(children: spans),
    maxLines: maxLines,
    overflow: maxLines == null ? TextOverflow.visible : TextOverflow.ellipsis,
    softWrap: maxLines != 1,
  );
}

class T extends StatelessWidget {
  const T(
    this.text, {
    super.key,
    this.color,
    this.size,
    this.weight = FontWeight.w400,
    this.mono = false,
    this.maxLines = 1,
    this.align,
    this.height,
  });

  final String text;
  final Color? color;
  final double? size;
  final FontWeight weight;
  final bool mono;
  final int? maxLines;
  final TextAlign? align;
  final double? height;

  @override
  Widget build(BuildContext context) => Text(
    text,
    style: ts(context, color: color, size: size, weight: weight, mono: mono, height: height),
    maxLines: maxLines,
    textAlign: align,
    overflow: maxLines == null ? TextOverflow.visible : TextOverflow.ellipsis,
    softWrap: maxLines != 1,
  );
}

/// A small label above or beside a value.
class Label extends StatelessWidget {
  const Label(this.text, {super.key, this.color});

  final String text;
  final Color? color;

  @override
  Widget build(BuildContext context) =>
      T(text, size: kSmall, weight: FontWeight.w500, color: color ?? pal(context).faint);
}

// ---- interaction -----------------------------------------------------------------------------

typedef HoverBuilder = Widget Function(BuildContext context, bool hot, bool down, bool focus);

/// Hover, press and keyboard focus for anything clickable; Enter and Space activate it.
class Hover extends StatefulWidget {
  const Hover({super.key, required this.builder, this.onTap, this.tooltip, this.focusable = true});

  final HoverBuilder builder;
  final VoidCallback? onTap;
  final String? tooltip;
  final bool focusable;

  @override
  State<Hover> createState() => _HoverState();
}

class _HoverState extends State<Hover> {
  bool _hot = false;
  bool _down = false;
  bool _focus = false;

  @override
  Widget build(BuildContext context) {
    final enabled = widget.onTap != null;
    Widget w = FocusableActionDetector(
      enabled: enabled && widget.focusable,
      mouseCursor: enabled ? SystemMouseCursors.click : SystemMouseCursors.basic,
      onShowHoverHighlight: (v) => setState(() => _hot = v),
      onShowFocusHighlight: (v) => setState(() => _focus = v),
      actions: {
        ActivateIntent: CallbackAction<ActivateIntent>(onInvoke: (_) => widget.onTap?.call()),
      },
      child: GestureDetector(
        behavior: HitTestBehavior.opaque,
        onTap: widget.onTap,
        onTapDown: enabled ? (_) => setState(() => _down = true) : null,
        onTapUp: enabled ? (_) => setState(() => _down = false) : null,
        onTapCancel: enabled ? () => setState(() => _down = false) : null,
        child: widget.builder(context, enabled && _hot, enabled && _down, enabled && _focus),
      ),
    );
    if (widget.tooltip != null && widget.tooltip!.isNotEmpty) {
      w = Tooltip(message: widget.tooltip, child: w);
    }
    return w;
  }
}

enum BtnKind { primary, secondary, ghost, danger, approve }

/// A button: primary is the one amber action of a view, approve and danger carry their meaning.
class Btn extends StatelessWidget {
  const Btn(
    this.label, {
    super.key,
    this.onTap,
    this.kind = BtnKind.secondary,
    this.glyph,
    this.tooltip,
    this.active = false,
    this.dense = false,
  });

  final String label;
  final VoidCallback? onTap;
  final BtnKind kind;
  final Glyph? glyph;
  final String? tooltip;

  /// Selected, for toggles and filters.
  final bool active;
  final bool dense;

  @override
  Widget build(BuildContext context) {
    final p = pal(context);
    final phone = Look.of(context).phone;
    final enabled = onTap != null;
    return Hover(
      onTap: onTap,
      tooltip: tooltip,
      builder: (c, hot, down, focus) {
        Color fg;
        Color? bg;
        Color? border;
        switch (kind) {
          case BtnKind.primary:
          case BtnKind.approve:
            final base = kind == BtnKind.primary ? p.accent : p.ok;
            fg = kind == BtnKind.primary ? p.onAccent : p.onWarn;
            bg = down
                ? Color.lerp(base, Colors.black, 0.12)
                : hot
                ? Color.lerp(base, Colors.white, p.dark ? 0.12 : 0.08)
                : base;
          case BtnKind.secondary:
            fg = active ? p.accent : p.fg;
            bg = active ? p.tint(p.accent) : (down ? p.press : hot ? p.raised : p.surface);
            border = active ? p.accent.withValues(alpha: 0.55) : p.edge;
          case BtnKind.ghost:
            fg = active ? p.accent : (hot ? p.fg : p.dim);
            bg = active ? p.tint(p.accent) : (down ? p.press : hot ? p.hover : null);
          case BtnKind.danger:
            fg = p.alarm;
            bg = down ? p.tint(p.alarm, 0.22) : hot ? p.tint(p.alarm, 0.14) : null;
            border = p.alarm.withValues(alpha: 0.5);
        }
        final h = dense ? 26.0 : (phone ? 34.0 : 30.0);
        return Opacity(
          opacity: enabled ? 1 : 0.4,
          child: Container(
            height: h,
            padding: EdgeInsets.symmetric(horizontal: dense ? 8 : 12),
            decoration: BoxDecoration(
              color: bg,
              borderRadius: BorderRadius.circular(kRadius),
              border: focus
                  ? Border.all(color: p.accent, width: 1.5)
                  : border == null
                  ? null
                  : Border.all(color: border),
            ),
            child: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                if (glyph != null) ...[
                  Icon16(glyph!, color: fg, size: 14),
                  if (label.isNotEmpty) const SizedBox(width: 6),
                ],
                if (label.isNotEmpty)
                  T(label, color: fg, weight: FontWeight.w500, size: dense ? kSmall : kBase),
              ],
            ),
          ),
        );
      },
    );
  }
}

/// A square icon-only button with a tooltip.
class IconBtn extends StatelessWidget {
  const IconBtn(this.glyph, {super.key, required this.tooltip, this.onTap, this.color, this.size = 28});

  final Glyph glyph;
  final String tooltip;
  final VoidCallback? onTap;
  final Color? color;
  final double size;

  @override
  Widget build(BuildContext context) {
    final p = pal(context);
    return Hover(
      onTap: onTap,
      tooltip: tooltip,
      builder: (c, hot, down, focus) => Opacity(
        opacity: onTap == null ? 0.4 : 1,
        child: Container(
          width: size,
          height: size,
          alignment: Alignment.center,
          decoration: BoxDecoration(
            color: down ? p.press : hot ? p.hover : null,
            borderRadius: BorderRadius.circular(kRadius),
            border: focus ? Border.all(color: p.accent, width: 1.5) : null,
          ),
          child: Icon16(glyph, color: color ?? (hot ? p.fg : p.dim)),
        ),
      ),
    );
  }
}

/// Buttons in a row that wrap on narrow screens.
class Buttons extends StatelessWidget {
  const Buttons(this.children, {super.key, this.spacing = 8});

  final List<Widget> children;
  final double spacing;

  @override
  Widget build(BuildContext context) => Wrap(
    spacing: spacing,
    runSpacing: 8,
    crossAxisAlignment: WrapCrossAlignment.center,
    children: children,
  );
}

/// A key cap for shortcut hints.
class Kbd extends StatelessWidget {
  const Kbd(this.text, {super.key});

  final String text;

  @override
  Widget build(BuildContext context) {
    final p = pal(context);
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 5),
      decoration: BoxDecoration(
        border: Border.all(color: p.line),
        borderRadius: BorderRadius.circular(4),
      ),
      child: T(text, mono: true, size: 10.5, color: p.faint, height: 1.6),
    );
  }
}

// ---- surfaces --------------------------------------------------------------------------------

/// A card: a surface with a hairline border, an optional title row and trailing widget.
class Panel extends StatelessWidget {
  const Panel({
    super.key,
    required this.child,
    this.title,
    this.trailing,
    this.padding = const EdgeInsets.all(16),
    this.accent,
  });

  final Widget child;
  final String? title;
  final Widget? trailing;
  final EdgeInsets padding;

  /// A coloured left edge and tint, for things that need attention.
  final Color? accent;

  @override
  Widget build(BuildContext context) {
    final p = pal(context);
    return Container(
      decoration: BoxDecoration(
        color: accent == null ? p.surface : Color.alphaBlend(p.tint(accent!, 0.035), p.surface),
        borderRadius: BorderRadius.circular(kCardRadius),
        border: Border.all(color: accent == null ? p.line : accent!.withValues(alpha: 0.32)),
      ),
      padding: padding,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        mainAxisSize: MainAxisSize.min,
        children: [
          if (title != null || trailing != null)
            Padding(
              padding: const EdgeInsets.only(bottom: 12),
              child: Row(children: [
                if (title != null)
                  Expanded(child: T(title!, weight: FontWeight.w600, size: kBase)),
                ?trailing,
              ]),
            ),
          child,
        ],
      ),
    );
  }
}

/// A section heading inside a page: a label with an optional count or trailing widget.
class Section extends StatelessWidget {
  const Section(this.title, {super.key, this.trailing, this.top = 24});

  final String title;
  final Widget? trailing;
  final double top;

  @override
  Widget build(BuildContext context) => Padding(
    padding: EdgeInsets.only(top: top, bottom: 10),
    child: Row(children: [
      Expanded(child: T(title, weight: FontWeight.w600, color: pal(context).dim, size: kSmall)),
      ?trailing,
    ]),
  );
}

/// A label and a value in two columns.
class KV extends StatelessWidget {
  const KV(this.label, this.value, {super.key, this.labelWidth = 120});

  final String label;
  final Widget value;
  final double labelWidth;

  @override
  Widget build(BuildContext context) {
    final phone = Look.of(context).phone;
    return Padding(
      padding: const EdgeInsets.only(bottom: 8),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          SizedBox(
            width: label.isEmpty && phone ? 0 : (phone ? 96 : labelWidth),
            child: Padding(
              padding: const EdgeInsets.only(top: 1, right: 12),
              child: T(label, color: pal(context).dim, maxLines: 2),
            ),
          ),
          Expanded(child: value),
        ],
      ),
    );
  }
}

/// What an empty list shows: a glyph, a sentence, and an optional hint.
class EmptyState extends StatelessWidget {
  const EmptyState(this.title, {super.key, this.glyph, this.detail, this.compact = false});

  final String title;
  final Glyph? glyph;
  final String? detail;
  final bool compact;

  @override
  Widget build(BuildContext context) {
    final p = pal(context);
    if (compact) return Padding(padding: const EdgeInsets.symmetric(vertical: 8), child: T(title, color: p.faint));
    return Center(
      child: Padding(
        padding: const EdgeInsets.symmetric(vertical: 40, horizontal: 16),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            if (glyph != null) ...[
              Container(
                width: 40,
                height: 40,
                alignment: Alignment.center,
                decoration: BoxDecoration(
                  color: p.surface,
                  borderRadius: BorderRadius.circular(10),
                  border: Border.all(color: p.line),
                ),
                child: Icon16(glyph!, color: p.faint, size: 18),
              ),
              const SizedBox(height: 12),
            ],
            T(title, weight: FontWeight.w500, color: p.dim),
            if (detail != null) ...[
              const SizedBox(height: 4),
              T(detail!, color: p.faint, size: kSmall, maxLines: 3, align: TextAlign.center),
            ],
          ],
        ),
      ),
    );
  }
}

// ---- status ----------------------------------------------------------------------------------

class Dot extends StatelessWidget {
  const Dot(this.color, {super.key, this.size = 7, this.hollow = false});

  final Color color;
  final double size;
  final bool hollow;

  @override
  Widget build(BuildContext context) => Container(
    width: size,
    height: size,
    decoration: BoxDecoration(
      shape: BoxShape.circle,
      color: hollow ? null : color,
      border: hollow ? Border.all(color: color, width: 1.5) : null,
    ),
  );
}

/// A rounded label in a colour: tinted by default, filled when it needs you.
class Pill extends StatelessWidget {
  const Pill(this.text, {super.key, required this.color, this.filled = false, this.dot = false,
      this.onColor, this.mono = false});

  final String text;
  final Color color;
  final bool filled;
  final bool dot;
  final Color? onColor;
  final bool mono;

  @override
  Widget build(BuildContext context) {
    final p = pal(context);
    final fg = filled ? (onColor ?? p.onWarn) : color;
    return Container(
      padding: EdgeInsets.fromLTRB(dot ? 6 : 7, 1, 7, 1),
      decoration: BoxDecoration(
        color: filled ? color : p.tint(color, 0.13),
        borderRadius: BorderRadius.circular(10),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          if (dot) ...[Dot(fg, size: 6), const SizedBox(width: 5)],
          T(text, color: fg, size: 11.5, weight: FontWeight.w500, mono: mono, height: 1.5),
        ],
      ),
    );
  }
}

/// A rounded bar with zone colours; null is unknown and draws an empty dashed track.
class Meter extends StatelessWidget {
  const Meter(this.fraction, {super.key, this.width = 96, this.pct = true, this.warnAt = 0.8,
      this.alarmAt = 0.95});

  final double? fraction;
  final double? width;
  final bool pct;
  final double warnAt;
  final double alarmAt;

  @override
  Widget build(BuildContext context) {
    final p = pal(context);
    final f = fraction;
    final v = f?.clamp(0.0, 1.0);
    final zone = v == null
        ? p.faint
        : v >= alarmAt
        ? p.alarm
        : v >= warnAt
        ? p.warn
        : p.accent;
    final bar = SizedBox(
      width: width,
      height: 6,
      child: CustomPaint(painter: _MeterPainter(v, zone, p.dark ? p.raised : p.line, p.edge)),
    );
    if (!pct) return bar;
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        width == null ? Expanded(child: bar) : bar,
        SizedBox(
          width: 40,
          child: T(v == null ? '-' : '${(v * 100).round()}%',
              color: v == null ? p.faint : (zone == p.accent ? p.fg : zone),
              align: TextAlign.right, size: kSmall),
        ),
      ],
    );
  }
}

class _MeterPainter extends CustomPainter {
  _MeterPainter(this.v, this.zone, this.track, this.dash);

  final double? v;
  final Color zone, track, dash;

  @override
  void paint(Canvas canvas, Size size) {
    final r = Radius.circular(size.height / 2);
    final full = RRect.fromLTRBR(0, 0, size.width, size.height, r);
    canvas.drawRRect(full, Paint()..color = track);
    final x = v;
    if (x == null) {
      final pt = Paint()
        ..color = dash
        ..strokeWidth = 1.5;
      for (var i = 4.0; i < size.width - 4; i += 6) {
        canvas.drawLine(Offset(i, size.height / 2), Offset(i + 2.5, size.height / 2), pt);
      }
      return;
    }
    if (x <= 0) return;
    final w = (size.width * x).clamp(size.height, size.width);
    canvas.drawRRect(RRect.fromLTRBR(0, 0, w, size.height, r), Paint()..color = zone);
  }

  @override
  bool shouldRepaint(_MeterPainter old) =>
      old.v != v || old.zone != zone || old.track != track || old.dash != dash;
}

class Status {
  const Status(this.word, this.color, {this.urgent = false, this.quiet = false});

  final String word;
  final Color color;

  /// Needs the owner: shown filled.
  final bool urgent;

  /// Nothing going on: a hollow dot.
  final bool quiet;
}

/// The one status a sleeve shows: a lifecycle badge unless it is running, then its activity.
Status sleeveStatus(Palette p, Sleeve s) {
  switch (s.lifecycle) {
    case Lifecycle.running:
      break;
    case Lifecycle.provisioning:
    case Lifecycle.launching:
      return Status('Starting', p.dim);
    case Lifecycle.recovering:
      return Status('Recovering', p.warn);
    case Lifecycle.resleeving:
      return Status('Resleeving', p.warn);
    case Lifecycle.stopping:
      return Status('Stopping', p.warn);
    case Lifecycle.stopped:
      return Status('Stopped', p.faint, quiet: true);
    case Lifecycle.parked:
      return Status('Parked', p.alarm);
    case Lifecycle.removed:
      return Status('Removed', p.faint, quiet: true);
    case Lifecycle.unknown:
      return Status('Unknown', p.faint, quiet: true);
  }
  final a = s.incarnation?.activity;
  if (a == null) return Status('Unknown', p.faint, quiet: true);
  return switch (a.state) {
    ActivityState.booting => Status('Booting', p.dim),
    ActivityState.idle => a.seen ? Status('Idle', p.faint, quiet: true) : Status('Done', p.accent),
    ActivityState.delivering => Status('Delivering', p.ok),
    ActivityState.working => Status('Working', p.ok),
    ActivityState.blocked => Status('Blocked', p.warn, urgent: true),
    ActivityState.interrupting => Status('Interrupting', p.warn),
    ActivityState.compacting => Status('Compacting', p.ok),
    ActivityState.unknown => Status('Unknown', p.faint, quiet: true),
    ActivityState.ended => Status('Ended', p.faint, quiet: true),
  };
}

class StatusDot extends StatelessWidget {
  const StatusDot(this.s, {super.key});

  final Status s;

  @override
  Widget build(BuildContext context) => Dot(s.color, hollow: s.quiet);
}

/// A status as a pill: filled when it needs you.
class StatusPill extends StatelessWidget {
  const StatusPill(this.s, {super.key, this.suffix});

  final Status s;
  final String? suffix;

  @override
  Widget build(BuildContext context) {
    final text = suffix == null ? s.word : '${s.word}  $suffix';
    return Pill(text, color: s.color, filled: s.urgent, dot: !s.urgent);
  }
}

String harnessName(Harness h) => h.wire;

String subscriptionLetter(Subscription s) => switch (s) {
  Subscription.claude => 'C',
  Subscription.codex => 'X',
  Subscription.grok => 'G',
  Subscription.local => 'L',
  Subscription.api => 'A',
  Subscription.unknown => '?',
};

String cap(String s) => s.isEmpty ? s : '${s[0].toUpperCase()}${s.substring(1)}';

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

/// `12s`, `4m`, `3h 12m`, `2d 4h`, for ages and countdowns.
String span(Duration d) {
  if (d.isNegative) d = Duration.zero;
  if (d.inSeconds < 60) return '${d.inSeconds}s';
  if (d.inMinutes < 60) return '${d.inMinutes}m';
  if (d.inHours < 24) return '${d.inHours}h ${d.inMinutes % 60}m';
  return '${d.inDays}d ${d.inHours % 24}h';
}

String bytes(int? b) {
  if (b == null) return '-';
  const k = 1024;
  if (b >= k * k * k * k) return '${(b / (k * k * k * k)).toStringAsFixed(1)} TB';
  if (b >= k * k * k) return '${(b / (k * k * k)).round()} GB';
  if (b >= k * k) return '${(b / (k * k)).round()} MB';
  return '${(b / k).round()} KB';
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

/// A bordered text field. Multiline fields submit on Enter and break the line on Shift+Enter.
class Field extends StatefulWidget {
  const Field({
    super.key,
    required this.controller,
    required this.onSubmit,
    this.focusNode,
    this.hint,
    this.multiline = false,
    this.obscure = false,
    this.mono = false,
    this.leading,
    this.trailing,
    this.bare = false,
    this.autofocus = false,
  });

  final TextEditingController controller;
  final FocusNode? focusNode;
  final String? hint;
  final bool multiline;
  final bool obscure;
  final bool mono;
  final Widget? leading;
  final Widget? trailing;

  /// No border or fill, for fields inside a composed box.
  final bool bare;
  final bool autofocus;
  final void Function(String) onSubmit;

  @override
  State<Field> createState() => _FieldState();
}

class _FieldState extends State<Field> {
  FocusNode? _own;
  FocusNode get _node => widget.focusNode ?? (_own ??= FocusNode());

  @override
  void initState() {
    super.initState();
    _node.addListener(_changed);
  }

  @override
  void didUpdateWidget(Field old) {
    super.didUpdateWidget(old);
    if (old.focusNode != widget.focusNode) {
      (old.focusNode ?? _own)?.removeListener(_changed);
      _node.addListener(_changed);
    }
  }

  void _changed() => setState(() {});

  @override
  void dispose() {
    _node.removeListener(_changed);
    _own?.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final p = pal(context);
    final size = widget.multiline ? kBody : kBase;
    final style = ts(context, mono: widget.mono, size: widget.mono ? 12.5 : size);
    final field = TextField(
      controller: widget.controller,
      focusNode: _node,
      autofocus: widget.autofocus,
      style: style,
      cursorColor: p.accent,
      cursorWidth: 1.5,
      obscureText: widget.obscure,
      minLines: 1,
      maxLines: widget.multiline ? 8 : 1,
      keyboardType: widget.multiline ? TextInputType.multiline : TextInputType.text,
      textInputAction: widget.multiline ? TextInputAction.newline : TextInputAction.done,
      decoration: InputDecoration.collapsed(
        hintText: widget.hint,
        hintStyle: style.copyWith(color: p.faint),
      ),
      onSubmitted: widget.multiline ? null : widget.onSubmit,
    );
    final input = widget.multiline
        ? CallbackShortcuts(
            bindings: {
              const SingleActivator(LogicalKeyboardKey.enter): () =>
                  widget.onSubmit(widget.controller.text),
              const SingleActivator(LogicalKeyboardKey.numpadEnter): () =>
                  widget.onSubmit(widget.controller.text),
            },
            child: field,
          )
        : field;
    final row = Row(
      crossAxisAlignment: widget.multiline ? CrossAxisAlignment.end : CrossAxisAlignment.center,
      children: [
        if (widget.leading != null) ...[widget.leading!, const SizedBox(width: 8)],
        Expanded(child: input),
        if (widget.trailing != null) ...[const SizedBox(width: 8), widget.trailing!],
      ],
    );
    if (widget.bare) return row;
    final focused = _node.hasFocus;
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 7),
      decoration: BoxDecoration(
        color: p.dark ? p.bg : p.surface,
        borderRadius: BorderRadius.circular(kRadius),
        border: Border.all(color: focused ? p.accent.withValues(alpha: 0.8) : p.edge),
      ),
      child: row,
    );
  }
}

/// Gives a table its minimum width and scrolls sideways below it.
class Wide extends StatelessWidget {
  const Wide({super.key, required this.min, required this.child});

  final double min;
  final Widget child;

  @override
  Widget build(BuildContext context) => LayoutBuilder(
    builder: (c, box) {
      if (box.maxWidth >= min) return child;
      return SingleChildScrollView(
        scrollDirection: Axis.horizontal,
        child: SizedBox(width: min, height: box.maxHeight, child: child),
      );
    },
  );
}

/// A table: a header row and rows that highlight on hover and open on click.
class TableHead extends StatelessWidget {
  const TableHead(this.children, {super.key});

  final List<Widget> children;

  @override
  Widget build(BuildContext context) {
    final p = pal(context);
    return Container(
      height: 32,
      padding: const EdgeInsets.symmetric(horizontal: 12),
      decoration: BoxDecoration(border: Border(bottom: BorderSide(color: p.line))),
      child: DefaultTextStyle.merge(
        style: ts(context, size: kSmall, weight: FontWeight.w500, color: p.faint),
        child: Row(children: children),
      ),
    );
  }
}

class ListRow extends StatelessWidget {
  const ListRow({super.key, required this.children, this.onTap, this.selected = false,
      this.height = 38});

  final List<Widget> children;
  final VoidCallback? onTap;
  final bool selected;
  final double height;

  @override
  Widget build(BuildContext context) {
    final p = pal(context);
    return Hover(
      onTap: onTap,
      builder: (c, hot, down, focus) => Container(
        height: height,
        padding: const EdgeInsets.symmetric(horizontal: 12),
        decoration: BoxDecoration(
          color: selected ? p.tint(p.accent, 0.08) : (down ? p.press : hot ? p.hover : null),
          border: Border(bottom: BorderSide(color: p.line.withValues(alpha: 0.6))),
          borderRadius: focus ? BorderRadius.circular(4) : null,
        ),
        foregroundDecoration: focus
            ? BoxDecoration(border: Border.all(color: p.accent), borderRadius: BorderRadius.circular(4))
            : null,
        child: Row(children: children),
      ),
    );
  }
}

/// A table header label as a fixed-width cell.
Widget headCell(String t, double w, {bool right = false}) => SizedBox(
  width: w,
  child: Text(t, textAlign: right ? TextAlign.right : TextAlign.left, maxLines: 1),
);

/// A fixed-width cell.
class Cell extends StatelessWidget {
  const Cell(this.w, {super.key, required this.child, this.right = false});

  final double w;
  final Widget child;
  final bool right;

  @override
  Widget build(BuildContext context) => SizedBox(
    width: w,
    child: right ? Align(alignment: Alignment.centerRight, child: child) : child,
  );
}
