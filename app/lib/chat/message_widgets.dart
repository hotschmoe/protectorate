import 'package:flutter/material.dart';

import '../gen/protectorate/v1/chat.pb.dart';
import '../sleeves/sleeve_info.dart';
import '../theme/tokens.dart';
import '../vfd/lamp.dart';
import '../vfd/panel.dart';
import 'chat_model.dart';

class UserBubble extends StatelessWidget {
  const UserBubble({super.key, required this.item});

  final UserItem item;

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    final hub = item.from == 'hub';
    final edge = hub ? t.p.dim : t.p.info;
    return Container(
      width: double.infinity,
      padding: const EdgeInsets.fromLTRB(14, 10, 12, 10),
      decoration: BoxDecoration(
        color: t.tint(edge).withValues(alpha: t.dark ? 0.07 : 0.06),
        border: Border(left: BorderSide(color: edge, width: 2)),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        mainAxisSize: MainAxisSize.min,
        children: [
          if (item.from.isNotEmpty && item.from != 'owner')
            Padding(
              padding: const EdgeInsets.only(bottom: 4),
              child: Text(item.from.toUpperCase(), style: t.legend(edge)),
            ),
          SelectableText(item.text, style: t.body(t.p.emph)),
        ],
      ),
    );
  }
}

class AssistantMessage extends StatelessWidget {
  const AssistantMessage({super.key, required this.item});

  final AssistantItem item;

  @override
  Widget build(BuildContext context) {
    if (item.thinking) return ThinkingBlock(item: item);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        for (final seg in splitFences(item.text))
          if (seg.code)
            Padding(
              padding: const EdgeInsets.symmetric(vertical: 6),
              child: Printout(
                text: seg.text,
                legend: seg.lang.isEmpty ? null : seg.lang.toUpperCase(),
                diff: seg.lang == 'diff' || looksLikeDiff(seg.text),
              ),
            )
          else
            Padding(
              padding: const EdgeInsets.symmetric(vertical: 2),
              child: _MarkdownProse(text: seg.text),
            ),
      ],
    );
  }
}

class _MarkdownProse extends StatelessWidget {
  const _MarkdownProse({required this.text});

  final String text;

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    final blocks = <Widget>[];
    final paragraph = <String>[];
    final heading = RegExp(r'^ {0,3}(#{1,6})\s+(.+?)(?:\s+#+)?$');
    final list = RegExp(r'^(\s*)([-+*]|\d+[.)])\s+(.+)$');

    Widget prose(String value, TextStyle style) => SelectableText.rich(
      TextSpan(children: _markdownSpans(value, style, t)),
      style: style,
    );

    void flush() {
      if (paragraph.isEmpty) return;
      blocks.add(prose(paragraph.join('\n'), t.body()));
      paragraph.clear();
    }

    for (final line in text.split('\n')) {
      final h = heading.firstMatch(line);
      final li = list.firstMatch(line);
      if (line.trim().isEmpty) {
        flush();
      } else if (h != null) {
        flush();
        blocks.add(
          prose(
            h[2]!,
            t
                .body(t.p.emph)
                .copyWith(
                  fontSize: h[1]!.length <= 2 ? 18 : 15,
                  fontWeight: FontWeight.w700,
                ),
          ),
        );
      } else if (li != null) {
        flush();
        blocks.add(
          Padding(
            padding: EdgeInsets.only(left: li[1]!.length.clamp(0, 8) * 4.0),
            child: Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text('${li[2]} ', style: t.body(t.p.dim)),
                Expanded(child: prose(li[3]!, t.body())),
              ],
            ),
          ),
        );
      } else if (line.trimLeft().startsWith('> ')) {
        flush();
        blocks.add(
          Container(
            padding: const EdgeInsets.only(left: 12),
            decoration: BoxDecoration(
              border: Border(left: BorderSide(color: t.p.bezelHi, width: 2)),
            ),
            child: prose(line.trimLeft().substring(2), t.body(t.p.dim)),
          ),
        );
      } else {
        paragraph.add(line);
      }
    }
    flush();
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        for (var i = 0; i < blocks.length; i++)
          Padding(
            padding: EdgeInsets.only(top: i == 0 ? 0 : 8),
            child: blocks[i],
          ),
      ],
    );
  }
}

