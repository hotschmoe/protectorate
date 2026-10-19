import 'package:flutter/material.dart';

import '../gen/protectorate/v1/chat.pb.dart';
import '../gen/protectorate/v1/sleeve.pb.dart';

const monoFamily = 'JetBrains Mono';
const segmentFamily = 'DSEG7 Classic';

const warnPercent = 75.0;
const critPercent = 90.0;

// Semantic colour roles; docs/design.md maps each to its meaning.
enum Tone { lit, dim, ghost, emph, info, warn, alarm, ok }

class Palette {
  const Palette({
    required this.chassis,
    required this.glass,
    required this.frame,
    required this.bezel,
    required this.bezelHi,
    required this.rule,
    required this.bar,
    required this.legend,
    required this.lit,
    required this.dim,
    required this.ghost,
    required this.emph,
    required this.info,
    required this.warn,
    required this.alarm,
    required this.ok,
    required this.onKey,
    required this.shadow,
    required this.ansi,
  });

  final Color chassis;
  final Color glass;
  // The bezel face the legends are printed on.
  final Color frame;
  final Color bezel;
  final Color bezelHi;
  final Color rule;
  final Color bar;
  final Color legend;
  final Color lit;
  final Color dim;
  final Color ghost;
  final Color emph;
  final Color info;
  final Color warn;
  final Color alarm;
  final Color ok;
  final Color onKey;
  final Color shadow;
  // black red green yellow blue magenta cyan white, then the bright set.
  final List<Color> ansi;

  static const vfd = Palette(
    chassis: Color(0xFF0E1013),
    glass: Color(0xFF07080A),
    frame: Color(0xFF181B20),
    bezel: Color(0xFF23272D),
    bezelHi: Color(0xFF353B43),
    rule: Color(0xFF2B2417),
    bar: Color(0xFF0C0D10),
    legend: Color(0xFF8E97A1),
    lit: Color(0xFFFFB52E),
    dim: Color(0xFFC08A2E),
    ghost: Color(0xFF2A1F0E),
    emph: Color(0xFFE8F4FF),
    info: Color(0xFF5CC8FF),
    warn: Color(0xFFFFE45C),
    alarm: Color(0xFFFF5A1F),
    ok: Color(0xFF7CC793),
    onKey: Color(0xFF07080A),
    shadow: Color(0xFF000000),
    ansi: [
      Color(0xFF1A1C20),
      Color(0xFFFF5A1F),
      Color(0xFF8FBF8A),
      Color(0xFFFFB52E),
      Color(0xFF5CC8FF),
      Color(0xFFFF7AB0),
      Color(0xFF7FDDEA),
      Color(0xFFD8E4EE),
      Color(0xFF5D646D),
      Color(0xFFFF7A45),
      Color(0xFFA9D6A0),
      Color(0xFFFFE45C),
      Color(0xFF8ED8FF),
      Color(0xFFFF9CC4),
      Color(0xFFA6ECF5),
      Color(0xFFF4FAFF),
    ],
  );

  static const printout = Palette(
    chassis: Color(0xFFEFE9DC),
    glass: Color(0xFFFBF9F3),
    frame: Color(0xFFF4EBD7),
    bezel: Color(0xFFD6CDB9),
    bezelHi: Color(0xFFFFFFFF),
    rule: Color(0xFFDED4C0),
    bar: Color(0xFFF4EBD7),
    legend: Color(0xFF6E6352),
    lit: Color(0xFF2A2118),
    dim: Color(0xFF6B5D4A),
    ghost: Color(0xFFE7DECB),
    emph: Color(0xFF17110A),
    info: Color(0xFF1D5FA6),
    warn: Color(0xFF965600),
    alarm: Color(0xFFB8321A),
    ok: Color(0xFF2A6E42),
    onKey: Color(0xFFFBF9F3),
    shadow: Color(0xFF6E5C3A),
    ansi: [
      Color(0xFF2A2118),
      Color(0xFFB8321A),
      Color(0xFF2A6E42),
      Color(0xFF965600),
      Color(0xFF1D5FA6),
      Color(0xFF8E2F6B),
      Color(0xFF16707E),
      Color(0xFF8A8070),
      Color(0xFF6B5D4A),
      Color(0xFFD2401F),
      Color(0xFF368553),
      Color(0xFFB06A00),
      Color(0xFF2B74C4),
      Color(0xFFA83C80),
      Color(0xFF1C8899),
      Color(0xFF4A4034),
    ],
  );
}

