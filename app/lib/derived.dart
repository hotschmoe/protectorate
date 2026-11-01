import 'dart:convert';

import 'api/api.g.dart';

/// Lookups and derived values over the generated API types.
extension HubStateLookups on HubState {
  Sleeve? sleeve(String id) => sleeves.where((s) => s.id == id).firstOrNull;

  Sleeve? get envoy => sleeves.where((s) => s.role == SleeveRole.envoy).firstOrNull;

  Node? node(String id) => nodes.where((n) => n.id == id).firstOrNull;

  Quota? quota(Subscription s) => quotas.where((q) => q.subscription == s).firstOrNull;

  Approval? approval(String id) => approvals.where((a) => a.id == id).firstOrNull;

  List<Approval> get pending => [
    for (final a in approvals)
      if (a.state == ApprovalState.pending) a,
  ];

  int get unreadEvents => events.where((e) => !e.read).length;
}

extension SleeveActivity on Sleeve {
  ActivityState? get activity => incarnation?.activity.state;
}

extension IncarnationContext on Incarnation {
  double? get contextFraction => contextWindow > 0 ? contextTokens / contextWindow : null;
}

extension DiskUse on Disk {
  double get usedFraction => sizeBytes > 0 ? 1 - freeBytes / sizeBytes : 0;
}

extension NodeMemory on Node {
  double? get memUsedFraction {
    final a = sample?.memAvailBytes;
    final t = inventory?.ramBytes ?? 0;
    return a == null || t <= 0 ? null : 1 - a / t;
  }
}

extension QuotaTightest on Quota {
  /// The tightest window, the one the quota hint shows.
  QuotaWindow? get tightest {
    QuotaWindow? best;
    for (final w in windows) {
      final u = w.usedPct;
      if (u == null) continue;
      if (best == null || u > best.usedPct!) best = w;
    }
    return best;
  }
}

extension JobCommand on Job {
  /// The argv as one shell-like line.
  String get command => [
    for (final a in argv) a.isEmpty || a.contains(RegExp(r'''[\s'"\\$]''')) ? "'${a.replaceAll("'", r"'\''")}'" : a,
  ].join(' ');
}

// ---- transcript folding ----------------------------------------------------------------------

/// One row of a sleeve's chat, folded from its append-only transcript: tool results join their
/// call, deltas join their block, acks and decisions join their message or card.
sealed class ChatRow {
  ChatRow(this.seq, this.at);

  final int seq;
  final DateTime at;
}

class UserRow extends ChatRow {
  UserRow(super.seq, super.at, this.msg);

  final EntryUserMessage msg;
  AckKind? ack;
}

class TextRow extends ChatRow {
  TextRow(super.seq, super.at, this.id, {required this.thinking});

  final String id;
  final bool thinking;
  String text = '';
  bool streaming = false;
}

class ToolRow extends ChatRow {
  ToolRow(super.seq, super.at, this.call);

  final EntryToolCall call;
  EntryToolResult? result;
  DateTime? endedAt;

  String get input => switch (call.input) {
    null => '',
    final String s => s,
    final v => const JsonEncoder.withIndent('  ').convert(v),
  };
}

class ApprovalRow extends ChatRow {
  ApprovalRow(super.seq, super.at, this.request);

  final EntryApprovalRequest request;
}

/// A one-line entry: session, turn footer, notice, error, compaction, file change, exit.
class LineRow extends ChatRow {
  LineRow(super.seq, super.at, this.text, this.level, {this.who = '--'});

  final String text;
  final Level level;
  final String who;
}

String _k(int n) => n >= 1000 ? '${(n / 1000).toStringAsFixed(1)}k' : '$n';

