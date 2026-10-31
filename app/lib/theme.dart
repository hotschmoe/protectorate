import 'package:flutter/material.dart';

/// Colour tokens from docs/spec-page/2026-11-01-next-spec.html. Colour carries meaning only:
/// lit text, dim secondary, info selection and links, warn waiting-on-you, alarm failed or
/// unreachable, ok only for a status word.
class Palette {
  const Palette({
    required this.dark,
    required this.bg,
    required this.panel,
    required this.bar,
    required this.lit,
    required this.fg,
    required this.dim,
    required this.rule,
    required this.edge,
    required this.accent,
    required this.hotFill,
    required this.info,
    required this.warn,
    required this.alarm,
    required this.ok,
  });

  final bool dark;
  final Color bg, panel, bar, lit, fg, dim, rule, edge, accent, hotFill;
  final Color info, warn, alarm, ok;

  static const amber = Palette(
    dark: true,
    bg: Color(0xFF0E0C09),
    panel: Color(0xFF16120C),
    bar: Color(0xFF13100B),
    lit: Color(0xFFFFB547),
    fg: Color(0xFFECD7A9),
    dim: Color(0xFFA08D69),
    rule: Color(0xFF4C3F28),
    edge: Color(0xFF85714C),
    accent: Color(0xFFFFB547),
    hotFill: Color(0x21FFB547),
    info: Color(0xFF86CBD9),
    warn: Color(0xFFFFD84A),
    alarm: Color(0xFFFF7A57),
    ok: Color(0xFFA6D884),
  );

  /// The line-printer rendering: buff paper, green bars, black and red ribbon.
  static const printer = Palette(
    dark: false,
    bg: Color(0xFFF5EDD6),
    panel: Color(0xFFECE2C4),
    bar: Color(0xFFE6EFD9),
    lit: Color(0xFF1B1813),
    fg: Color(0xFF2A261D),
    dim: Color(0xFF6C634E),
    rule: Color(0xFFC4B48B),
    edge: Color(0xFF857451),
    accent: Color(0xFFB3261E),
    hotFill: Color(0x14B3261E),
    info: Color(0xFF1C4F86),
    warn: Color(0xFF8F6100),
    alarm: Color(0xFFB3261E),
    ok: Color(0xFF2D6A28),
  );
}

/// One typeface, one size. A cell is 0.6 em wide (JetBrains Mono advance) and rows are 20 px.
const String kFont = 'JetBrainsMono';
const double kFontSize = 13;
const double kRow = 20;
const double kCell = kFontSize * 0.6;

/// Below this width the phone layout is used.
const double kPhoneWidth = 720;

class Term extends InheritedWidget {
  const Term({super.key, required this.p, required this.phone, required super.child});

  final Palette p;
  final bool phone;

  static Term of(BuildContext context) {
    final t = context.dependOnInheritedWidgetOfExactType<Term>();
    assert(t != null, 'no Term above this widget');
    return t!;
  }

  @override
  bool updateShouldNotify(Term old) => old.p != p || old.phone != phone;
}

Palette pal(BuildContext context) => Term.of(context).p;

TextStyle baseStyle(Palette p) => TextStyle(
  fontFamily: kFont,
  fontFamilyFallback: const ['monospace'],
  fontSize: kFontSize,
  height: kRow / kFontSize,
  color: p.fg,
  fontFeatures: const [FontFeature.tabularFigures()],
  leadingDistribution: TextLeadingDistribution.even,
);

ThemeData buildTheme(Palette p) {
  final text = baseStyle(p);
  final tt = TextTheme(
    displayLarge: text,
    displayMedium: text,
    displaySmall: text,
    headlineLarge: text,
    headlineMedium: text,
    headlineSmall: text,
    titleLarge: text,
    titleMedium: text,
    titleSmall: text,
    bodyLarge: text,
    bodyMedium: text,
    bodySmall: text,
    labelLarge: text,
    labelMedium: text,
    labelSmall: text,
  );
  return ThemeData(
    brightness: p.dark ? Brightness.dark : Brightness.light,
    useMaterial3: true,
    fontFamily: kFont,
    scaffoldBackgroundColor: p.bg,
    canvasColor: p.bg,
    colorScheme: ColorScheme(
      brightness: p.dark ? Brightness.dark : Brightness.light,
      primary: p.lit,
      onPrimary: p.bg,
      secondary: p.info,
      onSecondary: p.bg,
      error: p.alarm,
      onError: p.bg,
      surface: p.bg,
      onSurface: p.fg,
    ),
    textTheme: tt,
    primaryTextTheme: tt,
    splashFactory: NoSplash.splashFactory,
    highlightColor: Colors.transparent,
    splashColor: Colors.transparent,
    hoverColor: p.hotFill,
    focusColor: p.hotFill,
    visualDensity: VisualDensity.compact,
    materialTapTargetSize: MaterialTapTargetSize.shrinkWrap,
    textSelectionTheme: TextSelectionThemeData(
      cursorColor: p.lit,
      selectionColor: p.dark ? const Color(0x55FFB547) : const Color(0x33B3261E),
      selectionHandleColor: p.lit,
    ),
    inputDecorationTheme: const InputDecorationTheme(
      isDense: true,
      border: InputBorder.none,
      contentPadding: EdgeInsets.zero,
    ),
    scrollbarTheme: ScrollbarThemeData(
      thickness: const WidgetStatePropertyAll(6),
      radius: Radius.zero,
      thumbColor: WidgetStatePropertyAll(p.rule),
      crossAxisMargin: 1,
      mainAxisMargin: 0,
    ),
    tooltipTheme: TooltipThemeData(
      decoration: BoxDecoration(color: p.panel, border: Border.all(color: p.edge)),
      textStyle: text.copyWith(color: p.fg),
      waitDuration: const Duration(milliseconds: 600),
    ),
  );
}