class Accent {
  const Accent(this.fg, this.bg);
  final Color fg;
  final Color bg;
}

// Every colour, metric and text style the widgets use lives here, so a
// restyle edits this file and not the widgets.
class Tokens {
  const Tokens._(this.p, this.dark);

  static const vfd = Tokens._(Palette.vfd, true);
  static const printout = Tokens._(Palette.printout, false);

  factory Tokens.of(BuildContext context) =>
      Theme.of(context).brightness == Brightness.dark ? vfd : printout;

  factory Tokens.forBrightness(Brightness b) =>
      b == Brightness.dark ? vfd : printout;

  final Palette p;
  final bool dark;

  static const gapS = 8.0;
  static const gap = 12.0;
  static const gapL = 16.0;
  static const gapXL = 24.0;
  static const bezelRadius = 10.0;
  static const innerRadius = 3.0;
  static const contentMaxWidth = 760.0;
  static const boardMaxWidth = 1320.0;
  static const touch = 48.0;
  static const meterHeight = 10.0;
  static const segmentGap = 2.0;
  static const segments = 20;
  static const lampSize = 10.0;
  static const glowMinSize = 13.0;
  static const fill = Duration(milliseconds: 260);
  static const warmUp = Duration(milliseconds: 450);
  static const blink = Duration(milliseconds: 1600);
  static const chase = Duration(milliseconds: 1100);
  // JetBrains Mono advance width per em.
  static const monoAdvance = 0.6;

  Color tone(Tone t) => switch (t) {
    Tone.lit => p.lit,
    Tone.dim => p.dim,
    Tone.ghost => p.ghost,
    Tone.emph => p.emph,
    Tone.info => p.info,
    Tone.warn => p.warn,
    Tone.alarm => p.alarm,
    Tone.ok => p.ok,
  };

  // Unlit segment tint for each zone, so thresholds read before they are hit.
  Color ghostFor(Tone t) => switch (t) {
    Tone.warn => Color.alphaBlend(p.warn.withValues(alpha: 0.16), p.glass),
    Tone.alarm => Color.alphaBlend(p.alarm.withValues(alpha: 0.18), p.glass),
    _ => p.ghost,
  };

  Color tint(Color c) => c.withValues(alpha: dark ? 0.12 : 0.10);

  List<Shadow> glow(Color c, {double size = 14, double strength = 1}) =>
      !dark || size < glowMinSize
      ? const []
      : [
          Shadow(color: c.withValues(alpha: 0.45 * strength), blurRadius: 6),
          Shadow(color: c.withValues(alpha: 0.25 * strength), blurRadius: 14),
        ];

  List<BoxShadow> boxGlow(Color c, {double radius = 6}) => !dark
      ? const []
      : [
          BoxShadow(
            color: c.withValues(alpha: 0.55),
            blurRadius: radius,
            spreadRadius: 0.5,
          ),
        ];

  List<BoxShadow> get bezelShadow => [
    BoxShadow(
      color: p.shadow.withValues(alpha: dark ? 0.55 : 0.14),
      blurRadius: dark ? 18 : 10,
      offset: const Offset(0, 4),
    ),
  ];

  TextStyle legend([Color? c]) => TextStyle(
    fontFamily: monoFamily,
    fontSize: 11,
    fontWeight: FontWeight.w700,
    letterSpacing: 1.4,
    height: 1.2,
    color: c ?? p.legend,
  );

  TextStyle title([Color? c]) {
    final color = c ?? p.emph;
    return TextStyle(
      fontFamily: monoFamily,
      fontSize: 16,
      fontWeight: FontWeight.w700,
      letterSpacing: 0.4,
      height: 1.25,
      color: color,
      shadows: glow(color, size: 16, strength: 0.6),
    );
  }

  TextStyle body([Color? c]) => TextStyle(
    fontFamily: monoFamily,
    fontSize: 14,
    height: 1.5,
    color: c ?? p.lit,
  );