final _markdownInline = RegExp(
  r'\\[\\`*_\[\]()]|`([^`\n]+)`|\[([^\]\n]+)\]\(([^)\n]+)\)|\*\*(.+?)\*\*|__(.+?)__|\*([^*\n]+)\*|_([^_\n]+)_',
);

List<InlineSpan> _markdownSpans(
  String text,
  TextStyle style,
  Tokens t, [
  int depth = 0,
]) {
  if (depth >= 8) return [TextSpan(text: text, style: style)];
  final spans = <InlineSpan>[];
  var end = 0;
  for (final match in _markdownInline.allMatches(text)) {
    if (match.start > end) {
      spans.add(TextSpan(text: text.substring(end, match.start), style: style));
    }
    if (match[1] != null) {
      spans.add(
        TextSpan(
          text: match[1],
          style: style.copyWith(color: t.p.emph, backgroundColor: t.p.bar),
        ),
      );
    } else if (match[2] != null) {
      spans.add(
        TextSpan(
          children: _markdownSpans(
            match[2]!,
            style.copyWith(
              color: t.p.info,
              decoration: TextDecoration.underline,
            ),
            t,
            depth + 1,
          ),
        ),
      );
      if (match[2] != match[3]) {
        spans.add(TextSpan(text: ' (${match[3]})', style: style));
      }
    } else if (match[4] != null || match[5] != null) {
      spans.addAll(
        _markdownSpans(
          match[4] ?? match[5]!,
          style.copyWith(fontWeight: FontWeight.w700),
          t,
          depth + 1,
        ),
      );
    } else if (match[6] != null || match[7] != null) {
      spans.addAll(
        _markdownSpans(
          match[6] ?? match[7]!,
          style.copyWith(fontStyle: FontStyle.italic),
          t,
          depth + 1,
        ),
      );
    } else {
      spans.add(TextSpan(text: match[0]!.substring(1), style: style));
    }
    end = match.end;
  }
  if (end < text.length) {
    spans.add(TextSpan(text: text.substring(end), style: style));
  }
  return spans;
}

class ThinkingBlock extends StatefulWidget {
  const ThinkingBlock({super.key, required this.item});

  final AssistantItem item;

  @override
  State<ThinkingBlock> createState() => _ThinkingBlockState();
}

class _ThinkingBlockState extends State<ThinkingBlock> {
  bool _open = false;

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        InkWell(
          key: const ValueKey('thinking-toggle'),
          borderRadius: BorderRadius.circular(Tokens.innerRadius),
          onTap: () => setState(() => _open = !_open),
          child: ConstrainedBox(
            constraints: const BoxConstraints(minHeight: 48),
            child: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                Icon(
                  _open ? Icons.expand_more : Icons.chevron_right,
                  size: 18,
                  color: t.p.dim,
                ),
                const SizedBox(width: 4),
                Text('THINKING', style: t.legend()),
              ],
            ),
          ),
        ),
        if (_open)
          Padding(
            padding: const EdgeInsets.only(left: 22, bottom: 4),
            child: SelectableText(
              widget.item.text,
              style: t.small().copyWith(fontStyle: FontStyle.italic),
            ),
          ),
      ],
    );
  }
}

bool looksLikeDiff(String s) =>
    s.startsWith('--- ') ||
    s.startsWith('diff ') ||
    RegExp(r'^@@ .* @@', multiLine: true).hasMatch(s);

