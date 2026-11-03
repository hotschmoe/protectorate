import 'dart:math' as math;

import 'package:flutter/widgets.dart';

/// Line icons drawn on a 16 unit grid with a 1.5 unit stroke; the app bundles no icon font.
enum Glyph {
  envoy,
  sleeves,
  approvals,
  nodes,
  quota,
  events,
  settings,
  search,
  chevronRight,
  chevronDown,
  back,
  send,
  interrupt,
  power,
  resleeve,
  terminal,
  branch,
  check,
  close,
  menu,
  alert,
  download,
}

class Icon16 extends StatelessWidget {
  const Icon16(this.glyph, {super.key, required this.color, this.size = 16});

  final Glyph glyph;
  final Color color;
  final double size;

  @override
  Widget build(BuildContext context) => CustomPaint(
    size: Size.square(size),
    painter: _GlyphPainter(glyph, color),
  );
}

class _GlyphPainter extends CustomPainter {
  _GlyphPainter(this.glyph, this.color);

  final Glyph glyph;
  final Color color;

  @override
  void paint(Canvas canvas, Size size) {
    final k = size.width / 16;
    canvas.scale(k);
    final s = Paint()
      ..color = color
      ..style = PaintingStyle.stroke
      ..strokeWidth = 1.5
      ..strokeCap = StrokeCap.round
      ..strokeJoin = StrokeJoin.round;
    final f = Paint()
      ..color = color
      ..style = PaintingStyle.fill;
    void line(double x0, double y0, double x1, double y1) =>
        canvas.drawLine(Offset(x0, y0), Offset(x1, y1), s);
    void poly(List<Offset> pts, {bool close = false}) {
      final path = Path()..addPolygon(pts, close);
      canvas.drawPath(path, s);
    }
    RRect rr(double l, double t, double r, double b, double rad) =>
        RRect.fromLTRBR(l, t, r, b, Radius.circular(rad));

    switch (glyph) {
      case Glyph.envoy:
        final p = Path()
          ..moveTo(4.5, 2.75)
          ..lineTo(11.5, 2.75)
          ..arcToPoint(const Offset(13.75, 5), radius: const Radius.circular(2.25))
          ..lineTo(13.75, 9.5)
          ..arcToPoint(const Offset(11.5, 11.75), radius: const Radius.circular(2.25))
          ..lineTo(7.5, 11.75)
          ..lineTo(4.75, 13.75)
          ..lineTo(4.75, 11.75)
          ..lineTo(4.5, 11.75)
          ..arcToPoint(const Offset(2.25, 9.5), radius: const Radius.circular(2.25))
          ..lineTo(2.25, 5)
          ..arcToPoint(const Offset(4.5, 2.75), radius: const Radius.circular(2.25))
          ..close();
        canvas.drawPath(p, s);
        line(5.5, 6.25, 10.5, 6.25);
        line(5.5, 8.75, 8.5, 8.75);
      case Glyph.sleeves:
        poly([const Offset(8, 2.25), const Offset(14, 5.25), const Offset(8, 8.25),
            const Offset(2, 5.25)], close: true);
        poly([const Offset(2, 8.25), const Offset(8, 11.25), const Offset(14, 8.25)]);
        poly([const Offset(2, 11), const Offset(8, 14), const Offset(14, 11)]);
      case Glyph.approvals:
        final p = Path()
          ..moveTo(8, 1.75)
          ..lineTo(13.25, 3.75)
          ..lineTo(13.25, 7.5)
          ..cubicTo(13.25, 10.75, 10.75, 13, 8, 14.25)
          ..cubicTo(5.25, 13, 2.75, 10.75, 2.75, 7.5)
          ..lineTo(2.75, 3.75)
          ..close();
        canvas.drawPath(p, s);
        poly([const Offset(5.75, 8), const Offset(7.4, 9.6), const Offset(10.5, 6.4)]);
      case Glyph.nodes:
        canvas.drawRRect(rr(2.25, 2.25, 13.75, 7, 1.5), s);
        canvas.drawRRect(rr(2.25, 9, 13.75, 13.75, 1.5), s);
        canvas.drawCircle(const Offset(5, 4.6), 0.9, f);
        canvas.drawCircle(const Offset(5, 11.4), 0.9, f);
        line(8, 4.6, 11.25, 4.6);
        line(8, 11.4, 11.25, 11.4);
      case Glyph.quota:
        canvas.drawArc(Rect.fromCircle(center: const Offset(8, 10), radius: 6), math.pi * 0.85,
            math.pi * 1.3, false, s);
        line(8, 10, 10.75, 6.25);
        canvas.drawCircle(const Offset(8, 10), 1.1, f);
      case Glyph.events:
        final p = Path()
          ..moveTo(2.25, 9)
          ..lineTo(4.25, 3.25)
          ..lineTo(11.75, 3.25)
          ..lineTo(13.75, 9)
          ..lineTo(13.75, 12.25)
          ..arcToPoint(const Offset(12.25, 13.75), radius: const Radius.circular(1.5))
          ..lineTo(3.75, 13.75)
          ..arcToPoint(const Offset(2.25, 12.25), radius: const Radius.circular(1.5))
          ..close();
        canvas.drawPath(p, s);
        poly([const Offset(2.25, 9), const Offset(5.5, 9), const Offset(6.5, 10.75),
            const Offset(9.5, 10.75), const Offset(10.5, 9), const Offset(13.75, 9)]);
      case Glyph.settings:
        line(2.25, 4.5, 13.75, 4.5);
        line(2.25, 11.5, 13.75, 11.5);
        for (final (x, y) in const [(10.5, 4.5), (5.5, 11.5)]) {
          canvas.drawCircle(Offset(x, y), 2, f);
        }
      case Glyph.search:
        canvas.drawCircle(const Offset(7, 7), 4.5, s);
        line(10.4, 10.4, 13.75, 13.75);
      case Glyph.chevronRight:
        poly([const Offset(6, 3.5), const Offset(10.5, 8), const Offset(6, 12.5)]);
      case Glyph.chevronDown:
        poly([const Offset(3.5, 6), const Offset(8, 10.5), const Offset(12.5, 6)]);
      case Glyph.back:
        line(13, 8, 3, 8);
        poly([const Offset(7.5, 3.5), const Offset(3, 8), const Offset(7.5, 12.5)]);
      case Glyph.send:
        line(8, 13.5, 8, 2.75);
        poly([const Offset(3.5, 7.25), const Offset(8, 2.75), const Offset(12.5, 7.25)]);
      case Glyph.interrupt:
        canvas.drawRRect(rr(4, 4, 12, 12, 1.5), f);
      case Glyph.power:
        canvas.drawArc(Rect.fromCircle(center: const Offset(8, 8.75), radius: 5.25),
            -math.pi / 2 + 0.75, math.pi * 2 - 1.5, false, s);
        line(8, 2, 8, 8);
      case Glyph.resleeve:
        final r = Rect.fromCircle(center: const Offset(8, 8), radius: 5.25);
        canvas.drawArc(r, -math.pi * 0.95, math.pi * 0.8, false, s);
        canvas.drawArc(r, math.pi * 0.05, math.pi * 0.8, false, s);
        poly([const Offset(10.5, 2.25), const Offset(12.75, 3.9), const Offset(11, 6)]);
        poly([const Offset(5.5, 13.75), const Offset(3.25, 12.1), const Offset(5, 10)]);
      case Glyph.terminal:
        canvas.drawRRect(rr(1.75, 2.75, 14.25, 13.25, 2), s);
        poly([const Offset(4.75, 6), const Offset(7, 8), const Offset(4.75, 10)]);
        line(8.5, 10.25, 11.25, 10.25);
      case Glyph.branch:
        canvas.drawCircle(const Offset(4.5, 3.75), 1.5, s);
        canvas.drawCircle(const Offset(4.5, 12.25), 1.5, s);
        canvas.drawCircle(const Offset(11.5, 5.5), 1.5, s);
        line(4.5, 5.25, 4.5, 10.75);
        final p = Path()
          ..moveTo(11.5, 7)
          ..cubicTo(11.5, 9.5, 7, 9, 4.6, 10.6);
        canvas.drawPath(p, s);
      case Glyph.check:
        poly([const Offset(3, 8.5), const Offset(6.5, 12), const Offset(13, 4.5)]);
      case Glyph.close:
        line(4, 4, 12, 12);
        line(12, 4, 4, 12);
      case Glyph.menu:
        line(2.5, 4, 13.5, 4);
        line(2.5, 8, 13.5, 8);
        line(2.5, 12, 13.5, 12);
      case Glyph.alert:
        poly([const Offset(8, 2.25), const Offset(14.25, 13.25), const Offset(1.75, 13.25)],
            close: true);
        line(8, 6.5, 8, 9.25);
        canvas.drawCircle(const Offset(8, 11.25), 0.85, f);
      case Glyph.download:
        line(8, 2.25, 8, 10);
        poly([const Offset(4.5, 6.75), const Offset(8, 10.25), const Offset(11.5, 6.75)]);
        poly([const Offset(2.5, 11), const Offset(2.5, 13.5), const Offset(13.5, 13.5),
            const Offset(13.5, 11)]);
    }
  }

  @override
  bool shouldRepaint(_GlyphPainter old) => old.glyph != glyph || old.color != color;
}