  TextStyle data([Color? c]) => TextStyle(
    fontFamily: monoFamily,
    fontSize: 13,
    height: 1.3,
    color: c ?? p.lit,
    fontFeatures: const [FontFeature.tabularFigures()],
  );

  TextStyle small([Color? c]) => TextStyle(
    fontFamily: monoFamily,
    fontSize: 12.5,
    height: 1.35,
    color: c ?? p.dim,
  );

  TextStyle readout(double size, [Color? c]) {
    final color = c ?? p.emph;
    return TextStyle(
      fontFamily: segmentFamily,
      fontSize: size,
      height: 1.1,
      color: color,
      shadows: glow(color, size: size),
    );
  }

  static const readoutXL = 40.0;
  static const readoutL = 28.0;
  static const readoutM = 20.0;
  static const readoutS = 15.0;

  Tone levelTone(double? percent) {
    if (percent == null) return Tone.dim;
    if (percent >= critPercent) return Tone.alarm;
    if (percent >= warnPercent) return Tone.warn;
    return Tone.lit;
  }

  Color level(double? percent) => tone(levelTone(percent));

  Accent _accent(Color darkFg, Color lightFg) {
    final c = dark ? darkFg : lightFg;
    return Accent(c, tint(c));
  }

  Accent harness(Harness h) => switch (h) {
    Harness.HARNESS_CLAUDE => _accent(
      const Color(0xFFFF9447),
      const Color(0xFFA9481A),
    ),
    Harness.HARNESS_CODEX => _accent(
      const Color(0xFF6FCBFF),
      const Color(0xFF1C5E9E),
    ),
    Harness.HARNESS_GROK => _accent(
      const Color(0xFFFF7AB0),
      const Color(0xFF9E2C62),
    ),
    Harness.HARNESS_PI => _accent(
      const Color(0xFFE8DAB0),
      const Color(0xFF6E5A1E),
    ),
    _ => _accent(const Color(0xFFA8B4C0), const Color(0xFF4F5B66)),
  };

  // Colour of the party serving the model: the provider when known, the
  // harness otherwise. A model served locally gets its own colour whatever
  // the provider id is.
  Accent model(Harness h, String provider, {bool local = false}) {
    if (local) {
      return _accent(const Color(0xFF8FE3EE), const Color(0xFF1C6E78));
    }
    final p = provider.toLowerCase();
    bool is_(String prefix) => p.startsWith(prefix);
    if (is_('anthropic')) return harness(Harness.HARNESS_CLAUDE);
    if (is_('openai')) return harness(Harness.HARNESS_CODEX);
    if (is_('xai')) return harness(Harness.HARNESS_GROK);
    if (is_('deepseek')) {
      return _accent(const Color(0xFF8FA8FF), const Color(0xFF2A47B0));
    }
    if (is_('zai') || is_('z-ai') || is_('zhipu')) {
      return _accent(const Color(0xFFB8A4FF), const Color(0xFF5636B0));
    }
    if (is_('moonshot') || is_('kimi')) {
      return _accent(const Color(0xFF9ED6FF), const Color(0xFF1F6596));
    }
    if (is_('minimax')) {
      return _accent(const Color(0xFFFF8E70), const Color(0xFFA63C1C));
    }
    if (is_('qwen') || is_('alibaba') || is_('dashscope')) {
      return _accent(const Color(0xFFD8A6FF), const Color(0xFF7234A6));
    }
    if (is_('openrouter')) {
      return _accent(const Color(0xFFA9BCEB), const Color(0xFF3A5490));
    }
    return harness(h);
  }

  Tone chatTone(ChatState s) => switch (s) {
    ChatState.CHAT_STATE_AWAITING_APPROVAL => Tone.warn,
    ChatState.CHAT_STATE_WORKING => Tone.info,
    ChatState.CHAT_STATE_COMPACTING => Tone.info,
    ChatState.CHAT_STATE_IDLE => Tone.ok,
    ChatState.CHAT_STATE_FAILED => Tone.alarm,
    ChatState.CHAT_STATE_EXITED => Tone.ghost,
    _ => Tone.dim,
  };
}

