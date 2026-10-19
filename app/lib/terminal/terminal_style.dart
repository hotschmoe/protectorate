import 'package:xterm/xterm.dart';

import '../theme/tokens.dart';

/// Web canvas text cannot reach system fonts, so the monospace face is bundled.
const terminalTextStyle = TerminalStyle(fontFamily: monoFamily);
