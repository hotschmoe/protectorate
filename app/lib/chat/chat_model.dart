import 'dart:convert';

import '../gen/protectorate/v1/chat.pb.dart';

sealed class ChatItem {
  ChatItem(this.seq);
  final int seq;
}

class UserItem extends ChatItem {
  UserItem(super.seq, this.text, this.from);
  final String text;
  final String from;
}

class AssistantItem extends ChatItem {
  AssistantItem(super.seq, this.messageId, this.thinking, this.text);
  final String messageId;
  final bool thinking;
  String text;
}

class ToolItem extends ChatItem {
  ToolItem(super.seq, this.id, this.name, this.inputJson);
  final String id;
  final String name;
  final String inputJson;
  bool done = false;
  String output = '';
  bool isError = false;
  bool truncated = false;
}

class ApprovalItem extends ChatItem {
  ApprovalItem(super.seq, this.id, this.kind, this.summary);
  final String id;
  final String kind;
  final String summary;
  bool resolved = false;
  bool allowed = false;
  String by = '';
}

class CompactionItem extends ChatItem {
  CompactionItem(super.seq, this.tokensBefore, this.tokensAfter, this.native);
  final int tokensBefore;
  final int tokensAfter;
  final bool native;
}

class ErrorItem extends ChatItem {
  ErrorItem(super.seq, this.text);
  final String text;
}

// Where a resleeve replaced the harness; what follows comes from the new one.
class HandoffItem extends ChatItem {
  HandoffItem(super.seq, this.handoff);
  final Handoff handoff;
}

// Lower enum value means higher priority; UNSPECIFIED never wins.
ChatState resolveState(Iterable<ChatState> candidates) {
  var best = ChatState.CHAT_STATE_UNSPECIFIED;
  for (final c in candidates) {
    if (c == ChatState.CHAT_STATE_UNSPECIFIED) continue;
    if (best == ChatState.CHAT_STATE_UNSPECIFIED || c.value < best.value) {
      best = c;
    }
  }
  return best;
}

class ChatTranscript {
  final List<ChatItem> items = [];
  final Map<String, AssistantItem> _messages = {};
  final Map<String, ToolItem> _tools = {};
  final Map<String, ApprovalItem> _approvals = {};
  int lastSeq = 0;
  bool _lastWasText = false;
  int? _replaceSeq;
  ChatState reported = ChatState.CHAT_STATE_UNSPECIFIED;
  int contextTokens = 0;
  int contextWindow = 0;

  Iterable<ApprovalItem> get pendingApprovals =>
      _approvals.values.where((a) => !a.resolved);

  ChatState get state => resolveState([
    reported,
    if (pendingApprovals.isNotEmpty) ChatState.CHAT_STATE_AWAITING_APPROVAL,
  ]);

  double? get contextFraction => contextWindow > 0
      ? (contextTokens / contextWindow).clamp(0.0, 1.0)
      : null;

  void seed(ChatStatus s) {
    if (lastSeq > 0) return;
    reported = s.state;
    contextTokens = s.contextTokens.toInt();
    contextWindow = s.contextWindow.toInt();
  }

  // The after_seq for the next Watch. Text deltas share their message's seq,
  // so after a text event the watch restarts one earlier and the first text
  // it sends for that seq (the whole message so far) replaces ours.
  int resumeAfter() {
    if (!_lastWasText) return lastSeq;
    _replaceSeq = lastSeq;
    return lastSeq - 1;
  }

