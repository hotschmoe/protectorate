import 'dart:math' as math;

import 'package:flutter/material.dart';

import '../theme/tokens.dart';

// Drives a slow attention pulse; with reduced motion it stays at full.
mixin _Pulse<T extends StatefulWidget>
    on State<T>, TickerProviderStateMixin<T> {
  AnimationController? _pulse;

  bool get pulsing;

  bool get reduced => MediaQuery.disableAnimationsOf(context);

  void syncPulse() {
    final want = pulsing && !reduced;
    if (want && _pulse == null) {
      _pulse = AnimationController(vsync: this, duration: Tokens.blink)
        ..repeat();
    } else if (!want && _pulse != null) {
      _pulse!.dispose();
      _pulse = null;
    }
  }

  double level(double v) =>
      0.25 + 0.75 * (0.5 + 0.5 * math.cos(2 * math.pi * v));

  Widget pulse(Widget Function(double level) build) {
    final c = _pulse;
    if (c == null) return build(1);
    return AnimatedBuilder(
      animation: c,
      builder: (_, _) => build(level(c.value)),
    );
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    syncPulse();
  }

  @override
  void dispose() {
    _pulse?.dispose();
    super.dispose();
  }
}

class Lamp extends StatefulWidget {
  const Lamp({
    super.key,
    required this.tone,
    this.on = true,
    this.blink = false,
    this.beat,
    this.size = Tokens.lampSize,
  });

  final Tone tone;
  final bool on;
  final bool blink;
  // A change of beat flickers the lamp off and back once, like a heartbeat.
  final Object? beat;
  final double size;

  @override
  State<Lamp> createState() => _LampState();
}

class _LampState extends State<Lamp> with TickerProviderStateMixin, _Pulse {
  late final AnimationController _beat = AnimationController(
    vsync: this,
    duration: Tokens.beat,
    value: 1,
  );

  @override
  bool get pulsing => widget.on && widget.blink;

  @override
  void didUpdateWidget(Lamp old) {
    super.didUpdateWidget(old);
    syncPulse();
    if (widget.beat != old.beat && old.beat != null && !reduced) {
      _beat.forward(from: 0);
    }
  }

  @override
  void dispose() {
    _beat.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    final c = t.tone(widget.tone);
    final s = widget.size;
    final ring = widget.on && widget.blink && reduced;
    return AnimatedBuilder(
      animation: _beat,
      builder: (_, _) => pulse((p) {
        final v = p * (0.2 + 0.8 * Curves.easeIn.transform(_beat.value));
        final on = widget.on && widget.tone != Tone.ghost;
        final fill = on ? Color.lerp(t.p.ghost, c, v)! : t.p.ghost;
        return Container(
          width: s + (ring ? 6 : 0),
          height: s + (ring ? 6 : 0),
          alignment: Alignment.center,
          decoration: ring
              ? BoxDecoration(
                  shape: BoxShape.circle,
                  border: Border.all(color: c, width: 1.5),
                )
              : null,
          child: Container(
            width: s,
            height: s,
            decoration: BoxDecoration(
              shape: BoxShape.circle,
              color: fill,
              border: on ? null : Border.all(color: t.p.bezelHi),
              boxShadow: on ? t.boxGlow(c.withValues(alpha: v)) : null,
            ),
          ),
        );
      }),
    );
  }
}

// A lamp with its legend; the legend lights with the lamp.
class LampLegend extends StatelessWidget {
  const LampLegend({
    super.key,
    required this.label,
    required this.tone,
    this.on = true,
    this.blink = false,
    this.beat,
    this.big = false,
  });

  final String label;
  final Tone tone;
  final bool on;
  final bool blink;
  final Object? beat;
  final bool big;

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    final c = on && tone != Tone.ghost ? t.tone(tone) : t.p.dim;
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        Lamp(tone: tone, on: on, blink: blink, beat: beat),
        SizedBox(width: big ? 8 : 6),
        Flexible(
          child: Text(
            label,
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: t.legend(c).copyWith(fontSize: big ? 12.5 : 11.5),
          ),
        ),
      ],
    );
  }
}

// A legend tile that lights in its state colour.
class Annunciator extends StatefulWidget {
  const Annunciator({
    super.key,
    required this.label,
    required this.tone,
    this.lit = true,
    this.blink = false,
  });

  final String label;
  final Tone tone;
  final bool lit;
  final bool blink;

  @override
  State<Annunciator> createState() => _AnnunciatorState();
}