ThemeData appTheme(Brightness brightness) {
  final t = Tokens.forBrightness(brightness);
  final p = t.p;
  final scheme = ColorScheme(
    brightness: brightness,
    primary: p.lit,
    onPrimary: p.onKey,
    primaryContainer: t.tint(p.lit),
    onPrimaryContainer: p.lit,
    secondary: p.info,
    onSecondary: p.onKey,
    secondaryContainer: t.tint(p.info),
    onSecondaryContainer: p.info,
    tertiary: p.warn,
    onTertiary: p.onKey,
    tertiaryContainer: t.tint(p.warn),
    onTertiaryContainer: p.warn,
    error: p.alarm,
    onError: p.onKey,
    errorContainer: t.tint(p.alarm),
    onErrorContainer: p.alarm,
    surface: p.chassis,
    onSurface: p.lit,
    onSurfaceVariant: p.dim,
    surfaceContainerLowest: p.glass,
    surfaceContainerLow: p.glass,
    surfaceContainer: p.glass,
    surfaceContainerHigh: p.bar,
    surfaceContainerHighest: p.bar,
    outline: p.bezelHi,
    outlineVariant: p.bezel,
    shadow: p.shadow,
    scrim: p.shadow,
    inverseSurface: p.emph,
    onInverseSurface: p.glass,
    inversePrimary: p.lit,
    surfaceTint: Colors.transparent,
  );
  const square = RoundedRectangleBorder(
    borderRadius: BorderRadius.all(Radius.circular(Tokens.innerRadius)),
  );
  final keyText = t.legend().copyWith(fontSize: 12.5);
  final frame = RoundedRectangleBorder(
    borderRadius: const BorderRadius.all(Radius.circular(Tokens.bezelRadius)),
    side: BorderSide(color: p.bezelHi),
  );
  OutlineInputBorder field(Color c, [double w = 1]) => OutlineInputBorder(
    borderRadius: const BorderRadius.all(Radius.circular(Tokens.innerRadius)),
    borderSide: BorderSide(color: c, width: w),
  );
  final base = ThemeData(
    colorScheme: scheme,
    fontFamily: monoFamily,
    useMaterial3: true,
    brightness: brightness,
  );
  return base.copyWith(
    scaffoldBackgroundColor: p.chassis,
    canvasColor: p.chassis,
    splashFactory: NoSplash.splashFactory,
    hoverColor: p.lit.withValues(alpha: 0.06),
    highlightColor: p.lit.withValues(alpha: 0.08),
    focusColor: p.info.withValues(alpha: 0.16),
    dividerColor: p.rule,
    textTheme: base.textTheme
        .copyWith(
          titleLarge: t.title().copyWith(fontSize: 18, shadows: const []),
          titleMedium: t.title().copyWith(shadows: const []),
          titleSmall: t.data().copyWith(fontWeight: FontWeight.w700),
          bodyLarge: t.body(),
          bodyMedium: t.body(),
          bodySmall: t.small(),
          labelLarge: t.data().copyWith(fontWeight: FontWeight.w700),
          labelMedium: t.small(),
          labelSmall: t.legend(),
        )
        .apply(fontFamily: monoFamily),
    appBarTheme: AppBarTheme(
      backgroundColor: p.chassis,
      foregroundColor: p.lit,
      elevation: 0,
      scrolledUnderElevation: 0,
      surfaceTintColor: Colors.transparent,
      toolbarHeight: 56,
      titleSpacing: Tokens.gapL,
      titleTextStyle: t.title().copyWith(fontSize: 18),
      iconTheme: IconThemeData(color: p.dim, size: 22),
      actionsIconTheme: IconThemeData(color: p.dim, size: 22),
      shape: Border(bottom: BorderSide(color: p.bezel)),
    ),
    iconTheme: IconThemeData(color: p.dim, size: 20),
    cardTheme: CardThemeData(
      color: p.glass,
      elevation: 0,
      margin: EdgeInsets.zero,
      shape: frame,
    ),
    dialogTheme: DialogThemeData(
      backgroundColor: p.glass,
      surfaceTintColor: Colors.transparent,
      shape: frame,
      titleTextStyle: t.title(),
      contentTextStyle: t.body(),
    ),
    popupMenuTheme: PopupMenuThemeData(
      color: p.glass,
      surfaceTintColor: Colors.transparent,
      shape: frame,
      textStyle: t.data(),
      labelTextStyle: WidgetStateProperty.resolveWith(
        (s) => t.data(s.contains(WidgetState.disabled) ? p.ghost : p.lit),
      ),
    ),
    snackBarTheme: SnackBarThemeData(
      backgroundColor: p.glass,
      contentTextStyle: t.data(p.alarm),
      behavior: SnackBarBehavior.floating,
      shape: RoundedRectangleBorder(
        borderRadius: const BorderRadius.all(
          Radius.circular(Tokens.bezelRadius),
        ),
        side: BorderSide(color: p.alarm),
      ),
    ),
    tooltipTheme: TooltipThemeData(
      decoration: BoxDecoration(
        color: p.glass,
        border: Border.all(color: p.bezelHi),
        borderRadius: BorderRadius.circular(Tokens.innerRadius),
      ),
      textStyle: t.small(p.emph),
    ),
    dividerTheme: DividerThemeData(color: p.rule, thickness: 1, space: 1),
    progressIndicatorTheme: ProgressIndicatorThemeData(
      color: p.lit,
      linearTrackColor: p.ghost,
      circularTrackColor: p.ghost,
    ),
    filledButtonTheme: FilledButtonThemeData(
      style: FilledButton.styleFrom(
        backgroundColor: p.lit,
        foregroundColor: p.onKey,
        disabledBackgroundColor: p.ghost,
        disabledForegroundColor: p.dim,
        shape: square,
        minimumSize: const Size(64, 44),
        textStyle: keyText,
        padding: const EdgeInsets.symmetric(horizontal: 18),
      ),
    ),
    outlinedButtonTheme: OutlinedButtonThemeData(
      style: OutlinedButton.styleFrom(
        foregroundColor: p.lit,
        side: BorderSide(color: p.bezelHi),
        shape: square,
        minimumSize: const Size(64, 44),
        textStyle: keyText,
        padding: const EdgeInsets.symmetric(horizontal: 18),
      ),
    ),
    textButtonTheme: TextButtonThemeData(
      style: TextButton.styleFrom(
        foregroundColor: p.lit,
        shape: square,
        minimumSize: const Size(48, 44),
        textStyle: keyText,
      ),
    ),
    iconButtonTheme: IconButtonThemeData(
      style: IconButton.styleFrom(
        foregroundColor: p.dim,
        minimumSize: const Size(Tokens.touch, Tokens.touch),
        shape: square,
      ),
    ),
    floatingActionButtonTheme: FloatingActionButtonThemeData(
      backgroundColor: p.lit,
      foregroundColor: p.onKey,
      elevation: 0,
      focusElevation: 0,
      hoverElevation: 0,
      highlightElevation: 0,
      extendedTextStyle: keyText,
      shape: square,
    ),
    inputDecorationTheme: InputDecorationTheme(
      border: field(p.bezelHi),
      enabledBorder: field(p.bezelHi),
      focusedBorder: field(p.lit, 1.5),
      errorBorder: field(p.alarm),
      focusedErrorBorder: field(p.alarm, 1.5),
      labelStyle: t.small(),
      floatingLabelStyle: t.legend(p.lit),
      helperStyle: t.small(),
      hintStyle: t.body(p.dim),
      errorStyle: t.small(p.alarm),
      isDense: true,
      contentPadding: const EdgeInsets.symmetric(horizontal: 12, vertical: 12),
    ),
    textSelectionTheme: TextSelectionThemeData(
      cursorColor: p.lit,
      selectionColor: p.info.withValues(alpha: 0.30),
      selectionHandleColor: p.info,
    ),
    scrollbarTheme: ScrollbarThemeData(
      thumbColor: WidgetStatePropertyAll(p.bezelHi),
      radius: const Radius.circular(Tokens.innerRadius),
      thickness: const WidgetStatePropertyAll(4),
    ),
    dropdownMenuTheme: DropdownMenuThemeData(textStyle: t.data()),
    bottomSheetTheme: BottomSheetThemeData(backgroundColor: p.glass),
  );
}