  // Returns false for an event already applied, which a resumed watch can
  // legitimately repeat.
  bool apply(ChatEvent e) {
    final seq = e.seq.toInt();
    final isText = e.whichBody() == ChatEvent_Body.text;
    if (seq < lastSeq) return false;
    if (seq == lastSeq) {
      if (!isText || !_lastWasText) return false;
      final replace = _replaceSeq == seq;
      _replaceSeq = null;
      _text(seq, e.text, replace: replace);
      return true;
    }
    lastSeq = seq;
    _lastWasText = isText;
    _replaceSeq = null;
    switch (e.whichBody()) {
      case ChatEvent_Body.user:
        items.add(UserItem(seq, e.user.text, e.user.from));
      case ChatEvent_Body.text:
        _text(seq, e.text);
      case ChatEvent_Body.toolCall:
        final c = e.toolCall;
        final t = ToolItem(seq, c.id, c.name, c.inputJson);
        _tools[c.id] = t;
        items.add(t);
      case ChatEvent_Body.toolResult:
        _result(seq, e.toolResult);
      case ChatEvent_Body.approval:
        final a = ApprovalItem(
          seq,
          e.approval.id,
          e.approval.kind,
          e.approval.summary,
        );
        _approvals[a.id] = a;
        items.add(a);
      case ChatEvent_Body.approvalResolved:
        final r = e.approvalResolved;
        final a = _approvals[r.id];
        if (a != null) {
          a.resolved = true;
          a.allowed = r.allow;
          a.by = r.by;
        }
      case ChatEvent_Body.usage:
        contextTokens = e.usage.contextTokens.toInt();
        contextWindow = e.usage.contextWindow.toInt();
      case ChatEvent_Body.settled:
        break;
      case ChatEvent_Body.compaction:
        final c = e.compaction;
        items.add(
          CompactionItem(
            seq,
            c.tokensBefore.toInt(),
            c.tokensAfter.toInt(),
            c.native,
          ),
        );
        if (c.tokensAfter > 0) contextTokens = c.tokensAfter.toInt();
      case ChatEvent_Body.error:
        items.add(ErrorItem(seq, e.error.text));
      case ChatEvent_Body.state:
        reported = e.state.state;
      case ChatEvent_Body.handoff:
        items.add(HandoffItem(seq, e.handoff));
      case ChatEvent_Body.notSet:
        break;
    }
    return true;
  }

  void _text(int seq, AssistantText t, {bool replace = false}) {
    final key = '${t.thinking}:${t.messageId}';
    var m = _messages[key];
    if (m == null && t.messageId.isEmpty && items.isNotEmpty) {
      final last = items.last;
      if (last is AssistantItem && last.thinking == t.thinking) m = last;
    }
    if (m == null) {
      m = AssistantItem(seq, t.messageId, t.thinking, '');
      _messages[key] = m;
      items.add(m);
    }
    m.text = replace ? t.delta : m.text + t.delta;
  }

  void _result(int seq, ToolResult r) {
    var t = _tools[r.toolCallId];
    if (t == null) {
      t = ToolItem(seq, r.toolCallId, 'tool', '');
      _tools[r.toolCallId] = t;
      items.add(t);
    }
    t.done = true;
    t.output = r.output;
    t.isError = r.isError;
    t.truncated = r.truncated;
  }
}

String toolSummary(String name, String inputJson, {int max = 80}) {
  String clip(String s) {
    final one = s.replaceAll(RegExp(r'\s+'), ' ').trim();
    return one.length > max ? '${one.substring(0, max - 1)}...' : one;
  }

  try {
    final v = jsonDecode(inputJson);
    if (v is Map) {
      for (final k in const [
        'command',
        'cmd',
        'file_path',
        'path',
        'pattern',
        'query',
        'url',
      ]) {
        final x = v[k];
        if (x is String && x.isNotEmpty) return clip(x);
        if (x is List && x.isNotEmpty) return clip(x.join(' '));
      }
    }
  } on FormatException {
    return clip(inputJson);
  }
  return clip(inputJson);
}

String prettyJson(String s) {
  try {
    return const JsonEncoder.withIndent('  ').convert(jsonDecode(s));
  } on FormatException {
    return s;
  }
}

class TextSegment {
  const TextSegment(this.text, {this.code = false, this.lang = ''});
  final String text;
  final bool code;
  final String lang;
}

// Splits fenced code blocks from prose; an unclosed fence (mid-stream) renders
// as code to the end.
List<TextSegment> splitFences(String s) {
  final out = <TextSegment>[];
  final prose = StringBuffer();
  final code = StringBuffer();
  var inCode = false;
  var lang = '';
  void flushProse() {
    final t = prose.toString().trim();
    if (t.isNotEmpty) out.add(TextSegment(t));
    prose.clear();
  }

  for (final line in s.split('\n')) {
    if (line.trimLeft().startsWith('```')) {
      if (inCode) {
        out.add(TextSegment(code.toString(), code: true, lang: lang));
        code.clear();
        inCode = false;
      } else {
        flushProse();
        lang = line.trim().substring(3).trim();
        inCode = true;
      }
    } else if (inCode) {
      if (code.isNotEmpty) code.write('\n');
      code.write(line);
    } else {
      if (prose.isNotEmpty) prose.write('\n');
      prose.write(line);
    }
  }
  if (inCode) {
    out.add(TextSegment(code.toString(), code: true, lang: lang));
  } else {
    flushProse();
  }
  return out;
}