// Fixed-pitch output as it would come off the printer: buff bars every
// three lines on paper, lit additions and deletions for a diff.
class Printout extends StatelessWidget {
  const Printout({
    super.key,
    required this.text,
    this.diff = false,
    this.legend,
    this.maxHeight,
  });

  final String text;
  final bool diff;
  final String? legend;
  final double? maxHeight;

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    final lines = text.replaceAll('\t', '    ').split('\n');
    final base = t.data(t.p.emph).copyWith(fontSize: 12.5, height: 1.45);
    Widget line(int i) {
      final l = lines[i];
      Color? fg;
      Color? bg;
      if (diff) {
        if (l.startsWith('+++') || l.startsWith('---')) {
          fg = t.p.dim;
        } else if (l.startsWith('@@')) {
          fg = t.p.info;
        } else if (l.startsWith('+')) {
          fg = t.p.ok;
          bg = t.p.ok.withValues(alpha: t.dark ? 0.10 : 0.10);
        } else if (l.startsWith('-')) {
          fg = t.p.alarm;
          bg = t.p.alarm.withValues(alpha: t.dark ? 0.10 : 0.08);
        }
      }
      bg ??= !t.dark && (i ~/ 3).isOdd ? t.p.bar : null;
      return Container(
        color: bg,
        padding: const EdgeInsets.symmetric(horizontal: 10),
        child: Text(
          l.isEmpty ? ' ' : l,
          softWrap: false,
          style: fg == null ? base : base.copyWith(color: fg),
        ),
      );
    }

    final body = LayoutBuilder(
      builder: (context, box) => SingleChildScrollView(
        scrollDirection: Axis.horizontal,
        child: ConstrainedBox(
          constraints: BoxConstraints(minWidth: box.maxWidth),
          child: IntrinsicWidth(
            child: SelectionArea(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [for (var i = 0; i < lines.length; i++) line(i)],
              ),
            ),
          ),
        ),
      ),
    );
    return Container(
      width: double.infinity,
      decoration: BoxDecoration(
        color: t.dark ? t.p.bar : t.p.glass,
        border: Border.all(color: t.p.bezel),
        borderRadius: BorderRadius.circular(Tokens.innerRadius + 1),
      ),
      padding: const EdgeInsets.symmetric(vertical: 8),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          if (legend != null)
            Padding(
              padding: const EdgeInsets.fromLTRB(10, 0, 10, 6),
              child: Text(legend!, style: t.legend()),
            ),
          if (maxHeight == null)
            body
          else
            ConstrainedBox(
              constraints: BoxConstraints(maxHeight: maxHeight!),
              child: SingleChildScrollView(child: body),
            ),
        ],
      ),
    );
  }
}

class ToolCard extends StatefulWidget {
  const ToolCard({super.key, required this.item});

  final ToolItem item;

  @override
  State<ToolCard> createState() => _ToolCardState();
}