class _AnnunciatorState extends State<Annunciator>
    with TickerProviderStateMixin, _Pulse {
  @override
  bool get pulsing => widget.lit && widget.blink;

  @override
  void didUpdateWidget(Annunciator old) {
    super.didUpdateWidget(old);
    syncPulse();
  }

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    final c = t.tone(widget.tone);
    return pulse((v) {
      final lit = widget.lit;
      return Container(
        padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 5),
        decoration: BoxDecoration(
          color: lit
              ? t.tint(c).withValues(alpha: (t.dark ? 0.16 : 0.12) * v)
              : null,
          border: Border.all(
            color: lit ? c.withValues(alpha: 0.4 + 0.5 * v) : t.p.bezel,
          ),
          borderRadius: BorderRadius.circular(Tokens.innerRadius),
          boxShadow: lit && t.dark
              ? [
                  BoxShadow(
                    color: c.withValues(alpha: 0.18 * v),
                    blurRadius: 10,
                  ),
                ]
              : null,
        ),
        child: Text(
          widget.label,
          maxLines: 1,
          overflow: TextOverflow.ellipsis,
          style: t.legend(lit ? c : t.p.dim),
        ),
      );
    });
  }
}

// A row of legend keys of which one is lit: a selector switch. Each key is a
// touch target of its own; a null onSelected disables the row.
class ChoiceKeys<T> extends StatelessWidget {
  const ChoiceKeys({
    super.key,
    required this.options,
    required this.selected,
    required this.onSelected,
    this.tone = Tone.info,
  });

  final List<(T, String)> options;
  final T selected;
  final ValueChanged<T>? onSelected;
  final Tone tone;

  @override
  Widget build(BuildContext context) => Wrap(
    spacing: 6,
    runSpacing: 6,
    children: [
      for (final (value, label) in options)
        Semantics(
          button: true,
          selected: value == selected,
          child: InkWell(
            key: ValueKey('choice-$label'),
            borderRadius: BorderRadius.circular(Tokens.innerRadius),
            onTap: onSelected == null ? null : () => onSelected!(value),
            child: ConstrainedBox(
              constraints: const BoxConstraints(minHeight: Tokens.touch),
              child: Align(
                widthFactor: 1,
                child: Annunciator(
                  label: label,
                  tone: tone,
                  lit: value == selected && onSelected != null,
                ),
              ),
            ),
          ),
        ),
    ],
  );
}

// A segment chaser in place of indeterminate progress.
class BusyBar extends StatefulWidget {
  const BusyBar({super.key, this.tone = Tone.lit, this.height = 3});

  final Tone tone;
  final double height;

  @override
  State<BusyBar> createState() => _BusyBarState();
}

class _BusyBarState extends State<BusyBar> with TickerProviderStateMixin {
  AnimationController? _c;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    final reduced = MediaQuery.disableAnimationsOf(context);
    if (!reduced && _c == null) {
      _c = AnimationController(vsync: this, duration: Tokens.chase)..repeat();
    } else if (reduced && _c != null) {
      _c!.dispose();
      _c = null;
    }
  }

  @override
  void dispose() {
    _c?.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    final c = _c;
    Widget paint(double v) => CustomPaint(
      size: Size(double.infinity, widget.height),
      painter: _ChasePainter(t, t.tone(widget.tone), c == null ? null : v),
    );
    return Semantics(
      label: 'busy',
      child: SizedBox(
        height: widget.height,
        width: double.infinity,
        child: c == null
            ? paint(0)
            : AnimatedBuilder(animation: c, builder: (_, _) => paint(c.value)),
      ),
    );
  }
}

class _ChasePainter extends CustomPainter {
  _ChasePainter(this.t, this.color, this.phase);

  final Tokens t;
  final Color color;
  final double? phase;

  @override
  void paint(Canvas canvas, Size size) {
    const w = 10.0;
    const gap = 3.0;
    final n = ((size.width + gap) / (w + gap)).floor();
    if (n <= 0) return;
    final head = phase == null ? -1.0 : phase! * (n + 6);
    final paint = Paint();
    for (var i = 0; i < n; i++) {
      final d = head - i;
      final a = phase == null
          ? (i % 3 == 0 ? 0.6 : 0.0)
          : (d >= 0 && d < 6 ? 1 - d / 6 : 0.0);
      paint.color = Color.lerp(t.p.ghost, color, a)!;
      canvas.drawRect(Rect.fromLTWH(i * (w + gap), 0, w, size.height), paint);
    }
  }

  @override
  bool shouldRepaint(_ChasePainter old) => old.phase != phase || old.t != t;
}
