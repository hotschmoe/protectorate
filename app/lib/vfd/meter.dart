import 'dart:math' as math;

import 'package:flutter/material.dart';

import '../theme/tokens.dart';
import 'panel.dart';

int litSegments(double? percent, int segments) =>
    percent == null ? 0 : (percent.clamp(0.0, 100.0) / 100 * segments).round();

// The zone a segment belongs to, by the percentage at its upper edge.
Tone zoneTone(int index, int segments) {
  final top = (index + 1) * 100 / segments;
  if (top > critPercent) return Tone.alarm;
  if (top > warnPercent) return Tone.warn;
  return Tone.lit;
}

bool _reduced(BuildContext context) => MediaQuery.disableAnimationsOf(context);

class MeterPainter extends CustomPainter {
  MeterPainter({
    required this.t,
    required this.percent,
    required this.segments,
    required this.dim,
    required this.fixed,
  });

  final Tokens t;
  final double? percent;
  final int segments;
  final bool dim;
  final Tone? fixed;

  int get lit => litSegments(percent, segments);

  Color litColor(int i) {
    if (dim) return t.p.dim;
    return t.tone(fixed ?? zoneTone(i, segments));
  }

  Color ghostColor(int i) =>
      fixed != null ? t.p.ghost : t.ghostFor(zoneTone(i, segments));

  @override
  void paint(Canvas canvas, Size size) {
    final gap = Tokens.segmentGap;
    final w = (size.width - gap * (segments - 1)) / segments;
    if (w <= 0) return;
    Rect rect(int i) => Rect.fromLTWH(i * (w + gap), 0, w, size.height);
    const r = Radius.circular(1);
    if (percent == null) {
      final dash = Paint()..color = t.p.dim.withValues(alpha: 0.75);
      for (var i = 0; i < segments; i++) {
        final s = rect(i);
        canvas.drawRect(
          Rect.fromCenter(
            center: s.center,
            width: s.width * 0.55,
            height: math.max(1.5, size.height * 0.16),
          ),
          dash,
        );
      }
      return;
    }
    final n = lit;
    if (t.dark && !dim && n > 0) {
      final glow = Paint()
        ..maskFilter = const MaskFilter.blur(BlurStyle.normal, 3.5);
      for (var i = 0; i < n; i++) {
        glow.color = litColor(i).withValues(alpha: 0.5);
        canvas.drawRRect(RRect.fromRectAndRadius(rect(i), r), glow);
      }
    }
    final paint = Paint();
    for (var i = 0; i < segments; i++) {
      paint.color = i < n ? litColor(i) : ghostColor(i);
      canvas.drawRRect(RRect.fromRectAndRadius(rect(i), r), paint);
    }
  }

  @override
  bool shouldRepaint(MeterPainter old) =>
      old.percent != percent ||
      old.dim != dim ||
      old.t != t ||
      old.segments != segments ||
      old.fixed != fixed;
}

// A bar of discrete segments; null percent means unknown and draws dashes
// with no unlit segments, so it can never read as spare capacity.
class SegmentMeter extends StatelessWidget {
  const SegmentMeter({
    super.key,
    required this.percent,
    this.segments = Tokens.segments,
    this.height = Tokens.meterHeight,
    this.dim = false,
    this.tone,
  });

  final double? percent;
  final int segments;
  final double height;
  final bool dim;
  final Tone? tone;

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    final target = percent?.clamp(0.0, 100.0);
    return Semantics(
      value: target == null ? 'unknown' : '${target.round()}%',
      child: SizedBox(
        height: height,
        child: target == null
            ? CustomPaint(
                size: Size.infinite,
                painter: MeterPainter(
                  t: t,
                  percent: null,
                  segments: segments,
                  dim: dim,
                  fixed: tone,
                ),
              )
            : TweenAnimationBuilder<double>(
                tween: Tween(begin: 0, end: target),
                duration: _reduced(context) ? Duration.zero : Tokens.fill,
                curve: Curves.easeOutCubic,
                builder: (context, v, _) => CustomPaint(
                  size: Size.infinite,
                  painter: MeterPainter(
                    t: t,
                    percent: v,
                    segments: segments,
                    dim: dim,
                    fixed: tone,
                  ),
                ),
              ),
      ),
    );
  }
}