List<ChatRow> foldTranscript(List<ChatEntry> entries) {
  final rows = <ChatRow>[];
  final users = <String, UserRow>{};
  final texts = <String, TextRow>{};
  final tools = <String, ToolRow>{};
  void line(ChatEntry e, String text, Level level, {String who = '--'}) =>
      rows.add(LineRow(e.seq, e.at, text, level, who: who));
  for (final e in entries) {
    switch (e.entry) {
      case final EntrySessionInfo x:
        final model = x.model == null ? '' : ' ${x.model}';
        line(e, '${x.harness.wire}$model g${e.g} ${x.mode.wire}', Level.info);
      case final EntryUserMessage x:
        final r = UserRow(e.seq, e.at, x);
        users[x.requestId] = r;
        rows.add(r);
      case final EntryPromptAck x:
        users[x.requestId]?.ack = x.ack;
        if (x.ack == AckKind.rejected) {
          line(e, 'message not taken${x.detail == null ? '' : ': ${x.detail}'}', Level.alarm);
        }
      case EntryAssistantText(:final id, :final text, :final delta):
      case EntryReasoning(:final id, :final text, :final delta):
        final thinking = e.entry is EntryReasoning;
        final key = '${thinking ? 'r' : 't'}:$id';
        final r = texts[key] ??= () {
          final n = TextRow(e.seq, e.at, id, thinking: thinking);
          rows.add(n);
          return n;
        }();
        if (delta) {
          if (r.streaming || r.text.isEmpty) {
            r.text += text;
            r.streaming = true;
          }
        } else {
          r.text = text;
          r.streaming = false;
        }
      case final EntryToolCall x:
        final r = ToolRow(e.seq, e.at, x);
        tools[x.id] = r;
        rows.add(r);
      case final EntryToolResult x:
        final r = tools[x.id];
        if (r == null) {
          line(e, 'result for unknown tool call ${x.id}: ${x.status.wire}', Level.warn);
        } else {
          r.result = x;
          r.endedAt = x.durationMs == null ? e.at : r.at.add(Duration(milliseconds: x.durationMs!));
        }
      case final EntryFileChange x:
        final n = x.files.length;
        final names = x.files.take(3).map((f) => f.path).join(', ');
        line(e, 'changed $n file${n == 1 ? '' : 's'}${n == 0 ? '' : ': $names'}${n > 3 ? ', ...' : ''}',
            Level.info);
      case final EntryApprovalRequest x:
        rows.add(ApprovalRow(e.seq, e.at, x));
      case EntryApprovalDecision():
      case EntryTurnStarted():
        break;
      case final EntryTurnSettled x:
        final u = x.usage;
        final ctx = u.contextTokens != null && (u.contextWindow ?? 0) > 0
            ? '  ctx ${(u.contextTokens! * 100 / u.contextWindow!).round()}%'
            : '';
        final secs = x.durationMs == null ? '' : '  ${(x.durationMs! / 1000).toStringAsFixed(1)}s';
        final stop = x.stop == Stop.other ? (x.nativeStop ?? 'other') : x.stop.wire;
        line(e, '$stop  ${_k(u.input)} in  ${_k(u.output)} out$ctx$secs',
            x.stop == Stop.endTurn ? Level.info : Level.warn, who: 'turn');
      case final EntryError x:
        line(e, '${x.kind.wire}: ${x.text}', Level.alarm, who: 'error');
      case final EntryCompaction x:
        final sizes = x.tokensBefore == null || x.tokensAfter == null
            ? ''
            : ' ${_k(x.tokensBefore!)} -> ${_k(x.tokensAfter!)}';
        line(e, 'context compacted$sizes${x.trigger == null ? '' : ' (${x.trigger})'}', Level.info);
      case final EntryNotice x:
        line(e, x.text, x.level);
      case final EntryExited x:
        final how = x.signal != null ? 'signal ${x.signal}' : 'code ${x.code ?? '?'}';
        final tail = x.stderrTail.trim().isEmpty ? '' : '\n${x.stderrTail.trim()}';
        line(e, 'g${e.g} exited, $how$tail', x.requested ? Level.info : Level.alarm);
      case final EntryUnknown x:
        line(e, '(entry of unknown type ${x.json['type']})', Level.info, who: '?');
    }
  }
  return rows;
}
