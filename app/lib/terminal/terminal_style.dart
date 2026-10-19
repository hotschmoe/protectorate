import 'package:xterm/xterm.dart';

/// Web canvas text cannot reach system fonts, so the monospace face is bundled.
const terminalTextStyle = TerminalStyle(fontFamily: 'DejaVu Sans Mono');