class _ToolCardState extends State<ToolCard> {
  bool _open = false;

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    final it = widget.item;
    final tone = !it.done ? Tone.info : (it.isError ? Tone.alarm : Tone.ok);
    final edge = it.isError ? t.p.alarm.withValues(alpha: 0.7) : t.p.bezel;
    return Container(
      key: ValueKey('tool-${it.id}'),
      decoration: BoxDecoration(
        color: t.p.glass,
        border: Border.all(color: edge),
        borderRadius: BorderRadius.circular(Tokens.innerRadius + 2),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          InkWell(
            borderRadius: BorderRadius.circular(Tokens.innerRadius + 2),
            onTap: () => setState(() => _open = !_open),
            child: ConstrainedBox(
              constraints: const BoxConstraints(minHeight: 48),
              child: Padding(
                padding: const EdgeInsets.symmetric(horizontal: 10),
                child: Row(
                  children: [
                    Lamp(tone: tone, size: 8),
                    const SizedBox(width: 10),
                    Flexible(
                      child: Text(
                        it.name.toUpperCase(),
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: t.legend(t.p.emph).copyWith(fontSize: 12),
                      ),
                    ),
                    const SizedBox(width: 10),
                    Expanded(
                      child: Text(
                        toolSummary(it.name, it.inputJson),
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: t.small(),
                      ),
                    ),
                    if (!it.done)
                      Padding(
                        padding: const EdgeInsets.only(left: 8),
                        child: Text('RUN', style: t.legend(t.p.info)),
                      ),
                    Icon(
                      _open ? Icons.expand_less : Icons.expand_more,
                      size: 18,
                      color: t.p.dim,
                    ),
                  ],
                ),
              ),
            ),
          ),
          if (_open)
            Padding(
              padding: const EdgeInsets.fromLTRB(8, 0, 8, 8),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  if (it.inputJson.isNotEmpty)
                    Printout(
                      text: prettyJson(it.inputJson),
                      legend: 'INPUT',
                      maxHeight: 200,
                    ),
                  if (it.done) ...[
                    const SizedBox(height: 6),
                    Printout(
                      text: it.output.isEmpty ? '(no output)' : it.output,
                      legend: it.isError ? 'ERROR' : 'OUTPUT',
                      diff: looksLikeDiff(it.output),
                      maxHeight: 280,
                    ),
                  ],
                  if (it.truncated)
                    Padding(
                      padding: const EdgeInsets.only(top: 6),
                      child: Text(
                        'OUTPUT TRUNCATED',
                        style: t.legend(t.p.warn),
                      ),
                    ),
                ],
              ),
            ),
        ],
      ),
    );
  }
}

class ApprovalCard extends StatelessWidget {
  const ApprovalCard({super.key, required this.item, required this.onRespond});

  final ApprovalItem item;
  final void Function(bool allow) onRespond;

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    final open = !item.resolved;
    final c = open ? t.p.warn : t.p.dim;
    return Container(
      key: ValueKey('approval-${item.id}'),
      padding: const EdgeInsets.all(Tokens.gap),
      decoration: BoxDecoration(
        color: open && !t.dark ? t.tint(t.p.warn) : t.p.glass,
        borderRadius: BorderRadius.circular(Tokens.bezelRadius),
        border: Border.all(
          color: open ? t.p.warn.withValues(alpha: 0.8) : t.p.bezel,
          width: open ? 1.5 : 1,
        ),
        boxShadow: open && t.dark
            ? [
                BoxShadow(
                  color: t.p.warn.withValues(alpha: 0.12),
                  blurRadius: 18,
                ),
              ]
            : null,
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              if (open)
                const Annunciator(
                  label: 'APPROVAL',
                  tone: Tone.warn,
                  blink: true,
                )
              else
                Annunciator(
                  label: item.allowed ? 'ALLOWED' : 'DENIED',
                  tone: item.allowed ? Tone.ok : Tone.alarm,
                  lit: false,
                ),
              const SizedBox(width: 10),
              Expanded(
                child: Text(
                  open
                      ? (item.kind.isEmpty ? 'NEEDED' : item.kind.toUpperCase())
                      : item.by.isEmpty
                      ? ''
                      : 'BY ${item.by.toUpperCase()}',
                  style: t.legend(c),
                ),
              ),
            ],
          ),
          const SizedBox(height: 10),
          SelectableText(
            item.summary,
            style: t.data(open ? t.p.emph : t.p.dim).copyWith(fontSize: 13.5),
          ),
          if (open) ...[
            const SizedBox(height: 12),
            Row(
              children: [
                FilledButton(
                  key: ValueKey('allow-${item.id}'),
                  style: FilledButton.styleFrom(
                    minimumSize: const Size(64, 48),
                    backgroundColor: t.p.warn,
                    foregroundColor: t.p.onKey,
                  ),
                  onPressed: () => onRespond(true),
                  child: const Text('ALLOW'),
                ),
                const SizedBox(width: 8),
                OutlinedButton(
                  key: ValueKey('deny-${item.id}'),
                  style: OutlinedButton.styleFrom(
                    minimumSize: const Size(64, 48),
                    foregroundColor: t.p.alarm,
                    side: BorderSide(color: t.p.alarm.withValues(alpha: 0.7)),
                  ),
                  onPressed: () => onRespond(false),
                  child: const Text('DENY'),
                ),
              ],
            ),
          ],
        ],
      ),
    );
  }
}

