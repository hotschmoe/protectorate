import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:fixnum/fixnum.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:protectorate/gen/protectorate/v1/quota.pb.dart';
import 'package:protectorate/screens/quota_tab.dart';
import 'package:protectorate/theme/tokens.dart';
import 'package:protectorate/vfd/lamp.dart';
import 'package:protectorate/vfd/meter.dart';
import 'package:protectorate/vfd/panel.dart';
import 'package:protobuf/well_known_types/google/protobuf/timestamp.pb.dart';

Widget _host(Widget child, {bool reduced = false, bool dark = true}) =>
    MaterialApp(
      theme: appTheme(dark ? Brightness.dark : Brightness.light),
      home: MediaQuery(
        data: MediaQueryData(disableAnimations: reduced),
        child: Scaffold(
          body: Center(child: SizedBox(width: 300, child: child)),
        ),
      ),
    );

MeterPainter _painter(WidgetTester t) => t
    .widgetList<CustomPaint>(
      find.descendant(
        of: find.byType(SegmentMeter),
        matching: find.byType(CustomPaint),
      ),
    )
    .map((c) => c.painter)
    .whereType<MeterPainter>()
    .single;

void main() {
  test('lit segment count for a value', () {
    for (final (pct, want) in [
      (null, 0),
      (0.0, 0),
      (2.0, 0),
      (3.0, 1),
      (50.0, 10),
      (82.0, 16),
      (100.0, 20),
      (140.0, 20),
      (-5.0, 0),
    ]) {
      expect(litSegments(pct, 20), want, reason: '$pct');
    }
    expect(litSegments(50, 12), 6);
  });

  test('segment zones follow the warn and critical thresholds', () {
    final zones = [for (var i = 0; i < 20; i++) zoneTone(i, 20)];
    expect(zones.sublist(0, 15).toSet(), {Tone.lit});
    expect(zones.sublist(15, 18).toSet(), {Tone.warn});
    expect(zones.sublist(18).toSet(), {Tone.alarm});
  });

  test('level tone and readout helpers', () {
    const t = Tokens.vfd;
    expect(t.levelTone(null), Tone.dim);
    expect(t.levelTone(74.9), Tone.lit);
    expect(t.levelTone(75), Tone.warn);
    expect(t.levelTone(90), Tone.alarm);
    expect(t.p.warn, isNot(t.p.lit));
    expect(ghostDigits('12.5'), '88.8');
    expect(ghostDigits('-'), '8');
    expect(countdownParts(const Duration(hours: 2, minutes: 5)), [
      ('2', 'H'),
      ('05', 'M'),
    ]);
    expect(countdownParts(const Duration(days: 3, hours: 4)), [
      ('3', 'D'),
      ('04', 'H'),
    ]);
    expect(countdownParts(const Duration(seconds: -1)), [('--', 'DUE')]);
  });

  testWidgets('meter fills to the value with threshold colours', (t) async {
    await t.pumpWidget(_host(const SegmentMeter(percent: 95)));
    await t.pump(Tokens.fill);
    final p = _painter(t);
    expect(p.lit, 19);
    expect(p.litColor(0), Tokens.vfd.p.lit);
    expect(p.litColor(16), Tokens.vfd.p.warn);
    expect(p.litColor(18), Tokens.vfd.p.alarm);
  });

  testWidgets('stale meter dims every lit segment', (t) async {
    await t.pumpWidget(_host(const SegmentMeter(percent: 95, dim: true)));
    await t.pump(Tokens.fill);
    expect(_painter(t).litColor(18), Tokens.vfd.p.dim);
  });

  testWidgets('unknown meter lights nothing', (t) async {
    await t.pumpWidget(_host(const SegmentMeter(percent: null)));
    final p = _painter(t);
    expect(p.percent, isNull);
    expect(p.lit, 0);
    expect(
      t.getSemantics(find.byType(SegmentMeter)),
      matchesSemantics(value: 'unknown'),
    );
  });

  testWidgets('meter animates its fill, or jumps with reduced motion', (
    t,
  ) async {
    await t.pumpWidget(_host(const SegmentMeter(percent: 100)));
    await t.pump(const Duration(milliseconds: 60));
    expect(_painter(t).lit, inExclusiveRange(0, 20));
    await t.pump(Tokens.fill);
    expect(_painter(t).lit, 20);

    await t.pumpWidget(const SizedBox());
    await t.pumpWidget(_host(const SegmentMeter(percent: 100), reduced: true));
    expect(_painter(t).lit, 20);
  });

  testWidgets('meter row value never truncates', (t) async {
    const v = '1.66 GB / 4.00 GB';
    await t.pumpWidget(
      _host(const MeterRow(label: 'MEM', percent: 41, value: v)),
    );
    final para = t.renderObject<RenderParagraph>(find.text(v));
    expect(para.didExceedMaxLines, isFalse);
    expect(
      para.size.width,
      greaterThanOrEqualTo(para.getMaxIntrinsicWidth(double.infinity) - 0.5),
    );
  });

  testWidgets('attention lamp blinks, and holds a ring with reduced motion', (
    t,
  ) async {
    await t.pumpWidget(_host(const Lamp(tone: Tone.warn, blink: true)));
    await t.pump(const Duration(milliseconds: 100));
    expect(t.binding.hasScheduledFrame, isTrue);

    await t.pumpWidget(const SizedBox());
    await t.pumpWidget(
      _host(const Lamp(tone: Tone.warn, blink: true), reduced: true),
    );
    await t.pump();
    expect(t.binding.hasScheduledFrame, isFalse);
    final ring = find.descendant(
      of: find.byType(Lamp),
      matching: find.byWidgetPredicate(
        (w) =>
            w is Container &&
            w.decoration is BoxDecoration &&
            (w.decoration! as BoxDecoration).border != null &&
            (w.decoration! as BoxDecoration).color == null,
      ),
    );
    expect(ring, findsOneWidget);
  });

  test('only multi-day windows resetting within a day count as soon', () {
    final now = DateTime.utc(2026, 10, 20, 12);
    QuotaWindow w(String name, int windowHours, Duration? left) => QuotaWindow(
      name: name,
      windowSeconds: Int64(windowHours * 3600),
      resetsAt: left == null ? null : Timestamp.fromDateTime(now.add(left)),
    );
    for (final (win, want) in [
      (w('7d', 168, const Duration(hours: 20)), true),
      (w('7d fable', 168, const Duration(hours: 24)), true),
      (w('7d', 168, const Duration(hours: 25)), false),
      (w('5h', 5, const Duration(hours: 2)), false),
      (w('24h', 24, const Duration(hours: 3)), false),
      (w('7d', 168, const Duration(minutes: -5)), false),
      (w('7d', 168, null), false),
      (w('weekly', 0, const Duration(hours: 6)), true),
      (w('5h', 0, const Duration(hours: 1)), false),
    ]) {
      expect(resetsSoon(win, now), want, reason: '${win.name} $win');
    }
  });

  testWidgets('a new beat flickers the lamp once; reduced motion does not', (
    t,
  ) async {
    Color? fill() => t
        .widgetList<Container>(
          find.descendant(
            of: find.byType(Lamp),
            matching: find.byType(Container),
          ),
        )
        .map((c) => c.decoration)
        .whereType<BoxDecoration>()
        .map((d) => d.color)
        .whereType<Color>()
        .first;

    await t.pumpWidget(_host(const Lamp(tone: Tone.ok, beat: 1)));
    final steady = fill();
    await t.pumpWidget(_host(const Lamp(tone: Tone.ok, beat: 2)));
    await t.pump(Tokens.beat * 0.1);
    expect(fill(), isNot(steady));
    await t.pumpAndSettle();
    expect(fill(), steady);

    await t.pumpWidget(
      _host(const Lamp(tone: Tone.ok, beat: 3), reduced: true),
    );
    await t.pumpWidget(
      _host(const Lamp(tone: Tone.ok, beat: 4), reduced: true),
    );
    await t.pump(Tokens.beat * 0.1);
    expect(fill(), steady);
  });

  testWidgets('steady lamp schedules no frames', (t) async {
    await t.pumpWidget(_host(const Lamp(tone: Tone.ok)));
    await t.pump();
    expect(t.binding.hasScheduledFrame, isFalse);
  });

  testWidgets('readout exposes its value to accessibility', (t) async {
    await t.pumpWidget(
      _host(
        SegmentReadout.parts(
          parts: const [('2', 'H'), ('14', 'M')],
          size: Tokens.readoutM,
        ),
      ),
    );
    expect(find.bySemanticsLabel('2 H 14 M'), findsOneWidget);
  });

  testWidgets('light theme meters use ink, not phosphor', (t) async {
    await t.pumpWidget(_host(const SegmentMeter(percent: 50), dark: false));
    await t.pump(Tokens.fill);
    expect(_painter(t).litColor(0), Tokens.printout.p.lit);
  });
}