double monoWidth(int chars, double fontSize) =>
    chars * fontSize * Tokens.monoAdvance;

// Legend, meter and a value column sized to its text, so values never
// truncate; rows with values up to valueChars long keep their meters aligned.
class MeterRow extends StatelessWidget {
  const MeterRow({
    super.key,
    required this.label,
    required this.percent,
    required this.value,
    this.dim = false,
    this.labelChars = 4,
    this.valueChars = 12,
    this.segments = Tokens.segments,
    this.tone,
    this.meterKey,
  });

  final String label;
  final double? percent;
  final String value;
  final bool dim;
  final int labelChars;
  final int valueChars;
  final int segments;
  final Tone? tone;
  final Key? meterKey;

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    final level = t.levelTone(percent);
    final style = t
        .data(
          dim
              ? t.p.dim
              : level == Tone.lit || level == Tone.dim
              ? t.p.emph
              : t.tone(level),
        )
        .copyWith(
          fontWeight: level == Tone.alarm && !dim ? FontWeight.w700 : null,
        );
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 3),
      child: Row(
        children: [
          if (labelChars > 0)
            SizedBox(
              width: labelChars * (11 * Tokens.monoAdvance + 1.4) + 8,
              child: Text(label, style: t.legend(), maxLines: 1),
            ),
          Expanded(
            child: SegmentMeter(
              key: meterKey,
              percent: percent,
              dim: dim,
              segments: segments,
              tone: tone,
            ),
          ),
          const SizedBox(width: 10),
          ConstrainedBox(
            constraints: BoxConstraints(minWidth: monoWidth(valueChars, 13)),
            child: Text(
              value,
              textAlign: TextAlign.end,
              maxLines: 1,
              softWrap: false,
              style: style,
            ),
          ),
        ],
      ),
    );
  }
}

class _ArcPainter extends CustomPainter {
  _ArcPainter(this.t, this.percent, this.segments, this.dim);

  final Tokens t;
  final double? percent;
  final int segments;
  final bool dim;

  static const _sweep = 1.5 * math.pi;
  static const _start = 0.75 * math.pi;

  @override
  void paint(Canvas canvas, Size size) {
    final stroke = size.shortestSide * 0.085;
    final rect = Rect.fromCircle(
      center: size.center(Offset.zero),
      radius: size.shortestSide / 2 - stroke,
    );
    final n = litSegments(percent, segments);
    final step = _sweep / segments;
    final gap = step * 0.22;
    final paint = Paint()
      ..style = PaintingStyle.stroke
      ..strokeWidth = stroke;
    final glow = Paint()
      ..style = PaintingStyle.stroke
      ..strokeWidth = stroke
      ..maskFilter = const MaskFilter.blur(BlurStyle.normal, 4);
    for (var i = 0; i < segments; i++) {
      final zone = zoneTone(i, segments);
      final on = percent != null && i < n;
      final c = on ? (dim ? t.p.dim : t.tone(zone)) : t.ghostFor(zone);
      final a = _start + i * step + gap / 2;
      if (on && t.dark && !dim) {
        glow.color = c.withValues(alpha: 0.45);
        canvas.drawArc(rect, a, step - gap, false, glow);
      }
      paint.color = percent == null ? t.p.ghost.withValues(alpha: 0.5) : c;
      canvas.drawArc(rect, a, step - gap, false, paint);
    }
  }

  @override
  bool shouldRepaint(_ArcPainter old) =>
      old.percent != percent || old.t != t || old.dim != dim;
}

// One headline value per pane: a segmented 270 degree arc around a readout.
class ArcGauge extends StatelessWidget {
  const ArcGauge({
    super.key,
    required this.percent,
    required this.value,
    required this.caption,
    this.unit = '',
    this.size = 148,
    this.segments = 30,
    this.dim = false,
  });

