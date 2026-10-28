import 'package:flutter/material.dart';

import '../gen/protectorate/v1/chat.pb.dart';
import '../gen/protectorate/v1/sleeve.pb.dart';
import '../format.dart';
import '../sleeves/sleeve_info.dart';
import '../theme/tokens.dart';
import '../vfd/lamp.dart';
import '../vfd/meter.dart';

class StateLamp extends StatelessWidget {
  const StateLamp({
    super.key,
    required this.state,
    this.label,
    this.big = false,
  });

  final ChatState state;
  final String? label;
  final bool big;

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    return LampLegend(
      label: (label ?? stateLabel(state)).toUpperCase(),
      tone: t.chatTone(state),
      on: state != ChatState.CHAT_STATE_EXITED,
      blink: state == ChatState.CHAT_STATE_AWAITING_APPROVAL,
      big: big,
    );
  }
}

class HarnessBadge extends StatelessWidget {
  const HarnessBadge({
    super.key,
    required this.harness,
    this.provider = '',
    this.local = false,
  });

  final Harness harness;
  final String provider;
  final bool local;

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    final a = t.model(harness, provider, local: local);
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
      decoration: BoxDecoration(
        color: a.bg,
        border: Border.all(color: a.fg.withValues(alpha: 0.7)),
        borderRadius: BorderRadius.circular(Tokens.innerRadius),
      ),
      child: Text(harnessLabel(harness).toUpperCase(), style: t.legend(a.fg)),
    );
  }
}

class LocalModelBadge extends StatelessWidget {
  const LocalModelBadge({super.key});

  @override
  Widget build(BuildContext context) {
    return const Tooltip(
      message:
          'This model is served from your own machine or network. Expect '
          'weaker results than a frontier model.',
      child: Annunciator(
        key: ValueKey('local-model-badge'),
        label: 'LOCAL MODEL',
        tone: Tone.warn,
      ),
    );
  }
}

class ContextMeter extends StatelessWidget {
  const ContextMeter({
    super.key,
    required this.tokens,
    required this.window,
    this.width = 120,
  });

  final int tokens;
  final int window;
  final double? width;

  @override
  Widget build(BuildContext context) {
    final t = Tokens.of(context);
    final label = Text('CTX', style: t.legend());
    if (window <= 0) {
      return Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          label,
          const SizedBox(width: 8),
          Text(formatTokens(tokens), style: t.data(t.p.emph)),
        ],
      );
    }
    final pct = tokens * 100 / window;
    final value = '${formatTokens(tokens)} / ${formatTokens(window)}';
    if (width == null) {
      return MeterRow(
        meterKey: const ValueKey('context-bar'),
        label: 'CTX',
        labelChars: 3,
        percent: pct,
        value: value,
        valueChars: 17,
      );
    }
    final meter = SegmentMeter(
      key: const ValueKey('context-bar'),
      percent: pct,
      height: 8,
    );
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        label,
        const SizedBox(width: 8),
        SizedBox(width: width, child: meter),
        const SizedBox(width: 8),
        Text(
          value,
          style: t.data(t.level(pct) == t.p.lit ? t.p.emph : t.level(pct)),
        ),
      ],
    );
  }
}

class LocalSummaryBadge extends StatelessWidget {
  const LocalSummaryBadge({super.key});

  @override
  Widget build(BuildContext context) {
    return const Tooltip(
      message:
          'The handoff this harness started from was summarized by a local '
          'model, because no subscription had spare quota. Check it against '
          'the facts in .protectorate/HANDOFF.md.',
      child: Annunciator(
        key: ValueKey('local-summary-badge'),
        label: 'LOCAL SUMMARY',
        tone: Tone.warn,
      ),
    );
  }
}

// Whether the provider's prompt cache is still warm, so the owner knows when a
// message is cheap. Claude reports when the cache runs out.
class CacheLamp extends StatelessWidget {
  const CacheLamp({super.key, required this.ratio, required this.cold});

  final double ratio;
  final bool cold;

  @override
  Widget build(BuildContext context) {
    final pct = (ratio * 100).round();
    return LampLegend(
      key: const ValueKey('cache-lamp'),
      label: cold ? 'CACHE MISS $pct%' : 'CACHE $pct%',
      tone: cold ? Tone.warn : (pct >= 50 ? Tone.ok : Tone.dim),
    );
  }
}
