import '../gen/protectorate/v1/chat.pb.dart';
import '../gen/protectorate/v1/sleeve.pb.dart';

final _frontier = RegExp(r'^(claude|gpt|chatgpt|codex|o\d|grok|gemini)');

// Only pi can run arbitrary local models; an empty model name is unknown and
// not flagged.
bool isLocalModel(Harness harness, String model) {
  if (harness != Harness.HARNESS_PI) return false;
  final name = model.trim().toLowerCase().split('/').last;
  return name.isNotEmpty && !_frontier.hasMatch(name);
}

// A spawn without a mode means chat for agent harnesses and terminal for
// shell; older sleeves report unspecified.
SleeveMode effectiveMode(Sleeve s) {
  if (s.mode != SleeveMode.SLEEVE_MODE_UNSPECIFIED) return s.mode;
  return s.harness == Harness.HARNESS_SHELL
      ? SleeveMode.SLEEVE_MODE_TERMINAL
      : SleeveMode.SLEEVE_MODE_CHAT;
}

String stateLabel(ChatState s) => switch (s) {
  ChatState.CHAT_STATE_AWAITING_APPROVAL => 'Needs approval',
  ChatState.CHAT_STATE_WORKING => 'Working',
  ChatState.CHAT_STATE_COMPACTING => 'Compacting',
  ChatState.CHAT_STATE_IDLE => 'Idle',
  ChatState.CHAT_STATE_FAILED => 'Failed',
  ChatState.CHAT_STATE_EXITED => 'Exited',
  _ => 'Unknown',
};

String formatTokens(num n) {
  if (n >= 1000000) return '${(n / 1000000).toStringAsFixed(1)}M';
  if (n >= 1000) return '${(n / 1000).toStringAsFixed(n >= 100000 ? 0 : 1)}k';
  return '$n';
}