  final double? percent;
  final String value;
  final String unit;
  final String caption;
  final double size;
  final int segments;
  final bool dim;

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    final target = percent?.clamp(0.0, 100.0);
    final level = t.levelTone(target);
    final color = dim
        ? t.p.dim
        : level == Tone.lit || level == Tone.dim
        ? t.p.emph
        : t.tone(level);
    return Semantics(
      label: caption,
      value: target == null ? 'unknown' : '${target.round()}%',
      child: SizedBox.square(
        dimension: size,
        child: Stack(
          alignment: Alignment.center,
          children: [
            Positioned.fill(
              child: TweenAnimationBuilder<double>(
                tween: Tween(begin: 0, end: target ?? 0),
                duration: _reduced(context) ? Duration.zero : Tokens.fill,
                curve: Curves.easeOutCubic,
                builder: (context, v, _) => CustomPaint(
                  painter: _ArcPainter(
                    t,
                    target == null ? null : v,
                    segments,
                    dim,
                  ),
                ),
              ),
            ),
            Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                SegmentReadout(
                  value: value,
                  unit: unit,
                  size: size * 0.2,
                  color: color,
                ),
                const SizedBox(height: 6),
                Text(caption, style: t.legend()),
              ],
            ),
          ],
        ),
      ),
    );
  }
}

class _DotPainter extends CustomPainter {
  _DotPainter(this.t, this.samples, this.rows, this.pitch, this.dim);

  final Tokens t;
  final List<double?> samples;
  final int rows;
  final double pitch;
  final bool dim;

  @override
  void paint(Canvas canvas, Size size) {
    final cols = (size.width / pitch).floor();
    if (cols <= 0) return;
    final dot = pitch * 0.64;
    final rowPitch = size.height / rows;
    final offset = size.width - cols * pitch;
    final paint = Paint();
    final glow = Paint()
      ..maskFilter = const MaskFilter.blur(BlurStyle.normal, 2.5);
    final start = samples.length - cols;
    for (var c = 0; c < cols; c++) {
      final si = start + c;
      final v = si >= 0 ? samples[si] : null;
      final litRows = v == null
          ? 0
          : (v.clamp(0.0, 100.0) / 100 * rows).ceil().clamp(
              v > 0 ? 1 : 0,
              rows,
            );
      for (var r = 0; r < rows; r++) {
        final zone = zoneTone(r, rows);
        final on = r < litRows;
        final x = offset + c * pitch + (pitch - dot) / 2;
        final y = size.height - (r + 1) * rowPitch + (rowPitch - dot) / 2;
        final rr = RRect.fromRectAndRadius(
          Rect.fromLTWH(x, y, dot, dot),
          Radius.circular(dot / 3),
        );
        final color = on ? (dim ? t.p.dim : t.tone(zone)) : t.ghostFor(zone);
        if (on && t.dark && !dim) {
          glow.color = color.withValues(alpha: 0.5);
          canvas.drawRRect(rr, glow);
        }
        paint.color = color;
        canvas.drawRRect(rr, paint);
      }
    }
  }

  @override
  bool shouldRepaint(_DotPainter old) =>
      old.samples != samples || old.t != t || old.dim != dim;
}

// Recent history, newest on the right; one column of dots per sample.
class DotMatrixGraph extends StatelessWidget {
  const DotMatrixGraph({
    super.key,
    required this.samples,
    this.rows = 8,
    this.pitch = 6,
    this.dim = false,
  });

  final List<double?> samples;
  final int rows;
  final double pitch;
  final bool dim;

  @override
  Widget build(BuildContext context) {
    final last = samples.lastOrNull;
    return Semantics(
      label: 'history',
      value: last == null ? 'no samples' : '${last.round()}% latest',
      child: SizedBox(
        height: rows * pitch,
        child: CustomPaint(
          size: Size.infinite,
          painter: _DotPainter(Tokens.of(context), samples, rows, pitch, dim),
        ),
      ),
    );
  }
}
