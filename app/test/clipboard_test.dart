import 'package:flutter_test/flutter_test.dart';
import 'package:protectorate/clipboard.dart';

void main() {
  final cases =
      <
        ({
          String name,
          bool secure,
          bool setDataThrows,
          bool legacyOk,
          bool want,
          List<String> calls,
        })
      >[
        (
          name: 'secure uses the async api',
          secure: true,
          setDataThrows: false,
          legacyOk: true,
          want: true,
          calls: ['set'],
        ),
        (
          name: 'insecure skips the async api',
          secure: false,
          setDataThrows: false,
          legacyOk: true,
          want: true,
          calls: ['legacy'],
        ),
        (
          name: 'secure but throws falls back',
          secure: true,
          setDataThrows: true,
          legacyOk: true,
          want: true,
          calls: ['set', 'legacy'],
        ),
        (
          name: 'both fail',
          secure: false,
          setDataThrows: false,
          legacyOk: false,
          want: false,
          calls: ['legacy'],
        ),
        (
          name: 'secure throws and legacy fails',
          secure: true,
          setDataThrows: true,
          legacyOk: false,
          want: false,
          calls: ['set', 'legacy'],
        ),
      ];
  for (final c in cases) {
    test(c.name, () async {
      final calls = <String>[];
      final got = await copyWithFallback(
        'x',
        secureContext: c.secure,
        setData: (_) async {
          calls.add('set');
          if (c.setDataThrows) throw StateError('no clipboard');
        },
        legacyCopy: (_) {
          calls.add('legacy');
          return c.legacyOk;
        },
      );
      expect(got, c.want);
      expect(calls, c.calls);
    });
  }
}