class CompactionMarker extends StatelessWidget {
  const CompactionMarker({super.key, required this.item});

  final CompactionItem item;

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    final label =
        'COMPACTED ${formatTokens(item.tokensBefore)} TO '
        '${formatTokens(item.tokensAfter)} '
        '(${item.native ? 'NATIVE' : 'REQUESTED'})';
    return Row(
      children: [
        Expanded(
          child: Container(height: 1, color: t.p.info.withValues(alpha: 0.4)),
        ),
        Flexible(
          flex: 6,
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 10),
            child: Text(
              label,
              textAlign: TextAlign.center,
              style: t.legend(t.p.info),
            ),
          ),
        ),
        Expanded(
          child: Container(height: 1, color: t.p.info.withValues(alpha: 0.4)),
        ),
      ],
    );
  }
}

class HandoffMarker extends StatelessWidget {
  const HandoffMarker({super.key, required this.item});

  final HandoffItem item;

  @override
  Widget build(BuildContext context) {
    final h = item.handoff;
    final t = Tokens.of(context);
    final by = h.summarizerHarness.isEmpty
        ? 'NO SUMMARY'
        : h.summarizerLocal
        ? 'SUMMARIZED BY LOCAL MODEL'
        : 'SUMMARIZED BY ${h.summarizerHarness.toUpperCase()}';
    final tone = h.summarizerLocal || h.summarizerHarness.isEmpty
        ? t.p.warn
        : t.p.info;
    return Tooltip(
      message: h.summarizerHarness.isEmpty
          ? 'Handoff with facts only: ${h.summaryError}'
          : 'Handoff ${h.path}, summary by ${h.summarizerHarness} '
                    '${h.summarizerProvider} ${h.summarizerModel}'
                .trim(),
      child: Row(
        children: [
          Expanded(
            child: Container(height: 1, color: tone.withValues(alpha: 0.4)),
          ),
          Flexible(
            flex: 6,
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 10),
              child: Text(
                'RESLEEVED ${h.fromHarness.toUpperCase()} TO '
                '${h.toHarness.toUpperCase()} ($by)',
                key: const ValueKey('handoff-marker'),
                textAlign: TextAlign.center,
                style: t.legend(tone),
              ),
            ),
          ),
          Expanded(
            child: Container(height: 1, color: tone.withValues(alpha: 0.4)),
          ),
        ],
      ),
    );
  }
}

class ErrorRow extends StatelessWidget {
  const ErrorRow({super.key, required this.item});

  final ErrorItem item;

  @override
  Widget build(BuildContext context) => NoteStrip(item.text, tone: Tone.alarm);
}

Widget buildChatItem(
  ChatItem item, {
  required void Function(String id, bool allow) onRespond,
}) => switch (item) {
  UserItem() => UserBubble(item: item),
  AssistantItem() => AssistantMessage(item: item),
  ToolItem() => ToolCard(item: item),
  ApprovalItem() => ApprovalCard(
    item: item,
    onRespond: (allow) => onRespond(item.id, allow),
  ),
  CompactionItem() => CompactionMarker(item: item),
  ErrorItem() => ErrorRow(item: item),
  HandoffItem() => HandoffMarker(item: item),
  ReportItem() => NoteStrip(
    'REPORT ${item.result.status.name.replaceFirst('REPORT_STATUS_', '')}\n${item.result.summary}',
    tone: item.result.status == ReportStatus.REPORT_STATUS_COMPLETE
        ? Tone.info
        : Tone.warn,
  ),
};
