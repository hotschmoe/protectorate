import 'package:flutter/material.dart';

import '../gen/protectorate/v1/chat.pb.dart';
import '../gen/protectorate/v1/sleeve.pb.dart';
import '../nodes/node_view.dart';
import '../sleeves/sleeve_info.dart';
import '../theme/tokens.dart';

class StatusPill extends StatelessWidget {
  const StatusPill({super.key, required this.state, this.label});

  final ChatState state;
  final String? label;

  @override
  Widget build(BuildContext context) {
    final a = Tokens.of(context).chatState(state);
    final working =
        state == ChatState.CHAT_STATE_WORKING ||
        state == ChatState.CHAT_STATE_COMPACTING;
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 4),
      decoration: BoxDecoration(
        color: a.bg,
        borderRadius: BorderRadius.circular(99),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          working
              ? SizedBox(
                  width: 10,
                  height: 10,
                  child: CircularProgressIndicator(strokeWidth: 2, color: a.fg),
                )
              : Container(
                  width: 8,
                  height: 8,
                  decoration: BoxDecoration(
                    color: a.fg,
                    shape: BoxShape.circle,
                  ),
                ),
          const SizedBox(width: 6),
          Text(
            label ?? stateLabel(state),
            style: Theme.of(context).textTheme.labelMedium?.copyWith(
              color: a.fg,
              fontWeight: FontWeight.w700,
            ),
          ),
        ],
      ),
    );
  }
}

class HarnessBadge extends StatelessWidget {
  const HarnessBadge({super.key, required this.harness});

  final Harness harness;

  @override
  Widget build(BuildContext context) {
    final a = Tokens.of(context).harness(harness);
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 3),
      decoration: BoxDecoration(
        color: a.bg,
        borderRadius: BorderRadius.circular(Tokens.smallRadius),
      ),
      child: Text(
        harnessLabel(harness),
        style: Theme.of(context).textTheme.labelMedium?.copyWith(
          color: a.fg,
          fontWeight: FontWeight.w700,
        ),
      ),
    );
  }
}

class LocalModelBadge extends StatelessWidget {
  const LocalModelBadge({super.key});

  @override
  Widget build(BuildContext context) {
    final a = Tokens.of(context).warning;
    return Tooltip(
      message:
          'A local model is active. Expect weaker results than a '
          'frontier model.',
      child: Container(
        key: const ValueKey('local-model-badge'),
        padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 3),
        decoration: BoxDecoration(
          color: a.bg,
          borderRadius: BorderRadius.circular(Tokens.smallRadius),
          border: Border.all(color: a.fg),
        ),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(Icons.warning_amber_rounded, size: 14, color: a.fg),
            const SizedBox(width: 4),
            Text(
              'Local model',
              style: Theme.of(context).textTheme.labelMedium?.copyWith(
                color: a.fg,
                fontWeight: FontWeight.w700,
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class ContextMeter extends StatelessWidget {
  const ContextMeter({
    super.key,
    required this.tokens,
    required this.window,
    this.width = 96,
  });

  final int tokens;
  final int window;
  final double width;

  @override
  Widget build(BuildContext context) {
    final cs = Theme.of(context).colorScheme;
    final style = Theme.of(
      context,
    ).textTheme.labelSmall!.copyWith(color: cs.onSurfaceVariant);
    if (window <= 0) {
      return Text('ctx ${formatTokens(tokens)}', style: style);
    }
    final pct = tokens * 100 / window;
    final sev = severityFor(context, pct);
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        Text('ctx', style: style),
        const SizedBox(width: 6),
        ClipRRect(
          borderRadius: BorderRadius.circular(3),
          child: SizedBox(
            width: width,
            height: 6,
            child: LinearProgressIndicator(
              key: const ValueKey('context-bar'),
              value: (pct / 100).clamp(0.0, 1.0),
              color: sev.fill,
              backgroundColor: cs.surfaceContainerHighest,
            ),
          ),
        ),
        const SizedBox(width: 6),
        Text('${formatTokens(tokens)} / ${formatTokens(window)}', style: style),
      ],
    );
  }
}
