import 'package:flutter/material.dart';

/// Colour tokens. Amber is the accent and the brand; every other hue carries a meaning: info for
/// links and selection, ok for healthy or active, warn for waiting on you, alarm for failed or
/// unreachable. Surfaces step up from bg through surface to raised; line and edge are borders.
class Palette {
  const Palette({
    required this.dark,
    required this.bg,
    required this.side,
    required this.surface,
    required this.raised,
    required this.line,
    required this.edge,
    required this.fg,
    required this.dim,
    required this.faint,
    required this.accent,
    required this.onAccent,
    required this.info,
    required this.ok,
    required this.warn,
    required this.onWarn,
    required this.alarm,
  });

  final bool dark;
  final Color bg, side, surface, raised, line, edge;
  final Color fg, dim, faint;
  final Color accent, onAccent;
  final Color info, ok, warn, onWarn, alarm;

  /// A colour laid over the surface at low strength, for fills behind chips, rows and banners.
  Color tint(Color c, [double a = 0.12]) => c.withValues(alpha: dark ? a : a * 0.8);

  Color get hover => fg.withValues(alpha: dark ? 0.05 : 0.045);
  Color get press => fg.withValues(alpha: dark ? 0.09 : 0.08);

  static const amber = Palette(
    dark: true,
    bg: Color(0xFF111214),
    side: Color(0xFF0B0C0E),
    surface: Color(0xFF17181B),
    raised: Color(0xFF1E2024),
    line: Color(0xFF26282D),
    edge: Color(0xFF363940),
    fg: Color(0xFFECE6DA),
    dim: Color(0xFFA39D92),
    faint: Color(0xFF6F6B64),
    accent: Color(0xFFFFB52E),
    onAccent: Color(0xFF1B1204),
    info: Color(0xFF6DB8F2),
    ok: Color(0xFF74C69D),
    warn: Color(0xFFF5D35A),
    onWarn: Color(0xFF1E1803),
    alarm: Color(0xFFFF6B4A),
  );

  static const paper = Palette(
    dark: false,
    bg: Color(0xFFFAF9F6),
    side: Color(0xFFF1EFE9),
    surface: Color(0xFFFFFFFF),
    raised: Color(0xFFF2F0EA),
    line: Color(0xFFE3DFD5),
    edge: Color(0xFFCCC6B8),
    fg: Color(0xFF1E1B16),
    dim: Color(0xFF625C51),
    faint: Color(0xFF958E81),
    accent: Color(0xFFB56A00),
    onAccent: Color(0xFFFFFFFF),
    info: Color(0xFF1D62AB),
    ok: Color(0xFF2A7A48),
    warn: Color(0xFF9A6200),
    onWarn: Color(0xFFFFFFFF),
    alarm: Color(0xFFC0361C),
  );
}

/// Inter for the interface; JetBrains Mono for code, transcripts of tools, ids and output.
const String kSans = 'Inter';
const String kMono = 'JetBrainsMono';

/// Type scale.
const double kSmall = 12;
const double kBase = 13;
const double kBody = 14;
const double kTitle = 16;

/// Radii and the sidebar.
const double kRadius = 6;
const double kCardRadius = 8;
const double kSidebar = 248;

/// Below this width the phone layout is used.
const double kPhoneWidth = 720;

class Look extends InheritedWidget {
  const Look({super.key, required this.p, required this.phone, required super.child});

  final Palette p;
  final bool phone;

  static Look of(BuildContext context) {
    final t = context.dependOnInheritedWidgetOfExactType<Look>();
    assert(t != null, 'no Look above this widget');
    return t!;
  }

  @override
  bool updateShouldNotify(Look old) => old.p != p || old.phone != phone;
}

Palette pal(BuildContext context) => Look.of(context).p;

const _tabular = [FontFeature.tabularFigures()];

TextStyle sansStyle(Palette p, {double size = kBase, FontWeight weight = FontWeight.w400,
    Color? color, double? height}) => TextStyle(
  fontFamily: kSans,
  fontFamilyFallback: const ['sans-serif'],
  fontSize: size,
  fontWeight: weight,
  height: height ?? 1.4,
  color: color ?? p.fg,
  letterSpacing: size >= kTitle ? -0.2 : 0,
  fontFeatures: _tabular,
  leadingDistribution: TextLeadingDistribution.even,
);

TextStyle monoStyle(Palette p, {double size = 12.5, FontWeight weight = FontWeight.w400,
    Color? color, double? height}) => TextStyle(
  fontFamily: kMono,
  fontFamilyFallback: const ['monospace'],
  fontSize: size,
  fontWeight: weight,
  height: height ?? 1.5,
  color: color ?? p.fg,
  fontFeatures: const [
    FontFeature.tabularFigures(),
    FontFeature.disable('calt'),
    FontFeature.disable('liga'),
  ],
  leadingDistribution: TextLeadingDistribution.even,
);

ThemeData buildTheme(Palette p) {
  final text = sansStyle(p);
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
  final brightness = p.dark ? Brightness.dark : Brightness.light;
  return ThemeData(
    brightness: brightness,
    useMaterial3: true,
    fontFamily: kSans,
    scaffoldBackgroundColor: p.bg,
    canvasColor: p.bg,
    colorScheme: ColorScheme(
      brightness: brightness,
      primary: p.accent,
      onPrimary: p.onAccent,
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
    hoverColor: p.hover,
    focusColor: p.tint(p.accent, 0.16),
    visualDensity: VisualDensity.compact,
    materialTapTargetSize: MaterialTapTargetSize.shrinkWrap,
    dividerColor: p.line,
    textSelectionTheme: TextSelectionThemeData(
      cursorColor: p.accent,
      selectionColor: p.accent.withValues(alpha: p.dark ? 0.28 : 0.22),
      selectionHandleColor: p.accent,
    ),
    inputDecorationTheme: const InputDecorationTheme(
      isDense: true,
      border: InputBorder.none,
      contentPadding: EdgeInsets.zero,
    ),
    scrollbarTheme: ScrollbarThemeData(
      thickness: const WidgetStatePropertyAll(6),
      radius: const Radius.circular(3),
      thumbColor: WidgetStatePropertyAll(p.edge),
      crossAxisMargin: 2,
      mainAxisMargin: 2,
    ),
    tooltipTheme: TooltipThemeData(
      decoration: BoxDecoration(
        color: p.dark ? const Color(0xFF26282D) : const Color(0xFF2A2721),
        borderRadius: BorderRadius.circular(5),
        boxShadow: const [BoxShadow(color: Color(0x40000000), blurRadius: 8, offset: Offset(0, 2))],
      ),
      textStyle: sansStyle(p, size: kSmall, color: const Color(0xFFF2EEE6)),
      padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 5),
      waitDuration: const Duration(milliseconds: 500),
    ),
    drawerTheme: DrawerThemeData(
      backgroundColor: p.side,
      shape: const RoundedRectangleBorder(),
      width: 288,
    ),
  );
}
