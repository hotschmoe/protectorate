import 'package:flutter/material.dart';

import '../sleeves/sleeve_info.dart';
import '../theme/tokens.dart';
import 'chat_model.dart';

class UserBubble extends StatelessWidget {
  const UserBubble({super.key, required this.item});

  final UserItem item;

  @override
  Widget build(BuildContext context) {
    final cs = Theme.of(context).colorScheme;
    final hub = item.from == 'hub';
    return Align(
      alignment: Alignment.centerRight,
      child: ConstrainedBox(
        constraints: BoxConstraints(
          maxWidth: MediaQuery.sizeOf(context).width * 0.85,
        ),
        child: Container(
          padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 10),
          decoration: BoxDecoration(
            color: hub ? cs.surfaceContainerHighest : cs.primaryContainer,
            borderRadius: BorderRadius.circular(Tokens.radius),
          ),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            mainAxisSize: MainAxisSize.min,
            children: [
              if (item.from.isNotEmpty && item.from != 'owner')
                Text(
                  item.from,
                  style: Theme.of(
                    context,
                  ).textTheme.labelSmall?.copyWith(color: cs.onSurfaceVariant),
                ),
              SelectableText(
                item.text,
                style: TextStyle(
                  color: hub ? cs.onSurface : cs.onPrimaryContainer,
                ),
              ),
            ],
          ),
        ),
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
    final cs = Theme.of(context).colorScheme;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        for (final seg in splitFences(item.text))
          if (seg.code)
            Container(
              width: double.infinity,
              margin: const EdgeInsets.symmetric(vertical: 4),
              padding: const EdgeInsets.all(10),
              decoration: BoxDecoration(
                color: cs.surfaceContainerHighest,
                borderRadius: BorderRadius.circular(Tokens.smallRadius),
              ),
              child: SingleChildScrollView(
                scrollDirection: Axis.horizontal,
                child: SelectableText(
                  seg.text,
                  style: TextStyle(
                    fontFamily: monoFamily,
                    fontSize: 12.5,
                    color: cs.onSurface,
                  ),
                ),
              ),
            )
          else
            Padding(
              padding: const EdgeInsets.symmetric(vertical: 2),
              child: SelectableText(
                seg.text,
                style: TextStyle(height: 1.4, color: cs.onSurface),
              ),
            ),
      ],
    );
  }
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
    final cs = Theme.of(context).colorScheme;
    final style = TextStyle(
      color: cs.onSurfaceVariant,
      fontStyle: FontStyle.italic,
    );
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        InkWell(
          key: const ValueKey('thinking-toggle'),
          borderRadius: BorderRadius.circular(Tokens.smallRadius),
          onTap: () => setState(() => _open = !_open),
          child: Padding(
            padding: const EdgeInsets.symmetric(vertical: 4),
            child: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                Icon(
                  _open ? Icons.expand_more : Icons.chevron_right,
                  size: 18,
                  color: cs.onSurfaceVariant,
                ),
                Text('Thinking', style: style),
              ],
            ),
          ),
        ),
        if (_open)
          Padding(
            padding: const EdgeInsets.only(left: 18, bottom: 4),
            child: SelectableText(widget.item.text, style: style),
          ),
      ],
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

  Widget _mono(BuildContext context, String text) {
    final cs = Theme.of(context).colorScheme;
    return Container(
      width: double.infinity,
      constraints: const BoxConstraints(maxHeight: 280),
      margin: const EdgeInsets.only(top: 6),
      padding: const EdgeInsets.all(8),
      decoration: BoxDecoration(
        color: cs.surfaceContainerHighest,
        borderRadius: BorderRadius.circular(6),
      ),
      child: SingleChildScrollView(
        child: SingleChildScrollView(
          scrollDirection: Axis.horizontal,
          child: SelectableText(
            text,
            style: TextStyle(
              fontFamily: monoFamily,
              fontSize: 12,
              color: cs.onSurface,
            ),
          ),
        ),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final cs = Theme.of(context).colorScheme;
    final text = Theme.of(context).textTheme;
    final it = widget.item;
    final Widget status = !it.done
        ? const SizedBox(
            width: 16,
            height: 16,
            child: CircularProgressIndicator(strokeWidth: 2),
          )
        : it.isError
        ? Icon(Icons.error_outline, size: 18, color: cs.error)
        : Icon(Icons.check_circle_outline, size: 18, color: cs.primary);
    return Card(
      key: ValueKey('tool-${it.id}'),
      margin: EdgeInsets.zero,
      elevation: 0,
      color: cs.surfaceContainerLow,
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(Tokens.smallRadius + 2),
        side: BorderSide(color: it.isError ? cs.error : cs.outlineVariant),
      ),
      child: InkWell(
        borderRadius: BorderRadius.circular(Tokens.smallRadius + 2),
        onTap: () => setState(() => _open = !_open),
        child: Padding(
          padding: const EdgeInsets.all(10),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Row(
                children: [
                  Icon(
                    Icons.build_outlined,
                    size: 16,
                    color: cs.onSurfaceVariant,
                  ),
                  const SizedBox(width: 6),
                  Text(
                    it.name,
                    style: text.labelLarge?.copyWith(
                      fontWeight: FontWeight.w700,
                    ),
                  ),
                  const SizedBox(width: 8),
                  Expanded(
                    child: Text(
                      toolSummary(it.name, it.inputJson),
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: TextStyle(
                        fontFamily: monoFamily,
                        fontSize: 12,
                        color: cs.onSurfaceVariant,
                      ),
                    ),
                  ),
                  const SizedBox(width: 8),
                  status,
                  Icon(
                    _open ? Icons.expand_less : Icons.expand_more,
                    size: 18,
                    color: cs.onSurfaceVariant,
                  ),
                ],
              ),
              if (_open) ...[
                if (it.inputJson.isNotEmpty)
                  _mono(context, prettyJson(it.inputJson)),
                if (it.done)
                  _mono(context, it.output.isEmpty ? '(no output)' : it.output),
                if (it.truncated)
                  Padding(
                    padding: const EdgeInsets.only(top: 4),
                    child: Text(
                      'Output truncated',
                      style: text.labelSmall?.copyWith(color: cs.tertiary),
                    ),
                  ),
              ],
            ],
          ),
        ),
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
    final cs = Theme.of(context).colorScheme;
    final t = Tokens.of(context);
    final text = Theme.of(context).textTheme;
    return Container(
      key: ValueKey('approval-${item.id}'),
      padding: const EdgeInsets.all(12),
      decoration: BoxDecoration(
        color: item.resolved ? cs.surfaceContainerLow : t.warning.bg,
        borderRadius: BorderRadius.circular(Tokens.radius),
        border: Border.all(
          color: item.resolved ? cs.outlineVariant : t.approvalBorder,
          width: item.resolved ? 1 : 2,
        ),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Icon(
                item.resolved
                    ? (item.allowed ? Icons.check : Icons.block)
                    : Icons.shield_outlined,
                size: 18,
                color: item.resolved ? cs.onSurfaceVariant : t.warning.fg,
              ),
              const SizedBox(width: 6),
              Expanded(
                child: Text(
                  item.resolved
                      ? '${item.allowed ? 'Allowed' : 'Denied'}'
                            '${item.by.isEmpty ? '' : ' by ${item.by}'}'
                      : 'Approval needed${item.kind.isEmpty ? '' : ': ${item.kind}'}',
                  style: text.labelLarge?.copyWith(
                    fontWeight: FontWeight.w700,
                    color: item.resolved ? cs.onSurfaceVariant : t.warning.fg,
                  ),
                ),
              ),
            ],
          ),
          const SizedBox(height: 6),
          SelectableText(
            item.summary,
            style: TextStyle(
              fontFamily: monoFamily,
              fontSize: 12.5,
              color: item.resolved ? cs.onSurfaceVariant : t.warning.fg,
            ),
          ),
          if (!item.resolved) ...[
            const SizedBox(height: 10),
            Row(
              children: [
                FilledButton(
                  key: ValueKey('allow-${item.id}'),
                  onPressed: () => onRespond(true),
                  child: const Text('Allow'),
                ),
                const SizedBox(width: 8),
                OutlinedButton(
                  key: ValueKey('deny-${item.id}'),
                  onPressed: () => onRespond(false),
                  child: const Text('Deny'),
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
    final cs = Theme.of(context).colorScheme;
    final label =
        'Compacted ${formatTokens(item.tokensBefore)} to '
        '${formatTokens(item.tokensAfter)} tokens '
        '(${item.native ? 'native' : 'requested'})';
    return Row(
      children: [
        const Expanded(child: Divider()),
        Padding(
          padding: const EdgeInsets.symmetric(horizontal: 10),
          child: Text(
            label,
            style: Theme.of(
              context,
            ).textTheme.labelSmall?.copyWith(color: cs.onSurfaceVariant),
          ),
        ),
        const Expanded(child: Divider()),
      ],
    );
  }
}

class ErrorRow extends StatelessWidget {
  const ErrorRow({super.key, required this.item});

  final ErrorItem item;

  @override
  Widget build(BuildContext context) {
    final cs = Theme.of(context).colorScheme;
    return Container(
      padding: const EdgeInsets.all(10),
      decoration: BoxDecoration(
        color: cs.errorContainer,
        borderRadius: BorderRadius.circular(Tokens.smallRadius),
      ),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Icon(Icons.error_outline, size: 18, color: cs.onErrorContainer),
          const SizedBox(width: 8),
          Expanded(
            child: SelectableText(
              item.text,
              style: TextStyle(color: cs.onErrorContainer),
            ),
          ),
        ],
      ),
    );
  }
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
};
