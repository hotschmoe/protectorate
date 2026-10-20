import 'package:flutter/widgets.dart';
import 'package:xterm2/xterm.dart';

import '../theme/tokens.dart';

/// Web canvas text cannot reach system fonts, so the monospace face is bundled.
const terminalTextStyle = TerminalStyle(fontFamily: monoFamily, fontSize: 14);

TerminalTheme terminalTheme(BuildContext context) {
  final t = Tokens.of(context);
  final a = t.p.ansi;
  return TerminalTheme(
    cursor: t.p.lit,
    selection: t.p.info.withValues(alpha: 0.35),
    foreground: t.p.lit,
    background: t.p.glass,
    black: a[0],
    red: a[1],
    green: a[2],
    yellow: a[3],
    blue: a[4],
    magenta: a[5],
    cyan: a[6],
    white: a[7],
    brightBlack: a[8],
    brightRed: a[9],
    brightGreen: a[10],
    brightYellow: a[11],
    brightBlue: a[12],
    brightMagenta: a[13],
    brightCyan: a[14],
    brightWhite: a[15],
    searchHitBackground: t.p.warn.withValues(alpha: 0.5),
    searchHitBackgroundCurrent: t.p.warn,
    searchHitForeground: t.p.onKey,
  );
}
