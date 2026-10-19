import 'package:flutter/material.dart';

import '../gen/protectorate/v1/chat.pb.dart';
import '../gen/protectorate/v1/sleeve.pb.dart';

const monoFamily = 'JetBrains Mono';

ThemeData appTheme(Brightness brightness) => ThemeData(
  colorScheme: ColorScheme.fromSeed(
    seedColor: Colors.indigo,
    brightness: brightness,
  ),
  fontFamily: monoFamily,
  useMaterial3: true,
);

class Accent {
  const Accent(this.fg, this.bg);
  final Color fg;
  final Color bg;
}

// Every color and metric the chat and sleeve widgets use lives here, so a
// restyle edits this file and not the widgets.
class Tokens {
  const Tokens._(this.dark);

  factory Tokens.of(BuildContext context) =>
      Tokens._(Theme.of(context).brightness == Brightness.dark);

  final bool dark;

  static const gap = 12.0;
  static const radius = 14.0;
  static const smallRadius = 8.0;
  static const contentMaxWidth = 760.0;

  Accent _a(Color light, Color lightBg, Color darkFg, Color darkBg) =>
      dark ? Accent(darkFg, darkBg) : Accent(light, lightBg);

  Accent harness(Harness h) => switch (h) {
    Harness.HARNESS_CLAUDE => _a(
      const Color(0xFFB4501F),
      const Color(0xFFFBE3D6),
      const Color(0xFFFFA77F),
      const Color(0xFF4A2412),
    ),
    Harness.HARNESS_CODEX => _a(
      const Color(0xFF0B7A5C),
      const Color(0xFFD3F1E7),
      const Color(0xFF5FD8B2),
      const Color(0xFF0E3A2E),
    ),
    Harness.HARNESS_GROK => _a(
      const Color(0xFFB02A62),
      const Color(0xFFFADCE8),
      const Color(0xFFFF8DB8),
      const Color(0xFF4A1530),
    ),
    Harness.HARNESS_PI => _a(
      const Color(0xFF8A6A00),
      const Color(0xFFFBEFBF),
      const Color(0xFFF0CB4A),
      const Color(0xFF45380A),
    ),
    _ => _a(
      const Color(0xFF4F6470),
      const Color(0xFFE0E8EC),
      const Color(0xFFA9BCC6),
      const Color(0xFF28343A),
    ),
  };

  // Colour of the party serving the model: the provider when known, the
  // harness otherwise. A model served locally gets its own colour whatever
  // the provider id is.
  Accent model(Harness h, String provider, {bool local = false}) {
    if (local) {
      return _a(
        const Color(0xFF2F6B73),
        const Color(0xFFD6EEF1),
        const Color(0xFF7FD3DE),
        const Color(0xFF123C42),
      );
    }
    final p = provider.toLowerCase();
    bool is_(String prefix) => p.startsWith(prefix);
    if (is_('anthropic')) return harness(Harness.HARNESS_CLAUDE);
    if (is_('openai')) return harness(Harness.HARNESS_CODEX);
    if (is_('xai')) return harness(Harness.HARNESS_GROK);
    if (is_('deepseek')) {
      return _a(
        const Color(0xFF2A4FC0),
        const Color(0xFFDCE5FF),
        const Color(0xFF93AEFF),
        const Color(0xFF1B2D6B),
      );
    }
    if (is_('zai') || is_('z-ai') || is_('zhipu')) {
      return _a(
        const Color(0xFF5B3FC0),
        const Color(0xFFE6DFFF),
        const Color(0xFFBBA6FF),
        const Color(0xFF30226B),
      );
    }
    if (is_('moonshot') || is_('kimi')) {
      return _a(
        const Color(0xFF1F6FA8),
        const Color(0xFFD5EAF8),
        const Color(0xFF86C6F0),
        const Color(0xFF123A57),
      );
    }
    if (is_('minimax')) {
      return _a(
        const Color(0xFFB8421F),
        const Color(0xFFFBDDD2),
        const Color(0xFFFF9C80),
        const Color(0xFF55200F),
      );
    }
    if (is_('qwen') || is_('alibaba') || is_('dashscope')) {
      return _a(
        const Color(0xFF7A3FB0),
        const Color(0xFFF0DDFB),
        const Color(0xFFD9A8F5),
        const Color(0xFF3E1A5C),
      );
    }
    if (is_('openrouter')) {
      return _a(
        const Color(0xFF3F5A9A),
        const Color(0xFFDDE4F5),
        const Color(0xFFA6BAE8),
        const Color(0xFF212F52),
      );
    }
    return harness(h);
  }

  Accent chatState(ChatState s) => switch (s) {
    ChatState.CHAT_STATE_AWAITING_APPROVAL => _a(
      const Color(0xFF8A5300),
      const Color(0xFFFFE2B3),
      const Color(0xFFFFC266),
      const Color(0xFF5C3A00),
    ),
    ChatState.CHAT_STATE_WORKING => _a(
      const Color(0xFF1D4FB8),
      const Color(0xFFD9E5FF),
      const Color(0xFF9DBBFF),
      const Color(0xFF1B3366),
    ),
    ChatState.CHAT_STATE_COMPACTING => _a(
      const Color(0xFF6B3FB0),
      const Color(0xFFEBDDFF),
      const Color(0xFFCDB0FF),
      const Color(0xFF3A2466),
    ),
    ChatState.CHAT_STATE_IDLE => _a(
      const Color(0xFF1E7A3C),
      const Color(0xFFD5F2DF),
      const Color(0xFF86DDA2),
      const Color(0xFF173F25),
    ),
    ChatState.CHAT_STATE_FAILED => _a(
      const Color(0xFFB3261E),
      const Color(0xFFFFDAD6),
      const Color(0xFFFFB4AB),
      const Color(0xFF601410),
    ),
    _ => _a(
      const Color(0xFF55606A),
      const Color(0xFFE3E7EA),
      const Color(0xFFB5BEC5),
      const Color(0xFF30373C),
    ),
  };

  Accent get warning => _a(
    const Color(0xFF7A3E00),
    const Color(0xFFFFD9A8),
    const Color(0xFFFFD08A),
    const Color(0xFF6B3800),
  );

  Color get approvalBorder =>
      dark ? const Color(0xFFFFC266) : const Color(0xFFD08A00);
}
